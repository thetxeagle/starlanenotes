import { api, getApiBase } from "@/lib/api";
import {
  createLocalVaultAdapter,
  LOCAL_VAULT_SCHEMA_VERSION,
  type LocalVaultSnapshot,
  type LocalVaultSyncState,
  type VaultAsset,
  type VaultDocumentWorkflowEvent,
  type VaultDocumentWorkflow,
  type VaultDocument,
  type VaultDocTag,
  type VaultFolder,
  type VaultPropertySchema,
  type VaultPropertyValue,
  type VaultQueuedSyncOperation,
  type VaultSearchIndexEntry,
  type VaultTag,
  type VaultTemplate,
  type VaultWorkflowTemplate,
} from "@/lib/localVault";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { getSyncProvider, type SyncProviderKind } from "@/lib/storageProviders";
import {
  buildProviderRemoteImportApplyPlan,
  buildProviderRemoteImportDryRun,
  createProviderAdapter,
  executeProviderRemoteImportApplyPlan,
  type ProviderRemoteImportApplyExecutionResult,
  type ProviderRemoteImportApplyPlanOperation,
  type ProviderTransportOptions,
  type ProviderRemoteMutationPlan,
  type ProviderRemoteSyncManifest,
  type ProviderStatus,
  type ProviderSyncResult,
} from "@/lib/providerAdapters";
import { createGoogleDriveApiFetchTransport, readProviderSession } from "@/lib/providerSession";
import { filterDataviewDocuments, type DataviewDocument } from "@/lib/dataview";
import {
  readSharedSpaceDownloadCheckpoints,
  type SharedSpaceDownloadCheckpoint,
} from "@/lib/sharedSpaceDownloads";
import {
  buildNativeLocalDatabaseReplayContract,
  NATIVE_LOCAL_DATABASE_COMMANDS,
  preflightReplayOperationsForContract,
  sortReplayOperationsForContract,
  type NativeLocalDatabaseReplayContract,
  type NativeLocalDatabaseReplayReadiness,
} from "@/lib/nativeLocalDatabase";
import {
  clearConflicts,
  getConflict,
  hasPendingConflicts,
  listConflicts as listStoredConflicts,
  queueConflict,
  resolveConflictRecord,
  type ConflictResolutionStrategy,
  type DocumentConflictRecord,
} from "@/lib/conflicts";

const MAX_DOCUMENT_CONTENT_BYTES = 5 * 1024 * 1024;
const MAX_LOCAL_FOLDER_DEPTH = 3;
const PERSONAL_SPACE_NAME = "Personal Space";
const LOCAL_SYSTEM_USER_ID = "local-user";
const LOCAL_SYSTEM_USER = "Local User";
const LOCAL_SYSTEM_ROLE = "owner";
const RESETTABLE_SYNC_PROVIDERS: SyncProviderKind[] = ["none", "icloud-drive", "google-drive"];

export interface BookmarkMetadataRecord {
  url: string;
  title: string;
  description?: string;
  site_name?: string;
  image_url?: string;
}

const UNAVAILABLE_PROVIDER_STATUS: ProviderStatus = {
  connected: false,
  remote_contract: {
    status: "not_applicable",
    root_strategy: "server-api",
    auth_model: "server-session",
    required_scopes: [],
    write_model: "online-server-api",
    conflict_model: "server-managed",
  },
  remote_enablement: {
    provider: "none",
    status: "not_applicable",
    transport_kind: "none",
    enablement_boundary: "none",
    default_sync_enabled: false,
    required_scopes: [],
    blockers: [],
    steps: [],
  },
  remote_sync_enablement: {
    provider: "none",
    enabled: false,
    enablement_boundary: "explicit_manual_provider_session",
  },
  default_sync_policy: {
    provider: "none",
    status: "not_applicable",
    default_sync_enabled: false,
    automatic_enablement_allowed: false,
    policy_boundary: "none",
    readiness_gates: [],
    blockers: [],
    required_user_actions: [],
    validation_evidence: {
      production_oauth_verified: false,
      device_background_wake_verified: false,
      notes: [],
    },
  },
  remote_mutation_plans: [],
};

export interface SpaceRecord {
  id: string;
  name: string;
  slug: string;
  icon?: string;
  accent_color?: string;
  visibility?: string;
  member_role?: string;
  created_by?: string;
  owner_username?: string;
  owner_display_name?: string;
}

export interface FolderRecord {
  id: string;
  name: string;
  icon?: string;
  parent_id?: string;
}

export interface DocumentRecord {
  id: string;
  title: string;
  icon?: string;
  folder_id?: string;
  parent_id?: string;
  cover_image?: string;
  locked?: boolean;
  locked_by?: string;
  created_by?: string;
  last_updated_by?: string;
  created_at?: string;
  updated_at?: string;
  public?: boolean;
  space_id?: string;
  workflow_template_id?: string;
  workflow_template_name?: string;
  workflow_stage_id?: string;
  workflow_stage_name?: string;
  workflow_status?: "active" | "completed" | "cancelled";
}

export interface ListDocumentsOptions {
  folder_id?: string | null;
  include_nested?: boolean;
  workflow_status?: "active" | "completed" | "cancelled";
  workflow_stage_id?: string;
  workflow_template_id?: string;
}

interface ImportIdentityOptions {
  source_id?: string;
  created_at?: string;
  updated_at?: string;
}

export interface DocumentContentRecord {
  id: string;
  title: string;
  content: string;
}

export interface BacklinkRecord {
  document_id: string;
  title: string;
  space_id: string;
  space_name: string;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export interface GraphNodeRecord {
  id: string;
  title: string;
  folder_id?: string | null;
  connection_count: number;
  orphan: boolean;
}

export interface GraphEdgeRecord {
  source: string;
  target: string;
  relation: string;
}

export interface GraphRecord {
  nodes: GraphNodeRecord[];
  edges: GraphEdgeRecord[];
}

export interface SearchResultRecord {
  document_id: string;
  title: string;
  space_id: string;
  space_name: string;
  excerpt: string;
  updated_at: string;
  author: string;
}

export interface SpaceMemberRecord {
  user_id: string;
  display_name: string;
  email: string;
  role: string;
}

export interface SearchFilters {
  title?: string;
  space_id?: string;
  folder?: string;
  tag?: string;
  property_name?: string;
  property_value?: string;
  has_attachment?: string;
  has_cover?: string;
  has_workflow?: string;
  has_tag?: string;
  has_property?: string;
  is_locked?: string;
  created_on?: string;
  updated_on?: string;
}

export interface NotificationRecord {
  id: string;
  notification_type: string;
  title: string;
  body?: string;
  source_user?: string;
  target_type?: string;
  target_id?: string;
  read: boolean;
  created_at: string;
}

export interface NotificationListOptions {
  limit?: number;
  before?: string;
  unreadOnly?: boolean;
}

export type NotificationChannel = "both" | "in_app" | "email" | "none";

export interface NotificationPreferenceRecord {
  notification_type: string;
  label: string;
  channel: NotificationChannel;
}

export interface MoveFolderInput {
  target_space_id: string;
  parent_id?: string | null;
}

export interface MoveDocumentInput {
  target_space_id: string;
  folder_id?: string | null;
  parent_id?: string | null;
}

export interface UserSessionRecord {
  id: string;
  device_name?: string;
  ip_address?: string;
  user_agent?: string;
  session_kind?: "browser" | "app";
  last_seen_at: string;
  expires_at: string;
  created_at: string;
}

export interface TotpSetupResponse {
  qr_code_url: string;
  recovery_codes: string[];
}

export interface TotpStatusResponse {
  totp_enabled: boolean;
}

export interface PasskeyRecord {
  id: string;
  name: string;
  created_at: string;
  last_used_at?: string | null;
}

export interface PasskeyChallengeResponse {
  challenge_id: string;
  public_key: unknown;
}

export interface PasskeyAuthResponse {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  user: {
    id: string;
    email: string;
    username?: string | null;
    display_name: string;
    avatar_url?: string | null;
    role: string;
    totp_enabled: boolean;
  };
}

export interface DocumentVersionRecord {
  id: string;
  version_number: number;
  lines_added: number;
  lines_removed: number;
  created_by: string;
  author_name: string;
  created_at: string;
}

export interface AttachmentRecord {
  id: string;
  filename: string;
  original_name: string;
  mime_type: string;
  size_bytes: number;
  url: string;
  created_at: string;
}

export interface DocumentTextAssetInput {
  name: string;
  mime_type?: string;
  text: string;
  space_id?: string;
  document_id?: string;
}

export interface RestoreDocumentVersionResponse {
  version_id: string;
  version_number: number;
}

export interface UserDirectoryRecord {
  id: string;
  email: string;
  display_name: string;
  avatar_url?: string | null;
}

export interface TemplateRecord {
  id: string;
  space_id: string;
  name: string;
  description?: string;
  content: string;
  properties?: unknown;
  tags: string[];
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface TemplateInput {
  name: string;
  description?: string;
  content: string;
  properties?: unknown;
  tags?: string[];
}

export interface WorkflowTemplateStageRecord {
  id: string;
  name: string;
  sort_order: number;
}

export interface WorkflowTemplateApproverRecord {
  id: string;
  approver_type: "user" | "role";
  approver_id: string;
  stage_id?: string;
}

export interface WorkflowTemplateRecord {
  id: string;
  space_id: string;
  name: string;
  description?: string;
  stages: WorkflowTemplateStageRecord[];
  approvers: WorkflowTemplateApproverRecord[];
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface WorkflowTemplateInput {
  name: string;
  description?: string;
  stages: Array<{ id?: string; name: string; sort_order?: number }>;
  approvers?: Array<{
    id?: string;
    approver_type: "user" | "role";
    approver_id: string;
    stage_id?: string;
  }>;
}

export interface DocumentWorkflowRecord {
  id: string;
  document_id: string;
  workflow_template_id: string;
  workflow_template_name: string;
  current_stage_id: string;
  current_stage_name: string;
  status: "active" | "completed" | "cancelled";
  stages: WorkflowTemplateStageRecord[];
  approvers: WorkflowTemplateApproverRecord[];
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface DocumentWorkflowEventRecord {
  id: string;
  document_workflow_id: string;
  document_id: string;
  from_stage_id?: string;
  to_stage_id: string;
  action: "transition" | "enable" | "disable" | "delegate" | "escalate";
  note?: string;
  created_by?: string;
  created_at: string;
}

export interface EnableDocumentWorkflowInput {
  template_id: string;
  stage_id?: string;
  source_id?: string;
  event_source_id?: string;
  event_created_by?: string;
  event_created_at?: string;
  created_at?: string;
  updated_at?: string;
}

export interface TransitionDocumentWorkflowInput {
  stage_id: string;
  note?: string;
}

export interface RecordDocumentWorkflowEventInput {
  action: VaultDocumentWorkflowEvent["action"];
  note?: string;
  source_id?: string;
  created_by?: string;
  created_at?: string;
  from_stage_id?: string;
  to_stage_id?: string;
}

interface NormalizedWorkflowTemplateInput {
  name: string;
  description?: string;
  stages: WorkflowTemplateStageRecord[];
  approvers: WorkflowTemplateApproverRecord[];
}

export interface PropertySchemaRecord {
  id: string;
  name: string;
  property_type: string;
  options?: unknown;
  sort_order: number;
}

export interface DocumentPropertyValueRecord {
  property_id: string;
  property_name: string;
  property_type: string;
  options?: unknown;
  value: string | null;
}

export interface PropertySchemaInput {
  name: string;
  property_type: string;
  options?: unknown;
}

export interface ReorderPropertySchemasInput {
  property_ids: string[];
}

export interface TagRecord {
  id: string;
  name: string;
  color: string;
  created_at?: string;
  doc_count?: number | null;
}

export interface TagInput {
  name: string;
  color?: string;
}

export type LocalAssetRecord = VaultAsset;

export interface LocalAssetContext {
  space_id?: string;
  document_id?: string;
}

export interface LocalVaultStatsRecord {
  spaces: number;
  folders: number;
  documents: number;
  assets: number;
  asset_bytes: number;
}

export interface LocalSyncReplayOperationRecord extends VaultQueuedSyncOperation {
  payload_status: "available" | "missing";
  payload: unknown;
}

export interface LocalSyncReplayPreflightSummaryRecord {
  total_operations: number;
  missing_payload_count: number;
  ready_operation_count: number;
  by_entity_type: Record<string, number>;
  by_action: Record<string, number>;
  unavailable_payloads: LocalSyncReplayUnavailablePayloadRecord[];
}

export interface LocalSyncReplayUnavailablePayloadRecord {
  operation_id: string;
  entity_type: string;
  entity_id: string;
  action: string;
  reason: string;
  asset_name?: string;
  document_id?: string;
  source_url?: string;
  vault_path?: string;
  byte_sources_checked?: string[];
  recovery_action?: string;
}

export interface LocalSyncReplayManifestRecord {
  schema_version: number;
  exported_at: string;
  provider: SyncProviderKind;
  replay_contract: NativeLocalDatabaseReplayContract;
  replay_readiness: NativeLocalDatabaseReplayReadiness;
  operation_count: number;
  operation_summary: LocalSyncReplayPreflightSummaryRecord;
  recovery_checkpoint_count: number;
  recovery_checkpoints: SharedSpaceDownloadCheckpoint[];
  operations: LocalSyncReplayOperationRecord[];
}

export interface LocalSyncReplayOperationResultRecord {
  operation_id: string;
  entity_type: string;
  action: string;
  status: "applied" | "skipped" | "failed" | "unsupported" | string;
  message: string;
}

export interface LocalSyncReplayServerResultRecord {
  status: string;
  attempted_operation_count: number;
  applied_operation_count: number;
  skipped_operation_count: number;
  failed_operation_count: number;
  unsupported_operation_count: number;
  results: LocalSyncReplayOperationResultRecord[];
}

export interface LocalSyncReplayServerPreflightRecord {
  status: string;
  provider: string;
  ready: boolean;
  operation_count: number;
  ready_operation_count: number;
  blocked_operation_count: number;
  recovery_checkpoint_count: number;
  target_backends: string[];
  blockers: string[];
}

export interface LocalSyncReplayHistoryCheckpointRecord {
  id: string;
  space_id?: string | null;
  source_server_url?: string | null;
  downloaded_by_user_id?: string | null;
  checkpoint: unknown;
  created_at: string;
}

export interface LocalSyncReplayHistoryManifestRecord {
  id: string;
  provider: string;
  status: string;
  attempted_by: string;
  operation_count: number;
  attempted_operation_count: number;
  applied_operation_count: number;
  skipped_operation_count: number;
  failed_operation_count: number;
  unsupported_operation_count: number;
  recovery_checkpoint_count: number;
  created_at: string;
  checkpoints: LocalSyncReplayHistoryCheckpointRecord[];
}

export interface LocalSyncReplayHistoryRecord {
  manifests: LocalSyncReplayHistoryManifestRecord[];
}

export interface LocalProviderSyncReadinessRecord {
  provider: SyncProviderKind;
  connected: boolean;
  ready: boolean;
  sync_state: LocalVaultSyncState;
  queued_operation_count: number;
  ready_replay_operation_count: number;
  missing_payload_count: number;
  pending_conflict_count: number;
  blockers: string[];
  remote_mutation_plans: ProviderRemoteMutationPlan[];
}

export interface VaultMigrationSummaryRecord {
  spaces: number;
  folders: number;
  documents: number;
  assets: number;
  templates: number;
  workflow_templates: number;
  document_workflows: number;
  property_schemas: number;
  property_values: number;
  tags: number;
  doc_tags: number;
  warnings: string[];
}

export interface VaultMirrorReconciliationRecord {
  files_scanned: number;
  documents_updated: number;
  templates_updated: number;
  documents_imported?: number;
  templates_imported?: number;
}

function assertLocalVaultSnapshot(value: unknown): asserts value is LocalVaultSnapshot {
  if (!value || typeof value !== "object") {
    throw new Error("Invalid vault file");
  }

  const snapshot = value as Partial<LocalVaultSnapshot>;
  if (snapshot.schema_version !== LOCAL_VAULT_SCHEMA_VERSION) {
    throw new Error("Unsupported vault schema version");
  }
  if (
    !Array.isArray(snapshot.spaces) ||
    !Array.isArray(snapshot.folders) ||
    !Array.isArray(snapshot.documents)
  ) {
    throw new Error("Vault file is missing required collections");
  }
}

function normalizeLocalVaultSnapshot(snapshot: LocalVaultSnapshot): LocalVaultSnapshot {
  return {
    schema_version: LOCAL_VAULT_SCHEMA_VERSION,
    spaces: snapshot.spaces.map(normalizeVaultSpace),
    folders: snapshot.folders,
    documents: snapshot.documents.map(normalizeVaultDocument),
    assets: Array.isArray(snapshot.assets) ? snapshot.assets : [],
    templates: Array.isArray(snapshot.templates) ? snapshot.templates : [],
    workflow_templates: Array.isArray(snapshot.workflow_templates)
      ? snapshot.workflow_templates
      : [],
    document_workflows: Array.isArray(snapshot.document_workflows)
      ? snapshot.document_workflows
      : [],
    document_workflow_events: Array.isArray(snapshot.document_workflow_events)
      ? snapshot.document_workflow_events
      : [],
    property_schemas: Array.isArray(snapshot.property_schemas) ? snapshot.property_schemas : [],
    property_values: Array.isArray(snapshot.property_values) ? snapshot.property_values : [],
    tags: Array.isArray(snapshot.tags) ? snapshot.tags : [],
    doc_tags: Array.isArray(snapshot.doc_tags) ? snapshot.doc_tags : [],
    search_index: Array.isArray(snapshot.search_index) ? snapshot.search_index : [],
    sync_state: snapshot.sync_state,
  };
}

function normalizeVaultSpace(space: LocalVaultSnapshot["spaces"][number]) {
  const isLocalPersonalSpace = space.id === "local-space";
  return {
    ...space,
    name: isLocalPersonalSpace && space.name === "Local Space" ? PERSONAL_SPACE_NAME : space.name,
    slug: space.slug ?? space.id,
    owner_username:
      space.owner_username ?? (isLocalPersonalSpace ? LOCAL_SYSTEM_USER_ID : undefined),
    owner_display_name:
      space.owner_display_name ?? (isLocalPersonalSpace ? LOCAL_SYSTEM_USER : undefined),
    created_by: space.created_by ?? (isLocalPersonalSpace ? LOCAL_SYSTEM_USER_ID : undefined),
  };
}

function summarizeVaultSnapshot(snapshot: LocalVaultSnapshot): VaultMigrationSummaryRecord {
  const warnings: string[] = [];
  if (snapshot.assets.length > 0) {
    warnings.push(
      "Asset binaries are uploaded during server migration only when binary data and a document reference are available.",
    );
  }
  if ((snapshot.document_workflows ?? []).length > 0) {
    warnings.push(
      "Active workflow state can be mapped during server migration, but original event authorship and non-handoff workflow history are not preserved yet.",
    );
  }
  return {
    spaces: snapshot.spaces.length,
    folders: snapshot.folders.length,
    documents: snapshot.documents.length,
    assets: snapshot.assets.length,
    templates: snapshot.templates.length,
    workflow_templates: snapshot.workflow_templates?.length ?? 0,
    document_workflows: snapshot.document_workflows?.length ?? 0,
    property_schemas: snapshot.property_schemas?.length ?? 0,
    property_values: snapshot.property_values?.length ?? 0,
    tags: snapshot.tags?.length ?? 0,
    doc_tags: snapshot.doc_tags?.length ?? 0,
    warnings,
  };
}

function dataUrlToBlob(dataUrl: string, fallbackMime?: string): Blob | null {
  const [header, payload] = dataUrl.split(",", 2);
  if (!header?.startsWith("data:") || payload === undefined) return null;
  const mediaType =
    header.slice("data:".length).split(";")[0] || fallbackMime || "application/octet-stream";
  const isBase64 = header.split(";").includes("base64");
  try {
    const binary = isBase64 ? atob(payload) : decodeURIComponent(payload);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return new Blob([bytes], { type: mediaType });
  } catch {
    return null;
  }
}

function textToDataUrl(text: string, mimeType = "text/plain"): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    const chunk = bytes.slice(index, index + chunkSize);
    binary += String.fromCharCode(...chunk);
  }
  return `data:${mimeType};base64,${btoa(binary)}`;
}

async function vaultAssetToBlob(asset: VaultAsset): Promise<Blob | null> {
  if (asset.url.startsWith("data:")) {
    return dataUrlToBlob(asset.url, asset.mime_type);
  }
  try {
    const response = await fetch(asset.url);
    if (!response.ok) return null;
    return response.blob();
  } catch {
    return null;
  }
}

async function vaultAssetToText(asset: VaultAsset): Promise<string | null> {
  const blob = await vaultAssetToBlob(asset);
  if (!blob) return null;
  try {
    return blob.text();
  } catch {
    return null;
  }
}

function newestByCreatedAt<T extends { created_at?: string }>(items: T[]): T | undefined {
  return [...items].sort((left, right) => {
    const rightTime = Date.parse(right.created_at ?? "");
    const leftTime = Date.parse(left.created_at ?? "");
    return (Number.isFinite(rightTime) ? rightTime : 0) - (Number.isFinite(leftTime) ? leftTime : 0);
  })[0];
}

function resolveServerFileUrl(url: string): string {
  if (url.startsWith("/api/v1/")) {
    return `${getApiBase()}${url.slice("/api/v1".length)}`;
  }
  return url;
}

async function packNativeReplayAssetPayload(payload: unknown): Promise<unknown> {
  if (!payload || typeof payload !== "object") return payload;
  if (!canUseTauriNativeAssetImport()) return payload;

  const record = payload as Record<string, unknown>;
  if (hasReplayPayloadBytes(record)) return payload;

  const vaultPath = typeof record.vault_path === "string" ? record.vault_path : undefined;
  const url = typeof record.url === "string" ? record.url : undefined;
  if (!vaultPath && !url) return payload;

  try {
    const dataUrl = await invokeTauri<string>(NATIVE_LOCAL_DATABASE_COMMANDS.packReplayAsset, {
      provider: getSyncProvider(),
      vaultPath,
      url,
      mimeType: typeof record.mime_type === "string" ? record.mime_type : undefined,
    });
    return {
      ...record,
      data_url: dataUrl,
    };
  } catch (error) {
    return {
      ...record,
      native_pack_error:
        error instanceof Error ? error.message : "Native replay asset byte packing failed",
    };
  }
}

async function hydrateStaticSiteExportAssets(snapshot: LocalVaultSnapshot): Promise<LocalVaultSnapshot> {
  if (!canUseTauriNativeAssetImport()) return snapshot;
  const assets = await Promise.all(
    snapshot.assets.map(async (asset) => {
      if (asset.url.startsWith("data:") || (!asset.vault_path && !asset.url)) return asset;
      const packed = await packNativeReplayAssetPayload(asset);
      const dataUrl =
        packed && typeof packed === "object" && typeof (packed as { data_url?: unknown }).data_url === "string"
          ? (packed as { data_url: string }).data_url
          : undefined;
      return dataUrl ? { ...asset, url: dataUrl } : asset;
    }),
  );
  return { ...snapshot, assets };
}

function extractDocumentIdFromAssetPath(
  asset: VaultAsset,
  documents: VaultDocument[],
): string | undefined {
  const candidates = [asset.vault_path, asset.url]
    .filter((value): value is string => Boolean(value))
    .flatMap((value) => {
      let decoded = value;
      try {
        decoded = decodeURIComponent(value);
      } catch {
        decoded = value;
      }
      return [value, decoded];
    });

  for (const candidate of candidates) {
    const marker = "/attachments/";
    const index = candidate.indexOf(marker);
    if (index === -1) continue;
    const rest = candidate.slice(index + marker.length);
    const documentId = rest.split(/[\\/]/)[0];
    if (documents.some((document) => document.id === documentId)) {
      return documentId;
    }
  }

  const urlMatches = documents.filter((document) => document.content.includes(asset.url));
  if (urlMatches.length === 1) return urlMatches[0].id;

  const nameMatches = documents.filter((document) => document.content.includes(asset.name));
  if (nameMatches.length === 1) return nameMatches[0].id;

  return undefined;
}

interface LocalSpaceState {
  id: string;
  name: string;
  slug: string;
  owner_username?: string;
  owner_display_name?: string;
  created_by?: string;
  created_at: string;
  updated_at: string;
}

function localSpaceToRecord(space: LocalSpaceState): SpaceRecord {
  return {
    id: space.id,
    name: space.name,
    slug: space.slug,
    owner_username: space.owner_username,
    owner_display_name: space.owner_display_name,
    created_by: space.created_by,
  };
}

interface LocalDocState {
  id: string;
  space_id: string;
  title: string;
  content: string;
  icon?: string;
  cover_image?: string;
  locked?: boolean;
  folder_id?: string;
  parent_id?: string;
  created_by: string;
  last_updated_by: string;
  created_at: string;
  updated_at: string;
}

interface LocalFolderState {
  id: string;
  space_id: string;
  name: string;
  parent_id?: string;
}

const localVault = createLocalVaultAdapter(getSyncProvider());
const localSpaces = new Map<string, LocalSpaceState>();
const localDocs = new Map<string, LocalDocState>();
const localFolders = new Map<string, LocalFolderState>();
let localInitialized = false;
let localInitializationPromise: Promise<void> | null = null;

function assertValidEntityId(entity: string, value: string): void {
  if (!value || !value.trim()) {
    throw new Error(`Invalid ${entity} id`);
  }
}

function normalizeTitle(title: string): string {
  const normalized = title.trim();
  if (!normalized) throw new Error("Document title is required");
  return normalized.slice(0, 500);
}

function assertContentSize(content: string): void {
  const bytes = new TextEncoder().encode(content).length;
  if (bytes > MAX_DOCUMENT_CONTENT_BYTES) {
    throw new Error("Document content exceeds 5 MB limit");
  }
}

function makeId(prefix: string): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  return `${prefix}-${Math.random().toString(36).slice(2, 10)}`;
}

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

function canUseTauriNativeAssetImport(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function canUseTauriNativeVaultArchive(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriCoreModule })
    .__TAURI_INTERNALS__;
  if (internals?.invoke) return internals.invoke<T>(cmd, args);

  const coreModulePath = "@tauri-apps/api/core";
  const coreMod = (await import(/* @vite-ignore */ coreModulePath)) as unknown as TauriCoreModule;
  return coreMod.invoke<T>(cmd, args);
}

async function ensureLocalData(): Promise<void> {
  if (localInitialized) return;

  if (localInitializationPromise) {
    return localInitializationPromise;
  }

  localInitializationPromise = initializeLocalData();
  try {
    await localInitializationPromise;
  } finally {
    localInitializationPromise = null;
  }
}

async function initializeLocalData(): Promise<void> {
  if (localInitialized) return;

  await localVault.init();
  const snapshot = await localVault.loadSnapshot();
  const migratedPersonalSpaceName = snapshot.spaces.some(
    (space) => space.id === "local-space" && space.name === "Local Space",
  );
  const backfilledPersonalSpaceOwner = snapshot.spaces.some(
    (space) => space.id === "local-space" && (!space.owner_username || !space.owner_display_name),
  );
  hydrateFromSnapshot(snapshot);

  if (localSpaces.size === 0) {
    const now = new Date().toISOString();
    const spaceId = "local-space";
    const docId = "local-welcome";

    localSpaces.set(spaceId, {
      id: spaceId,
      name: PERSONAL_SPACE_NAME,
      slug: "local-space",
      owner_username: LOCAL_SYSTEM_USER_ID,
      owner_display_name: LOCAL_SYSTEM_USER,
      created_by: LOCAL_SYSTEM_USER_ID,
      created_at: now,
      updated_at: now,
    });

    localDocs.set(docId, {
      id: docId,
      space_id: spaceId,
      title: "Welcome",
      content: "# Welcome\n\nYou are using local mode. Your notes are stored locally.",
      created_by: LOCAL_SYSTEM_USER,
      last_updated_by: LOCAL_SYSTEM_USER,
      created_at: now,
      updated_at: now,
    });

    await persistLocalSnapshot();
  } else if (migratedPersonalSpaceName || backfilledPersonalSpaceOwner) {
    await persistLocalSnapshot();
  }

  localInitialized = true;
}

function buildSnapshot(): LocalVaultSnapshot {
  return {
    schema_version: LOCAL_VAULT_SCHEMA_VERSION,
    spaces: Array.from(localSpaces.values()).map((space) => ({
      id: space.id,
      name: space.name,
      slug: space.slug,
      owner_username: space.owner_username,
      owner_display_name: space.owner_display_name,
      created_by: space.created_by,
      created_at: space.created_at,
      updated_at: space.updated_at,
    })),
    folders: Array.from(localFolders.values()).map((folder) => ({
      id: folder.id,
      space_id: folder.space_id,
      name: folder.name,
      parent_id: folder.parent_id,
    })),
    documents: Array.from(localDocs.values()).map((doc) => ({
      id: doc.id,
      space_id: doc.space_id,
      title: doc.title,
      content: doc.content,
      icon: doc.icon,
      cover_image: doc.cover_image,
      locked: doc.locked,
      folder_id: doc.folder_id,
      parent_id: doc.parent_id,
      created_by: doc.created_by,
      last_updated_by: doc.last_updated_by,
      created_at: doc.created_at,
      updated_at: doc.updated_at,
    })),
    assets: snapshotAssetCache,
    templates: localTemplateCache,
    workflow_templates: localWorkflowTemplateCache,
    document_workflows: localDocumentWorkflowCache,
    document_workflow_events: localDocumentWorkflowEventCache,
    property_schemas: localPropertySchemas,
    property_values: localPropertyValues,
    tags: localTags,
    doc_tags: localDocTags,
    search_index: localSearchIndex,
  };
}

let snapshotAssetCache: VaultAsset[] = [];
let localTemplateCache: VaultTemplate[] = [];
let localWorkflowTemplateCache: VaultWorkflowTemplate[] = [];
let localDocumentWorkflowCache: VaultDocumentWorkflow[] = [];
let localDocumentWorkflowEventCache: VaultDocumentWorkflowEvent[] = [];
let localPropertySchemas: VaultPropertySchema[] = [];
let localPropertyValues: VaultPropertyValue[] = [];
let localTags: VaultTag[] = [];
let localDocTags: VaultDocTag[] = [];
let localSearchIndex: VaultSearchIndexEntry[] = [];

type LocalQueuedSyncOperationInput = Pick<
  VaultQueuedSyncOperation,
  "entity_type" | "entity_id" | "action" | "payload"
> &
  Partial<Pick<VaultQueuedSyncOperation, "queued_at">>;

function removeLocalDocumentMetadata(documentIds: Set<string>): void {
  if (documentIds.size === 0) return;
  localDocTags = localDocTags.filter((docTag) => !documentIds.has(docTag.document_id));
  localPropertyValues = localPropertyValues.filter(
    (value) => !documentIds.has(value.document_id),
  );
  localDocumentWorkflowCache = localDocumentWorkflowCache.filter(
    (workflow) => !documentIds.has(workflow.document_id),
  );
  localDocumentWorkflowEventCache = localDocumentWorkflowEventCache.filter(
    (event) => !documentIds.has(event.document_id),
  );
}

function removeLocalSpaceContents(spaceId: string): void {
  const documentIds = new Set(
    Array.from(localDocs.values())
      .filter((document) => document.space_id === spaceId)
      .map((document) => document.id),
  );
  for (const documentId of documentIds) localDocs.delete(documentId);
  for (const folder of Array.from(localFolders.values())) {
    if (folder.space_id === spaceId) localFolders.delete(folder.id);
  }
  removeLocalDocumentMetadata(documentIds);
  // Legacy assets may not have ownership metadata. Preserve those records rather
  // than risking cross-space deletion; new assets are removed by their space ID.
  snapshotAssetCache = snapshotAssetCache.filter((asset) => asset.space_id !== spaceId);
  localTemplateCache = localTemplateCache.filter((template) => template.space_id !== spaceId);
  localWorkflowTemplateCache = localWorkflowTemplateCache.filter(
    (template) => template.space_id !== spaceId,
  );
  localPropertySchemas = localPropertySchemas.filter((schema) => schema.space_id !== spaceId);
  localTags = localTags.filter((tag) => tag.space_id !== spaceId);
  localSpaces.delete(spaceId);
  localSearchIndex = buildLocalSearchIndexEntries();
}

function hydrateFromSnapshot(snapshot: LocalVaultSnapshot): void {
  localSpaces.clear();
  localFolders.clear();
  localDocs.clear();

  for (const space of snapshot.spaces) {
    const normalizedSpace = normalizeVaultSpace(space);
    localSpaces.set(space.id, {
      id: normalizedSpace.id,
      name: normalizedSpace.name,
      slug: normalizedSpace.slug,
      owner_username: normalizedSpace.owner_username,
      owner_display_name: normalizedSpace.owner_display_name,
      created_by: normalizedSpace.created_by,
      created_at: normalizedSpace.created_at,
      updated_at: normalizedSpace.updated_at,
    });
  }

  for (const folder of snapshot.folders) {
    localFolders.set(folder.id, {
      id: folder.id,
      space_id: folder.space_id,
      name: folder.name,
      parent_id: folder.parent_id,
    });
  }

  for (const doc of snapshot.documents) {
    const normalizedDoc = normalizeVaultDocument(doc);
    localDocs.set(doc.id, {
      id: normalizedDoc.id,
      space_id: normalizedDoc.space_id,
      title: normalizedDoc.title,
      content: normalizedDoc.content,
      icon: normalizedDoc.icon,
      cover_image: normalizedDoc.cover_image,
      locked: normalizedDoc.locked ?? false,
      folder_id: normalizedDoc.folder_id,
      parent_id: normalizedDoc.parent_id,
      created_by: normalizedDoc.created_by ?? LOCAL_SYSTEM_USER,
      last_updated_by: normalizedDoc.last_updated_by ?? LOCAL_SYSTEM_USER,
      created_at: normalizedDoc.created_at ?? normalizedDoc.updated_at,
      updated_at: normalizedDoc.updated_at,
    });
  }

  snapshotAssetCache = snapshot.assets;
  localTemplateCache = snapshot.templates ?? [];
  localWorkflowTemplateCache = snapshot.workflow_templates ?? [];
  localDocumentWorkflowCache = snapshot.document_workflows ?? [];
  localDocumentWorkflowEventCache = snapshot.document_workflow_events ?? [];
  localPropertySchemas = snapshot.property_schemas ?? [];
  localPropertyValues = snapshot.property_values ?? [];
  localTags = snapshot.tags ?? [];
  localDocTags = snapshot.doc_tags ?? [];
  localSearchIndex = snapshot.search_index ?? buildLocalSearchIndexEntries();
}

async function persistLocalSnapshot(operation?: LocalQueuedSyncOperationInput): Promise<void> {
  if (!isLocalSearchIndexFresh()) {
    localSearchIndex = buildLocalSearchIndexEntries();
  }
  const snapshot = buildSnapshot();
  if (operation) {
    await localVault.enqueueSyncOperation({
      ...operation,
      payload: operation.payload ?? findQueuedOperationPayload(snapshot, operation),
    });
  }
  snapshot.sync_state = (await localVault.loadSnapshot()).sync_state;
  await localVault.saveSnapshot(snapshot);
}

function buildLocalSearchIndexEntries(): VaultSearchIndexEntry[] {
  return Array.from(localDocs.values()).map((doc) => {
    const content = doc.content.replace(/\s+/g, " ").trim();
    return {
      document_id: doc.id,
      space_id: doc.space_id,
      title: doc.title,
      content,
      search_text: `${doc.title}\n${doc.content}`.toLowerCase(),
      updated_at: doc.updated_at,
    };
  });
}

function isLocalSearchIndexFresh(): boolean {
  if (localSearchIndex.length !== localDocs.size) return false;
  const indexedById = new Map(localSearchIndex.map((entry) => [entry.document_id, entry]));
  for (const doc of localDocs.values()) {
    const entry = indexedById.get(doc.id);
    if (
      !entry ||
      entry.space_id !== doc.space_id ||
      entry.title !== doc.title ||
      entry.search_text !== `${doc.title}\n${doc.content}`.toLowerCase() ||
      entry.updated_at !== doc.updated_at
    ) {
      return false;
    }
  }
  return true;
}

async function ensureLocalSearchIndex(): Promise<VaultSearchIndexEntry[]> {
  if (!isLocalSearchIndexFresh()) {
    localSearchIndex = buildLocalSearchIndexEntries();
    await persistLocalSnapshot();
  }
  return localSearchIndex;
}

function requireLocalSpace(spaceId: string): LocalSpaceState {
  assertValidEntityId("space", spaceId);
  const space = localSpaces.get(spaceId);
  if (!space) throw new Error("Space not found");
  return space;
}

function requireLocalFolder(spaceId: string, folderId: string): LocalFolderState {
  assertValidEntityId("folder", folderId);
  const folder = localFolders.get(folderId);
  if (!folder || folder.space_id !== spaceId) throw new Error("Folder not found");
  return folder;
}

function getLocalFolderDepth(spaceId: string, folderId: string, seen = new Set<string>()): number {
  if (seen.has(folderId)) {
    throw new Error("Folder hierarchy contains a cycle");
  }
  seen.add(folderId);
  const folder = requireLocalFolder(spaceId, folderId);
  if (!folder.parent_id) return 1;
  return getLocalFolderDepth(spaceId, folder.parent_id, seen) + 1;
}

function getLocalFolderSubtreeRelativeDepth(
  spaceId: string,
  folderId: string,
  seen = new Set<string>(),
): number {
  if (seen.has(folderId)) {
    throw new Error("Folder hierarchy contains a cycle");
  }
  seen.add(folderId);
  let maxDepth = 1;
  for (const candidate of localFolders.values()) {
    if (candidate.space_id !== spaceId || candidate.parent_id !== folderId) continue;
    maxDepth = Math.max(
      maxDepth,
      getLocalFolderSubtreeRelativeDepth(spaceId, candidate.id, new Set(seen)) + 1,
    );
  }
  return maxDepth;
}

function normalizeLocalFolderParentId(
  spaceId: string,
  parentId: string | null | undefined,
  movingFolderId?: string,
): string | undefined {
  if (!parentId) return undefined;
  const parent = requireLocalFolder(spaceId, parentId);
  if (movingFolderId) {
    if (parent.id === movingFolderId) {
      throw new Error("Folder cannot be moved inside itself");
    }
    let ancestorId = parent.parent_id;
    while (ancestorId) {
      if (ancestorId === movingFolderId) {
        throw new Error("Folder cannot be moved inside one of its descendants");
      }
      ancestorId = requireLocalFolder(spaceId, ancestorId).parent_id;
    }
  }
  const parentDepth = getLocalFolderDepth(spaceId, parent.id);
  const subtreeDepth = movingFolderId
    ? getLocalFolderSubtreeRelativeDepth(spaceId, movingFolderId)
    : 1;
  if (parentDepth + subtreeDepth > MAX_LOCAL_FOLDER_DEPTH) {
    throw new Error(`Folders can be nested up to ${MAX_LOCAL_FOLDER_DEPTH} levels`);
  }
  return parent.id;
}

function normalizeLocalDocumentFolderId(
  spaceId: string,
  folderId: string | null | undefined,
): string | undefined {
  if (!folderId) return undefined;
  const folder = requireLocalFolder(spaceId, folderId);
  if (getLocalFolderDepth(spaceId, folder.id) > MAX_LOCAL_FOLDER_DEPTH) {
    throw new Error(`Pages can be nested up to ${MAX_LOCAL_FOLDER_DEPTH} folder levels`);
  }
  return folder.id;
}

function getLocalDocumentDepth(spaceId: string, docId: string, seen = new Set<string>()): number {
  if (seen.has(docId)) throw new Error("Pages cannot contain a parent cycle");
  seen.add(docId);
  const doc = requireLocalDoc(spaceId, docId);
  return doc.parent_id ? getLocalDocumentDepth(spaceId, doc.parent_id, seen) + 1 : 1;
}

function normalizeLocalDocumentParentId(
  spaceId: string,
  parentId: string | null | undefined,
  folderId: string | undefined,
  movingDocumentId?: string,
): string | undefined {
  if (!parentId) return undefined;
  const parent = requireLocalDoc(spaceId, parentId);
  if (parent.folder_id !== folderId) {
    throw new Error("Child pages must stay in the same folder as their parent");
  }
  if (movingDocumentId) {
    if (parent.id === movingDocumentId) throw new Error("A page cannot be its own parent");
    let ancestorId = parent.parent_id;
    while (ancestorId) {
      if (ancestorId === movingDocumentId) throw new Error("A page cannot be moved inside its descendant");
      ancestorId = requireLocalDoc(spaceId, ancestorId).parent_id;
    }
  }
  if (getLocalDocumentDepth(spaceId, parent.id) >= 3) {
    throw new Error("Pages can be nested up to 3 levels");
  }
  return parent.id;
}

function collectLocalFolderSubtreeIds(spaceId: string, folderId: string): Set<string> {
  requireLocalFolder(spaceId, folderId);
  const folderIds = new Set<string>([folderId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const folder of localFolders.values()) {
      if (
        folder.space_id === spaceId &&
        folder.parent_id &&
        folderIds.has(folder.parent_id) &&
        !folderIds.has(folder.id)
      ) {
        folderIds.add(folder.id);
        changed = true;
      }
    }
  }
  return folderIds;
}

function collectLocalDocumentSubtreeIds(spaceId: string, docId: string): Set<string> {
  requireLocalDoc(spaceId, docId);
  const docIds = new Set<string>([docId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const doc of localDocs.values()) {
      if (
        doc.space_id === spaceId &&
        doc.parent_id &&
        docIds.has(doc.parent_id) &&
        !docIds.has(doc.id)
      ) {
        docIds.add(doc.id);
        changed = true;
      }
    }
  }
  return docIds;
}

function requireLocalDoc(spaceId: string, docId: string): LocalDocState {
  assertValidEntityId("document", docId);
  const doc = localDocs.get(docId);
  if (!doc || doc.space_id !== spaceId) throw new Error("Document not found");
  return doc;
}

function findLocalDocById(docId: string): LocalDocState | null {
  assertValidEntityId("document", docId);
  return localDocs.get(docId) ?? null;
}

function findLocalSpaceName(spaceId: string): string {
  return localSpaces.get(spaceId)?.name ?? PERSONAL_SPACE_NAME;
}

function normalizeVaultDocument(doc: VaultDocument): VaultDocument {
  return {
    ...doc,
    created_by: doc.created_by ?? LOCAL_SYSTEM_USER,
    last_updated_by: doc.last_updated_by ?? LOCAL_SYSTEM_USER,
    created_at: doc.created_at ?? doc.updated_at,
  };
}

function buildConflictRecordForDoc(
  doc: LocalDocState,
  reason = "content divergence",
): Omit<DocumentConflictRecord, "id" | "status"> {
  const spaceName = findLocalSpaceName(doc.space_id);
  const detectedAt = new Date().toISOString();
  const remoteContent = `${doc.content}\n\n[Remote change detected at ${detectedAt}]`;

  return {
    document_id: doc.id,
    document_title: doc.title,
    space_id: doc.space_id,
    space_name: spaceName,
    detected_at: detectedAt,
    summary: `${reason} for ${doc.title}`,
    local_snapshot: {
      title: doc.title,
      content: doc.content,
      updated_at: doc.updated_at,
    },
    remote_snapshot: {
      title: doc.title,
      content: remoteContent,
      updated_at: detectedAt,
    },
  };
}

function applyConflictResolutionToDocument(
  doc: LocalDocState,
  conflict: DocumentConflictRecord,
  strategy: ConflictResolutionStrategy,
): LocalDocState {
  const nextUpdatedAt = new Date().toISOString();

  if (strategy === "keep-remote") {
    return {
      ...doc,
      title: conflict.remote_snapshot.title,
      content: conflict.remote_snapshot.content,
      updated_at: nextUpdatedAt,
    };
  }

  if (strategy === "merge") {
    const mergedContent = `${conflict.local_snapshot.content}\n\n--- merged remote ---\n${conflict.remote_snapshot.content}`;
    return {
      ...doc,
      title: conflict.local_snapshot.title,
      content: mergedContent,
      updated_at: nextUpdatedAt,
    };
  }

  return {
    ...doc,
    title: conflict.local_snapshot.title,
    content: conflict.local_snapshot.content,
    updated_at: nextUpdatedAt,
  };
}

async function refreshSyncStateFromConflicts(): Promise<void> {
  if (await hasPendingConflicts()) {
    await localVault.setSyncState("conflict");
  } else {
    const current = await localVault.getSyncState();
    if (current === "conflict") {
      await localVault.setSyncState("idle");
    }
  }
}

function inLocalMode(): boolean {
  return getCurrentLaunchMode() === "local";
}

function normalizeTemplateInput(input: TemplateInput): TemplateInput {
  const name = input.name.trim();
  if (!name) throw new Error("Template name is required");
  if (name.length > 100) throw new Error("Template name must be at most 100 characters");
  if ((input.description ?? "").length > 1000) {
    throw new Error("Template description must be at most 1000 characters");
  }
  assertContentSize(input.content);
  return {
    name,
    description: input.description?.trim() || undefined,
    content: input.content,
    properties: input.properties,
    tags: input.tags ?? [],
  };
}

function toTemplateRecord(template: VaultTemplate): TemplateRecord {
  return {
    id: template.id,
    space_id: template.space_id,
    name: template.name,
    description: template.description,
    content: template.content,
    properties: template.properties,
    tags: template.tags,
    created_by: template.created_by,
    created_at: template.created_at,
    updated_at: template.updated_at,
  };
}

function normalizeWorkflowTemplateInput(
  input: WorkflowTemplateInput,
): NormalizedWorkflowTemplateInput {
  const name = input.name.trim();
  if (!name) throw new Error("Workflow template name is required");
  if (name.length > 100) {
    throw new Error("Workflow template name must be at most 100 characters");
  }
  if ((input.description ?? "").length > 1000) {
    throw new Error("Workflow template description must be at most 1000 characters");
  }

  const stages = input.stages
    .map((stage, index) => ({
      id: stage.id?.trim() || makeId("stage"),
      name: stage.name.trim(),
      sort_order: stage.sort_order ?? index,
    }))
    .filter((stage) => stage.name)
    .sort((a, b) => a.sort_order - b.sort_order);

  if (stages.length < 2) {
    throw new Error("Workflow templates require at least two stages");
  }
  if (stages.length > 12) {
    throw new Error("Workflow templates can have at most 12 stages");
  }

  const stageIds = new Set(stages.map((stage) => stage.id));
  const approvers = (input.approvers ?? []).map((approver) => {
    const approverId = approver.approver_id.trim();
    if (!approverId) throw new Error("Workflow approver is required");
    if (approver.stage_id && !stageIds.has(approver.stage_id)) {
      throw new Error("Workflow approver stage must exist");
    }
    return {
      id: approver.id?.trim() || makeId("approver"),
      approver_type: approver.approver_type,
      approver_id: approverId,
      stage_id: approver.stage_id,
    };
  });

  return {
    name,
    description: input.description?.trim() || undefined,
    stages,
    approvers,
  };
}

function toWorkflowTemplateRecord(template: VaultWorkflowTemplate): WorkflowTemplateRecord {
  return {
    id: template.id,
    space_id: template.space_id,
    name: template.name,
    description: template.description,
    stages: template.stages,
    approvers: template.approvers,
    created_by: template.created_by,
    created_at: template.created_at,
    updated_at: template.updated_at,
  };
}

function toDocumentWorkflowRecord(
  workflow: VaultDocumentWorkflow,
  template: VaultWorkflowTemplate,
): DocumentWorkflowRecord {
  const stage = template.stages.find((item) => item.id === workflow.current_stage_id);
  return {
    id: workflow.id,
    document_id: workflow.document_id,
    workflow_template_id: workflow.workflow_template_id,
    workflow_template_name: template.name,
    current_stage_id: workflow.current_stage_id,
    current_stage_name: stage?.name ?? "Unknown stage",
    status: workflow.status,
    stages: template.stages,
    approvers: template.approvers,
    created_by: workflow.created_by,
    created_at: workflow.created_at,
    updated_at: workflow.updated_at,
  };
}

function workflowSummaryForDocument(
  documentId: string,
): Pick<
  DocumentRecord,
  | "workflow_template_id"
  | "workflow_template_name"
  | "workflow_stage_id"
  | "workflow_stage_name"
  | "workflow_status"
> {
  const workflow = localDocumentWorkflowCache.find((item) => item.document_id === documentId);
  if (!workflow) return {};
  const template = localWorkflowTemplateCache.find(
    (item) => item.id === workflow.workflow_template_id,
  );
  if (!template) return {};
  const stage = template.stages.find((item) => item.id === workflow.current_stage_id);
  return {
    workflow_template_id: workflow.workflow_template_id,
    workflow_template_name: template.name,
    workflow_stage_id: workflow.current_stage_id,
    workflow_stage_name: stage?.name ?? "Unknown stage",
    workflow_status: workflow.status,
  };
}

function documentMatchesListOptions(doc: LocalDocState, options: ListDocumentsOptions): boolean {
  if (options.folder_id !== undefined) {
    if ((doc.folder_id ?? null) !== (options.folder_id ?? null)) return false;
  }

  const needsWorkflow =
    !!options.workflow_status || !!options.workflow_stage_id || !!options.workflow_template_id;
  if (!needsWorkflow) return true;

  const workflow = localDocumentWorkflowCache.find((item) => item.document_id === doc.id);
  if (!workflow) return false;
  if (options.workflow_status && workflow.status !== options.workflow_status) return false;
  if (options.workflow_stage_id && workflow.current_stage_id !== options.workflow_stage_id) {
    return false;
  }
  if (
    options.workflow_template_id &&
    workflow.workflow_template_id !== options.workflow_template_id
  ) {
    return false;
  }
  return true;
}

function isFinalWorkflowStage(template: VaultWorkflowTemplate, stageId: string): boolean {
  const orderedStages = [...template.stages].sort((a, b) => a.sort_order - b.sort_order);
  return orderedStages[orderedStages.length - 1]?.id === stageId;
}

function workflowApproverAllowsTransition(
  approvers: WorkflowTemplateApproverRecord[],
  stageId: string,
  userId: string,
  effectiveRole: string,
): boolean {
  if (approvers.length === 0) return true;
  const scopedApprovers = approvers.filter((approver) => approver.stage_id === stageId);
  const applicableApprovers =
    scopedApprovers.length > 0
      ? scopedApprovers
      : approvers.filter((approver) => !approver.stage_id);
  if (applicableApprovers.length === 0) return true;
  return applicableApprovers.some(
    (approver) =>
      (approver.approver_type === "user" && approver.approver_id === userId) ||
      (approver.approver_type === "role" &&
        (approver.approver_id === effectiveRole || effectiveRole === "admin")),
  );
}

function normalizePropertySchemaInput(input: PropertySchemaInput): PropertySchemaInput {
  const name = input.name.trim();
  if (!name) throw new Error("Property name is required");
  return {
    name,
    property_type: input.property_type || "text",
    options: input.options,
  };
}

function toPropertySchemaRecord(schema: VaultPropertySchema): PropertySchemaRecord {
  return {
    id: schema.id,
    name: schema.name,
    property_type: schema.property_type,
    options: schema.options,
    sort_order: schema.sort_order,
  };
}

function toTagRecord(tag: VaultTag, docCount?: number | null): TagRecord {
  return {
    id: tag.id,
    name: tag.name,
    color: tag.color,
    created_at: tag.created_at,
    doc_count: docCount,
  };
}

function findQueuedOperationPayload(
  snapshot: LocalVaultSnapshot,
  operation: Pick<VaultQueuedSyncOperation, "entity_type" | "entity_id" | "payload">,
): unknown {
  if (operation.payload !== undefined && operation.payload !== null) {
    return operation.payload;
  }

  switch (operation.entity_type) {
    case "space":
      return snapshot.spaces.find((item) => item.id === operation.entity_id) ?? null;
    case "folder":
      return snapshot.folders.find((item) => item.id === operation.entity_id) ?? null;
    case "document":
      return snapshot.documents.find((item) => item.id === operation.entity_id) ?? null;
    case "asset":
      return snapshot.assets.find((item) => item.id === operation.entity_id) ?? null;
    case "template":
      return snapshot.templates.find((item) => item.id === operation.entity_id) ?? null;
    case "workflow_template":
      return snapshot.workflow_templates?.find((item) => item.id === operation.entity_id) ?? null;
    case "document_workflow":
      return (
        snapshot.document_workflows?.find(
          (item) => item.id === operation.entity_id || item.document_id === operation.entity_id,
        ) ?? null
      );
    case "workflow_event":
      return (
        snapshot.document_workflow_events?.find((item) => item.id === operation.entity_id) ?? null
      );
    case "property_schema":
      return snapshot.property_schemas?.find((item) => item.id === operation.entity_id) ?? null;
    case "property_value":
      return snapshot.property_values?.find((item) => item.id === operation.entity_id) ?? null;
    case "tag":
      return snapshot.tags?.find((item) => item.id === operation.entity_id) ?? null;
    case "document_tag": {
      const [documentId, tagId] = operation.entity_id.split(":");
      return (
        snapshot.doc_tags?.find(
          (item) => item.document_id === documentId && item.tag_id === tagId,
        ) ?? null
      );
    }
    default:
      return null;
  }
}

function hasReplayPayloadBytes(payload: unknown): boolean {
  if (!payload || typeof payload !== "object") return false;
  const record = payload as Record<string, unknown>;
  const candidates = [record.data_url, record.url, record.source_url];
  return candidates.some((value) => typeof value === "string" && value.startsWith("data:"));
}

function isQueuedReplayPayloadAvailable(
  operation: Pick<VaultQueuedSyncOperation, "entity_type" | "action">,
  payload: unknown,
): boolean {
  if (!payload) return false;
  if (operation.entity_type === "asset" && operation.action === "create") {
    return hasReplayPayloadBytes(payload);
  }
  return true;
}

async function hydrateReplayOperationPayload(
  operation: Pick<VaultQueuedSyncOperation, "entity_type" | "action">,
  payload: unknown,
): Promise<unknown> {
  if (operation.entity_type === "asset" && operation.action === "create") {
    return packNativeReplayAssetPayload(payload);
  }
  return payload;
}

function summarizeQueuedSyncReplayOperations(
  operations: LocalSyncReplayOperationRecord[],
): LocalSyncReplayPreflightSummaryRecord {
  const byEntityType: Record<string, number> = {};
  const byAction: Record<string, number> = {};
  const unavailablePayloads: LocalSyncReplayUnavailablePayloadRecord[] = [];

  for (const operation of operations) {
    byEntityType[operation.entity_type] = (byEntityType[operation.entity_type] ?? 0) + 1;
    byAction[operation.action] = (byAction[operation.action] ?? 0) + 1;
    if (operation.payload_status === "missing") {
      unavailablePayloads.push(describeUnavailableReplayPayload(operation));
    }
  }

  return {
    total_operations: operations.length,
    missing_payload_count: unavailablePayloads.length,
    ready_operation_count: operations.length - unavailablePayloads.length,
    by_entity_type: byEntityType,
    by_action: byAction,
    unavailable_payloads: unavailablePayloads,
  };
}

async function applyProviderRemoteImportOperationToLocalVault(
  operation: ProviderRemoteImportApplyPlanOperation,
): Promise<{ status: "applied" | "skipped" | "failed" | "blocked"; message: string }> {
  if (operation.entity_type === "space") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<LocalSpaceState>)
        : null;

    if (
      operation.action !== "create" &&
      operation.action !== "update" &&
      operation.action !== "delete"
    ) {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (typeof payload?.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: `Remote space ${operation.action} payload ID does not match operation entity ID`,
      };
    }

    const existingSpace = localSpaces.get(operation.entity_id);
    if (operation.action === "delete") {
      if (!existingSpace) {
        return {
          status: "skipped",
          message: "Remote space delete target no longer exists locally",
        };
      }
      const deletePayload = payload as Record<string, unknown> | null;
      if (
        typeof deletePayload?.space_id === "string" &&
        deletePayload.space_id !== operation.entity_id
      ) {
        return {
          status: "failed",
          message: "Remote space delete payload space does not match operation entity ID",
        };
      }
      removeLocalSpaceContents(operation.entity_id);
      await persistLocalSnapshot();
      return {
        status: "applied",
        message: "Remote space delete applied through the confirmed local recovery flow",
      };
    }
    if (operation.action === "update" && !existingSpace) {
      return {
        status: "failed",
        message: "Remote space update target does not exist locally",
      };
    }

    if (!payload || typeof payload.name !== "string") {
      return {
        status: "failed",
        message: `Remote space ${operation.action} payload is missing name`,
      };
    }

    const name = payload.name.trim();
    if (!name) {
      return {
        status: "failed",
        message: `Remote space ${operation.action} payload is missing space name`,
      };
    }

    const generatedSlug = name
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9-]/g, "");
    const now = new Date().toISOString();
    const slug =
      typeof payload.slug === "string" && payload.slug.trim()
        ? payload.slug.trim()
        : (existingSpace?.slug ?? (generatedSlug || operation.entity_id));

    if (operation.action === "create" && existingSpace) {
      const matchesExisting =
        existingSpace.name === name &&
        existingSpace.slug === slug &&
        (typeof payload.owner_username !== "string" ||
          existingSpace.owner_username === payload.owner_username) &&
        (typeof payload.owner_display_name !== "string" ||
          existingSpace.owner_display_name === payload.owner_display_name) &&
        (typeof payload.created_by !== "string" || existingSpace.created_by === payload.created_by) &&
        (typeof payload.created_at !== "string" || existingSpace.created_at === payload.created_at) &&
        (typeof payload.updated_at !== "string" || existingSpace.updated_at === payload.updated_at);
      return matchesExisting
        ? {
            status: "skipped",
            message: "Remote space create target already exists locally with matching metadata",
          }
        : {
            status: "blocked",
            message: "Remote space create target already exists locally with different metadata",
          };
    }

    localSpaces.set(operation.entity_id, {
      ...(existingSpace ?? {
        id: operation.entity_id,
        created_at: now,
        updated_at: now,
        slug: generatedSlug || operation.entity_id,
      }),
      name,
      slug,
      owner_username:
        typeof payload.owner_username === "string"
          ? payload.owner_username
          : existingSpace?.owner_username,
      owner_display_name:
        typeof payload.owner_display_name === "string"
          ? payload.owner_display_name
          : existingSpace?.owner_display_name,
      created_by:
        typeof payload.created_by === "string" ? payload.created_by : existingSpace?.created_by,
      created_at:
        typeof payload.created_at === "string"
          ? payload.created_at
          : (existingSpace?.created_at ?? now),
      updated_at:
        typeof payload.updated_at === "string" ? payload.updated_at : new Date().toISOString(),
    });
    await persistLocalSnapshot();

    return {
      status: "applied",
      message: `Remote space ${operation.action} applied to the local vault`,
    };
  }

  if (operation.entity_type === "folder") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<VaultFolder>)
        : null;

    if (
      operation.action !== "create" &&
      operation.action !== "update" &&
      operation.action !== "delete"
    ) {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (typeof payload?.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: `Remote folder ${operation.action} payload ID does not match operation entity ID`,
      };
    }

    const existingFolder = localFolders.get(operation.entity_id);

    if (operation.action === "delete") {
      if (!existingFolder) {
        return {
          status: "skipped",
          message: "Remote folder delete target is already absent locally",
        };
      }

      if (typeof payload?.space_id === "string" && payload.space_id !== existingFolder.space_id) {
        return {
          status: "failed",
          message: "Remote folder delete payload space does not match local folder",
        };
      }

      if (typeof payload?.space_id === "string") {
        requireLocalSpace(payload.space_id);
      } else {
        requireLocalSpace(existingFolder.space_id);
      }

      const folderIds = new Set<string>([operation.entity_id]);
      let changed = true;
      while (changed) {
        changed = false;
        for (const candidate of localFolders.values()) {
          if (
            candidate.space_id === existingFolder.space_id &&
            candidate.parent_id &&
            folderIds.has(candidate.parent_id) &&
            !folderIds.has(candidate.id)
          ) {
            folderIds.add(candidate.id);
            changed = true;
          }
        }
      }

      for (const id of folderIds) {
        localFolders.delete(id);
      }
      const deletedDocumentIds = new Set<string>();
      for (const doc of Array.from(localDocs.values())) {
        if (
          doc.space_id === existingFolder.space_id &&
          doc.folder_id &&
          folderIds.has(doc.folder_id)
        ) {
          deletedDocumentIds.add(doc.id);
          localDocs.delete(doc.id);
        }
      }
      removeLocalDocumentMetadata(deletedDocumentIds);
      localSearchIndex = buildLocalSearchIndexEntries();
      await persistLocalSnapshot();

      return {
        status: "applied",
        message: "Remote folder delete applied to the local vault",
      };
    }

    if (!payload || typeof payload.space_id !== "string" || typeof payload.name !== "string") {
      return {
        status: "failed",
        message: `Remote folder ${operation.action} payload is missing space or name`,
      };
    }

    if (operation.action === "update" && !existingFolder) {
      return {
        status: "failed",
        message: "Remote folder update target does not exist locally",
      };
    }
    if (
      operation.action === "update" &&
      existingFolder &&
      existingFolder.space_id !== payload.space_id
    ) {
      return {
        status: "failed",
        message: "Remote folder update payload space does not match local folder",
      };
    }

    requireLocalSpace(payload.space_id);
    const name = payload.name.trim();
    if (!name) {
      return {
        status: "failed",
        message: `Remote folder ${operation.action} payload is missing folder name`,
      };
    }

    let parentId: string | undefined;
    try {
      parentId = normalizeLocalFolderParentId(
        payload.space_id,
        payload.parent_id,
        operation.action === "update" ? operation.entity_id : undefined,
      );
    } catch (error) {
      return {
        status: "blocked",
        message:
          error instanceof Error
            ? error.message
            : `Remote folder ${operation.action} payload has an invalid parent`,
      };
    }

    if (operation.action === "create" && existingFolder) {
      const matchesExisting =
        existingFolder.space_id === payload.space_id &&
        existingFolder.name === name &&
        existingFolder.parent_id === parentId;
      return matchesExisting
        ? {
            status: "skipped",
            message: "Remote folder create target already exists locally with matching metadata",
          }
        : {
            status: "blocked",
            message: "Remote folder create target already exists locally with different metadata",
          };
    }

    localFolders.set(operation.entity_id, {
      ...(existingFolder ?? { id: operation.entity_id }),
      space_id: payload.space_id,
      name,
      parent_id: parentId,
    });
    await persistLocalSnapshot();

    return {
      status: "applied",
      message: `Remote folder ${operation.action} applied to the local vault`,
    };
  }

  if (operation.entity_type === "tag") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<VaultTag>)
        : null;

    if (
      operation.action !== "create" &&
      operation.action !== "update" &&
      operation.action !== "delete"
    ) {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (typeof payload?.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: `Remote tag ${operation.action} payload ID does not match operation entity ID`,
      };
    }

    const existingTag = localTags.find((tag) => tag.id === operation.entity_id);

    if (operation.action === "delete") {
      if (!existingTag) {
        return {
          status: "skipped",
          message: "Remote tag delete target is already absent locally",
        };
      }

      if (typeof payload?.space_id === "string" && payload.space_id !== existingTag.space_id) {
        return {
          status: "failed",
          message: "Remote tag delete payload space does not match local tag",
        };
      }

      if (typeof payload?.space_id === "string") {
        requireLocalSpace(payload.space_id);
      } else {
        requireLocalSpace(existingTag.space_id);
      }

      localTags = localTags.filter((tag) => tag.id !== operation.entity_id);
      localDocTags = localDocTags.filter((docTag) => docTag.tag_id !== operation.entity_id);
      await persistLocalSnapshot();

      return {
        status: "applied",
        message: "Remote tag delete applied to the local vault",
      };
    }

    if (!payload || typeof payload.space_id !== "string" || typeof payload.name !== "string") {
      return {
        status: "failed",
        message: `Remote tag ${operation.action} payload is missing space or name`,
      };
    }

    if (operation.action === "update" && !existingTag) {
      return {
        status: "failed",
        message: "Remote tag update target does not exist locally",
      };
    }
    if (operation.action === "update" && existingTag && existingTag.space_id !== payload.space_id) {
      return {
        status: "failed",
        message: "Remote tag update payload space does not match local tag",
      };
    }

    requireLocalSpace(payload.space_id);
    const name = payload.name.trim();
    if (!name) {
      return {
        status: "failed",
        message: `Remote tag ${operation.action} payload is missing tag name`,
      };
    }

    const importedTag: VaultTag = {
      id: operation.entity_id,
      space_id: payload.space_id,
      name,
      color: typeof payload.color === "string" ? payload.color : (existingTag?.color ?? "#6b7280"),
      created_at:
        typeof payload.created_at === "string"
          ? payload.created_at
          : (existingTag?.created_at ?? new Date().toISOString()),
    };

    if (operation.action === "create" && existingTag) {
      const matchesExisting =
        existingTag.space_id === importedTag.space_id &&
        existingTag.name === importedTag.name &&
        existingTag.color === importedTag.color &&
        existingTag.created_at === importedTag.created_at;
      return matchesExisting
        ? {
            status: "skipped",
            message: "Remote tag create target already exists locally with matching metadata",
          }
        : {
            status: "blocked",
            message: "Remote tag create target already exists locally with different metadata",
          };
    }

    localTags = existingTag
      ? localTags.map((tag) => (tag.id === operation.entity_id ? importedTag : tag))
      : [...localTags, importedTag];
    await persistLocalSnapshot();

    return {
      status: "applied",
      message: `Remote tag ${operation.action} applied to the local vault`,
    };
  }

  if (operation.entity_type === "document_tag") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<VaultDocTag> & { space_id?: string })
        : null;

    if (operation.action !== "add" && operation.action !== "remove") {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (!payload || typeof payload.document_id !== "string" || typeof payload.tag_id !== "string") {
      return {
        status: "failed",
        message: `Remote document tag ${operation.action} payload is missing document or tag`,
      };
    }

    const relationshipId = `${payload.document_id}:${payload.tag_id}`;
    if (operation.entity_id !== relationshipId) {
      return {
        status: "failed",
        message: `Remote document tag ${operation.action} payload does not match operation entity ID`,
      };
    }

    const existingDoc = localDocs.get(payload.document_id);
    if (!existingDoc) {
      return {
        status: "failed",
        message: `Remote document tag ${operation.action} target document does not exist locally`,
      };
    }

    if (typeof payload.space_id === "string" && payload.space_id !== existingDoc.space_id) {
      return {
        status: "failed",
        message: `Remote document tag ${operation.action} payload space does not match local document`,
      };
    }

    requireLocalSpace(existingDoc.space_id);
    const existingTag = localTags.find(
      (tag) => tag.id === payload.tag_id && tag.space_id === existingDoc.space_id,
    );
    if (!existingTag) {
      return {
        status: "failed",
        message: `Remote document tag ${operation.action} target tag does not exist locally`,
      };
    }

    const hasRelationship = localDocTags.some(
      (docTag) => docTag.document_id === payload.document_id && docTag.tag_id === payload.tag_id,
    );

    if (operation.action === "add") {
      if (hasRelationship) {
        return {
          status: "skipped",
          message: "Remote document tag add target already exists locally",
        };
      }

      localDocTags = [
        ...localDocTags,
        { document_id: payload.document_id, tag_id: payload.tag_id },
      ];
      await persistLocalSnapshot();

      return {
        status: "applied",
        message: "Remote document tag add applied to the local vault",
      };
    }

    if (!hasRelationship) {
      return {
        status: "skipped",
        message: "Remote document tag remove target is already absent locally",
      };
    }

    localDocTags = localDocTags.filter(
      (docTag) => !(docTag.document_id === payload.document_id && docTag.tag_id === payload.tag_id),
    );
    await persistLocalSnapshot();

    return {
      status: "applied",
      message: "Remote document tag remove applied to the local vault",
    };
  }

  if (operation.entity_type === "template") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<VaultTemplate>)
        : null;

    if (
      operation.action !== "create" &&
      operation.action !== "update" &&
      operation.action !== "delete"
    ) {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (typeof payload?.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: `Remote template ${operation.action} payload ID does not match operation entity ID`,
      };
    }

    const existingTemplate = localTemplateCache.find(
      (template) => template.id === operation.entity_id,
    );

    if (operation.action === "delete") {
      if (!existingTemplate) {
        return {
          status: "skipped",
          message: "Remote template delete target is already absent locally",
        };
      }

      if (typeof payload?.space_id === "string" && payload.space_id !== existingTemplate.space_id) {
        return {
          status: "failed",
          message: "Remote template delete payload space does not match local template",
        };
      }

      if (typeof payload?.space_id === "string") {
        requireLocalSpace(payload.space_id);
      } else {
        requireLocalSpace(existingTemplate.space_id);
      }

      localTemplateCache = localTemplateCache.filter(
        (template) => template.id !== operation.entity_id,
      );
      await localVault.deleteTemplate(operation.entity_id);

      return {
        status: "applied",
        message: "Remote template delete applied to the local vault",
      };
    }

    if (
      !payload ||
      typeof payload.space_id !== "string" ||
      typeof payload.name !== "string" ||
      typeof payload.content !== "string"
    ) {
      return {
        status: "failed",
        message: `Remote template ${operation.action} payload is missing space, name, or content`,
      };
    }

    if (operation.action === "update" && !existingTemplate) {
      return {
        status: "failed",
        message: "Remote template update target does not exist locally",
      };
    }
    if (
      operation.action === "update" &&
      existingTemplate &&
      existingTemplate.space_id !== payload.space_id
    ) {
      return {
        status: "failed",
        message: "Remote template update payload space does not match local template",
      };
    }

    requireLocalSpace(payload.space_id);
    let normalized: TemplateInput;
    try {
      normalized = normalizeTemplateInput({
        name: payload.name,
        description: typeof payload.description === "string" ? payload.description : undefined,
        content: payload.content,
        properties: payload.properties,
      });
    } catch (error) {
      return {
        status: "failed",
        message:
          error instanceof Error
            ? `Remote template ${operation.action} payload is invalid: ${error.message}`
            : `Remote template ${operation.action} payload is invalid`,
      };
    }

    const now = new Date().toISOString();
    const importedTemplate: VaultTemplate = {
      id: operation.entity_id,
      space_id: payload.space_id,
      name: normalized.name,
      description: normalized.description,
      content: normalized.content,
      properties: normalized.properties,
      tags: Array.isArray(payload.tags)
        ? payload.tags.filter((tag) => typeof tag === "string")
        : [],
      created_by:
        typeof payload.created_by === "string" ? payload.created_by : existingTemplate?.created_by,
      created_at:
        typeof payload.created_at === "string"
          ? payload.created_at
          : (existingTemplate?.created_at ?? now),
      updated_at:
        typeof payload.updated_at === "string"
          ? payload.updated_at
          : operation.action === "update"
            ? now
            : (existingTemplate?.updated_at ?? now),
    };

    if (operation.action === "create" && existingTemplate) {
      const matchesExisting =
        existingTemplate.space_id === importedTemplate.space_id &&
        existingTemplate.name === importedTemplate.name &&
        existingTemplate.description === importedTemplate.description &&
        existingTemplate.content === importedTemplate.content &&
        JSON.stringify(existingTemplate.properties) === JSON.stringify(importedTemplate.properties) &&
        JSON.stringify(existingTemplate.tags) === JSON.stringify(importedTemplate.tags) &&
        existingTemplate.created_by === importedTemplate.created_by &&
        existingTemplate.created_at === importedTemplate.created_at &&
        existingTemplate.updated_at === importedTemplate.updated_at;
      return matchesExisting
        ? {
            status: "skipped",
            message: "Remote template create target already exists locally with matching metadata",
          }
        : {
            status: "blocked",
            message: "Remote template create target already exists locally with different metadata",
          };
    }

    localTemplateCache = existingTemplate
      ? localTemplateCache.map((template) =>
          template.id === operation.entity_id ? importedTemplate : template,
        )
      : [...localTemplateCache, importedTemplate];
    await localVault.saveTemplate(importedTemplate);

    return {
      status: "applied",
      message: `Remote template ${operation.action} applied to the local vault`,
    };
  }

  if (operation.entity_type === "workflow_template") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<VaultWorkflowTemplate>)
        : null;

    if (
      operation.action !== "create" &&
      operation.action !== "update" &&
      operation.action !== "delete"
    ) {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (typeof payload?.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: `Remote workflow template ${operation.action} payload ID does not match operation entity ID`,
      };
    }

    const existingTemplate = localWorkflowTemplateCache.find(
      (template) => template.id === operation.entity_id,
    );

    if (operation.action === "delete") {
      if (!existingTemplate) {
        return {
          status: "skipped",
          message: "Remote workflow template delete target is already absent locally",
        };
      }

      if (typeof payload?.space_id === "string" && payload.space_id !== existingTemplate.space_id) {
        return {
          status: "failed",
          message:
            "Remote workflow template delete payload space does not match local workflow template",
        };
      }

      if (typeof payload?.space_id === "string") {
        requireLocalSpace(payload.space_id);
      } else {
        requireLocalSpace(existingTemplate.space_id);
      }

      localWorkflowTemplateCache = localWorkflowTemplateCache.filter(
        (template) => template.id !== operation.entity_id,
      );
      await localVault.deleteWorkflowTemplate(operation.entity_id);

      return {
        status: "applied",
        message: "Remote workflow template delete applied to the local vault",
      };
    }

    if (
      !payload ||
      typeof payload.space_id !== "string" ||
      typeof payload.name !== "string" ||
      !Array.isArray(payload.stages)
    ) {
      return {
        status: "failed",
        message: `Remote workflow template ${operation.action} payload is missing space, name, or stages`,
      };
    }

    if (operation.action === "update" && !existingTemplate) {
      return {
        status: "failed",
        message: "Remote workflow template update target does not exist locally",
      };
    }
    if (
      operation.action === "update" &&
      existingTemplate &&
      existingTemplate.space_id !== payload.space_id
    ) {
      return {
        status: "failed",
        message:
          "Remote workflow template update payload space does not match local workflow template",
      };
    }

    requireLocalSpace(payload.space_id);
    const normalized = normalizeWorkflowTemplateInput({
      name: payload.name,
      description: payload.description,
      stages: payload.stages,
      approvers: Array.isArray(payload.approvers) ? payload.approvers : [],
    });
    const now = new Date().toISOString();
    const importedTemplate: VaultWorkflowTemplate = {
      id: operation.entity_id,
      space_id: payload.space_id,
      name: normalized.name,
      description: normalized.description,
      stages: normalized.stages,
      approvers: normalized.approvers ?? [],
      created_by:
        typeof payload.created_by === "string" ? payload.created_by : existingTemplate?.created_by,
      created_at:
        typeof payload.created_at === "string"
          ? payload.created_at
          : (existingTemplate?.created_at ?? now),
      updated_at:
        typeof payload.updated_at === "string"
          ? payload.updated_at
          : operation.action === "update"
            ? now
            : (existingTemplate?.updated_at ?? now),
    };

    if (operation.action === "create" && existingTemplate) {
      const matchesExisting =
        existingTemplate.space_id === importedTemplate.space_id &&
        existingTemplate.name === importedTemplate.name &&
        existingTemplate.description === importedTemplate.description &&
        JSON.stringify(existingTemplate.stages) === JSON.stringify(importedTemplate.stages) &&
        JSON.stringify(existingTemplate.approvers) === JSON.stringify(importedTemplate.approvers) &&
        existingTemplate.created_by === importedTemplate.created_by &&
        existingTemplate.created_at === importedTemplate.created_at &&
        existingTemplate.updated_at === importedTemplate.updated_at;
      return matchesExisting
        ? {
            status: "skipped",
            message:
              "Remote workflow template create target already exists locally with matching metadata",
          }
        : {
            status: "blocked",
            message:
              "Remote workflow template create target already exists locally with different metadata",
          };
    }

    localWorkflowTemplateCache = existingTemplate
      ? localWorkflowTemplateCache.map((template) =>
          template.id === operation.entity_id ? importedTemplate : template,
        )
      : [...localWorkflowTemplateCache, importedTemplate];
    await localVault.saveWorkflowTemplate(importedTemplate);

    return {
      status: "applied",
      message: `Remote workflow template ${operation.action} applied to the local vault`,
    };
  }

  if (operation.entity_type === "property_schema") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<VaultPropertySchema> & {
            property_ids?: unknown;
            schemas?: unknown;
          })
        : null;

    if (
      operation.action !== "create" &&
      operation.action !== "update" &&
      operation.action !== "delete" &&
      operation.action !== "reorder"
    ) {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (operation.action === "reorder") {
      if (
        !payload ||
        typeof payload.space_id !== "string" ||
        !Array.isArray(payload.property_ids)
      ) {
        return {
          status: "failed",
          message: "Remote property schema reorder payload is missing space or property order",
        };
      }

      if (operation.entity_id !== payload.space_id) {
        return {
          status: "failed",
          message:
            "Remote property schema reorder payload space does not match operation entity ID",
        };
      }

      requireLocalSpace(payload.space_id);
      const propertyIds = payload.property_ids.filter((id): id is string => typeof id === "string");
      if (propertyIds.length !== payload.property_ids.length) {
        return {
          status: "failed",
          message: "Remote property schema reorder payload includes invalid property IDs",
        };
      }
      if (new Set(propertyIds).size !== propertyIds.length) {
        return {
          status: "failed",
          message: "Remote property schema reorder payload includes duplicate property IDs",
        };
      }

      const schemasById = new Map(
        localPropertySchemas
          .filter((schema) => schema.space_id === payload.space_id)
          .map((schema) => [schema.id, schema]),
      );
      const missingIds = propertyIds.filter((id) => !schemasById.has(id));
      if (missingIds.length > 0) {
        return {
          status: "failed",
          message: "Remote property schema reorder references missing local schemas",
        };
      }

      const orderById = new Map(propertyIds.map((id, index) => [id, index + 1]));
      localPropertySchemas = localPropertySchemas.map((schema) => {
        const nextOrder =
          schema.space_id === payload.space_id ? orderById.get(schema.id) : undefined;
        return nextOrder ? { ...schema, sort_order: nextOrder } : schema;
      });
      await persistLocalSnapshot();

      return {
        status: "applied",
        message: "Remote property schema reorder applied to the local vault",
      };
    }

    if (typeof payload?.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: `Remote property schema ${operation.action} payload ID does not match operation entity ID`,
      };
    }

    const existingSchema = localPropertySchemas.find((schema) => schema.id === operation.entity_id);

    if (operation.action === "delete") {
      if (!existingSchema) {
        return {
          status: "skipped",
          message: "Remote property schema delete target is already absent locally",
        };
      }

      if (typeof payload?.space_id === "string" && payload.space_id !== existingSchema.space_id) {
        return {
          status: "failed",
          message:
            "Remote property schema delete payload space does not match local property schema",
        };
      }

      if (typeof payload?.space_id === "string") {
        requireLocalSpace(payload.space_id);
      } else {
        requireLocalSpace(existingSchema.space_id);
      }

      localPropertySchemas = localPropertySchemas.filter(
        (schema) => schema.id !== operation.entity_id,
      );
      localPropertyValues = localPropertyValues.filter(
        (value) => value.property_id !== operation.entity_id,
      );
      await persistLocalSnapshot();

      return {
        status: "applied",
        message: "Remote property schema delete applied to the local vault",
      };
    }

    if (!payload || typeof payload.space_id !== "string" || typeof payload.name !== "string") {
      return {
        status: "failed",
        message: `Remote property schema ${operation.action} payload is missing space or name`,
      };
    }

    if (operation.action === "update" && !existingSchema) {
      return {
        status: "failed",
        message: "Remote property schema update target does not exist locally",
      };
    }
    if (
      operation.action === "update" &&
      existingSchema &&
      existingSchema.space_id !== payload.space_id
    ) {
      return {
        status: "failed",
        message:
          "Remote property schema update payload space does not match local property schema",
      };
    }

    requireLocalSpace(payload.space_id);
    const normalized = normalizePropertySchemaInput({
      name: payload.name,
      property_type: typeof payload.property_type === "string" ? payload.property_type : "text",
      options: payload.options,
    });
    const nextSortOrder =
      typeof payload.sort_order === "number" && Number.isFinite(payload.sort_order)
        ? payload.sort_order
        : (existingSchema?.sort_order ??
          Math.max(
            0,
            ...localPropertySchemas
              .filter((schema) => schema.space_id === payload.space_id)
              .map((schema) => schema.sort_order),
          ) + 1);
    const importedSchema: VaultPropertySchema = {
      id: operation.entity_id,
      space_id: payload.space_id,
      name: normalized.name,
      property_type: normalized.property_type,
      options: normalized.options,
      sort_order: nextSortOrder,
    };

    if (operation.action === "create" && existingSchema) {
      const matchesExisting =
        existingSchema.space_id === importedSchema.space_id &&
        existingSchema.name === importedSchema.name &&
        existingSchema.property_type === importedSchema.property_type &&
        JSON.stringify(existingSchema.options) === JSON.stringify(importedSchema.options) &&
        existingSchema.sort_order === importedSchema.sort_order;
      return matchesExisting
        ? {
            status: "skipped",
            message:
              "Remote property schema create target already exists locally with matching metadata",
          }
        : {
            status: "blocked",
            message:
              "Remote property schema create target already exists locally with different metadata",
          };
    }

    localPropertySchemas = existingSchema
      ? localPropertySchemas.map((schema) =>
          schema.id === operation.entity_id ? importedSchema : schema,
        )
      : [...localPropertySchemas, importedSchema];
    await persistLocalSnapshot();

    return {
      status: "applied",
      message: `Remote property schema ${operation.action} applied to the local vault`,
    };
  }

  if (operation.entity_type === "document_property_values") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as {
            document_id?: unknown;
            space_id?: unknown;
            values?: unknown;
          })
        : null;

    if (operation.action !== "set") {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (
      !payload ||
      typeof payload.document_id !== "string" ||
      typeof payload.space_id !== "string" ||
      !Array.isArray(payload.values)
    ) {
      return {
        status: "failed",
        message:
          "Remote document property values set payload is missing document, space, or values",
      };
    }

    if (operation.entity_id !== payload.document_id) {
      return {
        status: "failed",
        message:
          "Remote document property values payload document does not match operation entity ID",
      };
    }

    const existingDoc = localDocs.get(payload.document_id);
    if (!existingDoc) {
      return {
        status: "failed",
        message: "Remote document property values target document does not exist locally",
      };
    }

    if (existingDoc.space_id !== payload.space_id) {
      return {
        status: "failed",
        message: "Remote document property values payload space does not match local document",
      };
    }

    requireLocalSpace(payload.space_id);
    const importedValues: VaultPropertyValue[] = [];
    const importedPropertyIds = new Set<string>();
    for (const value of payload.values) {
      if (!value || typeof value !== "object") {
        return {
          status: "failed",
          message: "Remote document property values payload includes an invalid value entry",
        };
      }

      const entry = value as Partial<VaultPropertyValue>;
      if (
        typeof entry.property_id !== "string" ||
        (entry.value !== null && typeof entry.value !== "string")
      ) {
        return {
          status: "failed",
          message:
            "Remote document property values payload includes a value without property or value",
        };
      }
      if (importedPropertyIds.has(entry.property_id)) {
        return {
          status: "failed",
          message:
            "Remote document property values payload includes duplicate property values",
        };
      }

      const schema = localPropertySchemas.find(
        (item) => item.id === entry.property_id && item.space_id === payload.space_id,
      );
      if (!schema) {
        return {
          status: "failed",
          message:
            "Remote document property values payload references a missing local property schema",
        };
      }

      const existingValueById =
        typeof entry.id === "string"
          ? localPropertyValues.find((item) => item.id === entry.id)
          : undefined;
      if (
        existingValueById &&
        (existingValueById.document_id !== payload.document_id ||
          existingValueById.property_id !== entry.property_id)
      ) {
        return {
          status: "failed",
          message:
            "Remote document property values payload ID conflicts with another local property value",
        };
      }

      const existingValue = localPropertyValues.find(
        (item) =>
          item.document_id === payload.document_id && item.property_id === entry.property_id,
      );
      importedPropertyIds.add(entry.property_id);
      importedValues.push({
        id: typeof entry.id === "string" ? entry.id : (existingValue?.id ?? makeId("propval")),
        document_id: payload.document_id,
        property_id: entry.property_id,
        value: entry.value,
        created_at:
          typeof entry.created_at === "string" ? entry.created_at : existingValue?.created_at,
        updated_at:
          typeof entry.updated_at === "string" ? entry.updated_at : existingValue?.updated_at,
      });
    }

    localPropertyValues = [
      ...localPropertyValues.filter(
        (value) =>
          value.document_id !== payload.document_id || !importedPropertyIds.has(value.property_id),
      ),
      ...importedValues,
    ];
    await persistLocalSnapshot();

    return {
      status: "applied",
      message: "Remote document property values set applied to the local vault",
    };
  }

  if (operation.entity_type === "asset") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<VaultAsset> & {
            space_id?: unknown;
            document_id?: unknown;
            data_url?: unknown;
          })
        : null;

    if (operation.action === "delete") {
      const existingAsset = snapshotAssetCache.find((asset) => asset.id === operation.entity_id);
      if (!existingAsset) {
        return {
          status: "skipped",
          message: "Remote asset delete target is already absent locally",
        };
      }

      if (typeof payload?.id === "string" && payload.id !== operation.entity_id) {
        return {
          status: "failed",
          message: "Remote asset delete payload ID does not match operation entity ID",
        };
      }
      if (typeof payload?.space_id === "string") {
        requireLocalSpace(payload.space_id);
      }
      if (typeof payload?.document_id === "string") {
        const existingDoc = localDocs.get(payload.document_id);
        if (!existingDoc) {
          return {
            status: "failed",
            message: "Remote asset delete payload document does not exist locally",
          };
        }
        if (typeof payload.space_id === "string" && existingDoc.space_id !== payload.space_id) {
          return {
            status: "failed",
            message: "Remote asset delete payload space does not match local document",
          };
        }
      }

      await localVault.deleteAsset(operation.entity_id);
      snapshotAssetCache = await localVault.listAssets();

      return {
        status: "applied",
        message: "Remote asset delete applied to the local vault",
      };
    }

    if (operation.action !== "create") {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (typeof payload?.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: "Remote asset create payload ID does not match operation entity ID",
      };
    }

    if (
      !payload ||
      typeof payload.space_id !== "string" ||
      typeof payload.name !== "string" ||
      typeof payload.size !== "number"
    ) {
      return {
        status: "failed",
        message: "Remote asset create payload is missing space, name, or size",
      };
    }

    requireLocalSpace(payload.space_id);
    const documentId = typeof payload.document_id === "string" ? payload.document_id : undefined;
    if (documentId) {
      const existingDoc = localDocs.get(documentId);
      if (!existingDoc) {
        return {
          status: "failed",
          message: "Remote asset create target document does not exist locally",
        };
      }
      if (existingDoc.space_id !== payload.space_id) {
        return {
          status: "failed",
          message: "Remote asset create payload space does not match local document",
        };
      }
    }

    const url =
      typeof payload.data_url === "string"
        ? payload.data_url
        : typeof payload.url === "string"
          ? payload.url
          : "";
    if (!url.startsWith("data:")) {
      return {
        status: "failed",
        message: "Remote asset create payload requires embedded data URL bytes",
      };
    }

    const importedAsset: VaultAsset = {
      id: operation.entity_id,
      name: payload.name.trim(),
      mime_type: typeof payload.mime_type === "string" ? payload.mime_type : undefined,
      size: payload.size,
      url,
      source: "browser",
      space_id: payload.space_id,
      document_id: payload.document_id,
      created_at:
        typeof payload.created_at === "string" ? payload.created_at : new Date().toISOString(),
    };
    if (!importedAsset.name) {
      return {
        status: "failed",
        message: "Remote asset create payload is missing asset name",
      };
    }
    const blob = await vaultAssetToBlob(importedAsset);
    if (!blob) {
      return {
        status: "failed",
        message: "Remote asset create payload includes invalid data URL bytes",
      };
    }
    if (blob.size !== importedAsset.size) {
      return {
        status: "failed",
        message: "Remote asset create payload byte size does not match declared size",
      };
    }

    const existingAsset = snapshotAssetCache.find((asset) => asset.id === operation.entity_id);
    if (existingAsset) {
      const matchesExisting =
        existingAsset.name === importedAsset.name &&
        existingAsset.mime_type === importedAsset.mime_type &&
        existingAsset.size === importedAsset.size &&
        existingAsset.url === importedAsset.url &&
        existingAsset.source === importedAsset.source &&
        existingAsset.space_id === importedAsset.space_id &&
        existingAsset.document_id === importedAsset.document_id;
      return matchesExisting
        ? {
            status: "skipped",
            message: "Remote asset create target already exists locally with matching bytes",
          }
        : {
            status: "blocked",
            message: "Remote asset create target already exists locally with different data",
          };
    }

    await localVault.saveAsset(importedAsset);
    snapshotAssetCache = await localVault.listAssets();

    return {
      status: "applied",
      message: "Remote asset create applied to the local vault",
    };
  }

  if (operation.entity_type === "document_workflow") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<VaultDocumentWorkflow> & { space_id?: unknown })
        : null;

    if (
      operation.action !== "create" &&
      operation.action !== "update" &&
      operation.action !== "transition" &&
      operation.action !== "delete"
    ) {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    const existingWorkflow =
      localDocumentWorkflowCache.find((workflow) => workflow.id === operation.entity_id) ??
      localDocumentWorkflowCache.find(
        (workflow) =>
          typeof payload?.document_id === "string" && workflow.document_id === payload.document_id,
      ) ??
      localDocumentWorkflowCache.find((workflow) => workflow.document_id === operation.entity_id);

    if (operation.action === "delete") {
      if (!existingWorkflow) {
        return {
          status: "skipped",
          message: "Remote document workflow delete target is already absent locally",
        };
      }

      if (typeof payload?.id === "string" && payload.id !== existingWorkflow.id) {
        return {
          status: "failed",
          message: "Remote document workflow delete payload ID does not match local workflow",
        };
      }
      if (
        typeof payload?.document_id === "string" &&
        payload.document_id !== existingWorkflow.document_id
      ) {
        return {
          status: "failed",
          message: "Remote document workflow delete payload document does not match local workflow",
        };
      }

      const existingDoc = localDocs.get(existingWorkflow.document_id);
      if (!existingDoc) {
        return {
          status: "failed",
          message: "Remote document workflow delete target document does not exist locally",
        };
      }
      if (typeof payload?.space_id === "string" && payload.space_id !== existingDoc.space_id) {
        return {
          status: "failed",
          message: "Remote document workflow delete payload space does not match local document",
        };
      }

      requireLocalSpace(existingDoc.space_id);
      localDocumentWorkflowCache = localDocumentWorkflowCache.filter(
        (workflow) => workflow.document_id !== existingWorkflow.document_id,
      );
      await localVault.deleteDocumentWorkflow(existingWorkflow.document_id);

      return {
        status: "applied",
        message: "Remote document workflow delete applied to the local vault",
      };
    }

    if (
      !payload ||
      typeof payload.id !== "string" ||
      typeof payload.document_id !== "string" ||
      typeof payload.workflow_template_id !== "string" ||
      typeof payload.current_stage_id !== "string" ||
      typeof payload.status !== "string"
    ) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} payload is missing workflow state`,
      };
    }

    if (payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} payload ID does not match operation entity ID`,
      };
    }

    if (
      payload.status !== "active" &&
      payload.status !== "completed" &&
      payload.status !== "cancelled"
    ) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} payload has invalid status`,
      };
    }

    const existingDoc = localDocs.get(payload.document_id);
    if (!existingDoc) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} target document does not exist locally`,
      };
    }
    if (typeof payload.space_id === "string" && payload.space_id !== existingDoc.space_id) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} payload space does not match local document`,
      };
    }

    requireLocalSpace(existingDoc.space_id);
    const template = localWorkflowTemplateCache.find(
      (item) => item.id === payload.workflow_template_id && item.space_id === existingDoc.space_id,
    );
    if (!template) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} target template does not exist locally`,
      };
    }
    if (!template.stages.some((stage) => stage.id === payload.current_stage_id)) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} target stage does not exist locally`,
      };
    }

    if ((operation.action === "update" || operation.action === "transition") && !existingWorkflow) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} target does not exist locally`,
      };
    }
    if (existingWorkflow && existingWorkflow.document_id !== payload.document_id) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} payload document does not match local workflow`,
      };
    }
    if (
      (operation.action === "update" || operation.action === "transition") &&
      existingWorkflow &&
      existingWorkflow.id !== payload.id
    ) {
      return {
        status: "failed",
        message: `Remote document workflow ${operation.action} payload ID does not match local workflow`,
      };
    }

    const now = new Date().toISOString();
    const importedWorkflow: VaultDocumentWorkflow = {
      id: payload.id,
      document_id: payload.document_id,
      workflow_template_id: payload.workflow_template_id,
      current_stage_id: payload.current_stage_id,
      status: payload.status,
      created_by:
        typeof payload.created_by === "string" ? payload.created_by : existingWorkflow?.created_by,
      created_at:
        typeof payload.created_at === "string"
          ? payload.created_at
          : (existingWorkflow?.created_at ?? now),
      updated_at:
        typeof payload.updated_at === "string"
          ? payload.updated_at
          : operation.action === "create"
            ? (existingWorkflow?.updated_at ?? now)
            : now,
    };

    if (
      operation.action === "create" &&
      existingWorkflow &&
      existingWorkflow.id !== importedWorkflow.id
    ) {
      return {
        status: "blocked",
        message:
          "Remote document workflow create target document already has a different local workflow",
      };
    }

    if (operation.action === "create" && existingWorkflow) {
      const matchesExisting =
        existingWorkflow.document_id === importedWorkflow.document_id &&
        existingWorkflow.workflow_template_id === importedWorkflow.workflow_template_id &&
        existingWorkflow.current_stage_id === importedWorkflow.current_stage_id &&
        existingWorkflow.status === importedWorkflow.status &&
        existingWorkflow.created_by === importedWorkflow.created_by &&
        existingWorkflow.created_at === importedWorkflow.created_at &&
        existingWorkflow.updated_at === importedWorkflow.updated_at;
      return matchesExisting
        ? {
            status: "skipped",
            message:
              "Remote document workflow create target already exists locally with matching state",
          }
        : {
            status: "blocked",
            message:
              "Remote document workflow create target already exists locally with different state",
          };
    }

    localDocumentWorkflowCache = [
      ...localDocumentWorkflowCache.filter(
        (workflow) => workflow.document_id !== importedWorkflow.document_id,
      ),
      importedWorkflow,
    ];
    await localVault.saveDocumentWorkflow(importedWorkflow);

    return {
      status: "applied",
      message: `Remote document workflow ${operation.action} applied to the local vault`,
    };
  }

  if (operation.entity_type === "document_workflow_event") {
    const payload =
      operation.payload && typeof operation.payload === "object"
        ? (operation.payload as Partial<VaultDocumentWorkflowEvent> & { space_id?: unknown })
        : null;

    if (
      operation.action !== "enable" &&
      operation.action !== "transition" &&
      operation.action !== "disable" &&
      operation.action !== "delegate" &&
      operation.action !== "escalate"
    ) {
      return {
        status: "blocked",
        message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
      };
    }

    if (
      !payload ||
      typeof payload.id !== "string" ||
      typeof payload.document_workflow_id !== "string" ||
      typeof payload.document_id !== "string" ||
      typeof payload.to_stage_id !== "string" ||
      typeof payload.action !== "string" ||
      typeof payload.created_at !== "string"
    ) {
      return {
        status: "failed",
        message: `Remote document workflow event ${operation.action} payload is missing event state`,
      };
    }

    if (payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: `Remote document workflow event ${operation.action} payload ID does not match operation entity ID`,
      };
    }
    if (payload.action !== operation.action) {
      return {
        status: "failed",
        message: `Remote document workflow event ${operation.action} payload action does not match operation action`,
      };
    }

    const existingDoc = localDocs.get(payload.document_id);
    if (!existingDoc) {
      return {
        status: "failed",
        message: `Remote document workflow event ${operation.action} target document does not exist locally`,
      };
    }
    if (typeof payload.space_id === "string" && payload.space_id !== existingDoc.space_id) {
      return {
        status: "failed",
        message: `Remote document workflow event ${operation.action} payload space does not match local document`,
      };
    }

    requireLocalSpace(existingDoc.space_id);
    const existingEvent = localDocumentWorkflowEventCache.find(
      (event) => event.id === operation.entity_id,
    );
    if (existingEvent) {
      const matchesExisting =
        existingEvent.document_workflow_id === payload.document_workflow_id &&
        existingEvent.document_id === payload.document_id &&
        existingEvent.from_stage_id === payload.from_stage_id &&
        existingEvent.to_stage_id === payload.to_stage_id &&
        existingEvent.action === payload.action &&
        existingEvent.note === payload.note &&
        existingEvent.created_by === payload.created_by &&
        existingEvent.created_at === payload.created_at;
      return matchesExisting
        ? {
            status: "skipped",
            message:
              "Remote document workflow event target already exists locally with matching state",
          }
        : {
            status: "blocked",
            message:
              "Remote document workflow event target already exists locally with different state",
          };
    }

    const workflow = localDocumentWorkflowCache.find(
      (item) =>
        item.id === payload.document_workflow_id && item.document_id === payload.document_id,
    );
    if (!workflow) {
      return {
        status: "failed",
        message: `Remote document workflow event ${operation.action} target workflow does not exist locally`,
      };
    }

    const template = localWorkflowTemplateCache.find(
      (item) => item.id === workflow.workflow_template_id && item.space_id === existingDoc.space_id,
    );
    if (!template) {
      return {
        status: "failed",
        message: `Remote document workflow event ${operation.action} target template does not exist locally`,
      };
    }
    if (!template.stages.some((stage) => stage.id === payload.to_stage_id)) {
      return {
        status: "failed",
        message: `Remote document workflow event ${operation.action} target stage does not exist locally`,
      };
    }
    if (
      typeof payload.from_stage_id === "string" &&
      !template.stages.some((stage) => stage.id === payload.from_stage_id)
    ) {
      return {
        status: "failed",
        message: `Remote document workflow event ${operation.action} source stage does not exist locally`,
      };
    }

    const importedEvent: VaultDocumentWorkflowEvent = {
      id: payload.id,
      document_workflow_id: payload.document_workflow_id,
      document_id: payload.document_id,
      from_stage_id: typeof payload.from_stage_id === "string" ? payload.from_stage_id : undefined,
      to_stage_id: payload.to_stage_id,
      action: payload.action,
      note: typeof payload.note === "string" ? payload.note : undefined,
      created_by: typeof payload.created_by === "string" ? payload.created_by : undefined,
      created_at: payload.created_at,
    };
    localDocumentWorkflowEventCache = [...localDocumentWorkflowEventCache, importedEvent];
    await persistLocalSnapshot();

    return {
      status: "applied",
      message: `Remote document workflow event ${operation.action} applied to the local vault`,
    };
  }

  if (operation.entity_type !== "document") {
    return {
      status: "blocked",
      message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
    };
  }

  const payload =
    operation.payload && typeof operation.payload === "object"
      ? (operation.payload as Partial<VaultDocument>)
      : null;

  if (operation.action === "delete") {
    if (typeof payload?.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: "Remote document delete payload ID does not match operation entity ID",
      };
    }

    const existing = localDocs.get(operation.entity_id);
    if (!existing) {
      return {
        status: "skipped",
        message: "Remote document delete target is already absent locally",
      };
    }

    if (typeof payload?.space_id === "string" && payload.space_id !== existing.space_id) {
      return {
        status: "failed",
        message: "Remote document delete payload space does not match local document",
      };
    }

    if (typeof payload?.space_id === "string") {
      requireLocalSpace(payload.space_id);
    } else {
      requireLocalSpace(existing.space_id);
    }

    localDocs.delete(operation.entity_id);
    removeLocalDocumentMetadata(new Set([operation.entity_id]));
    localSearchIndex = buildLocalSearchIndexEntries();
    await persistLocalSnapshot();

    return {
      status: "applied",
      message: "Remote document delete applied to the local vault",
    };
  }

  if (operation.action === "update") {
    if (!payload || typeof payload.space_id !== "string" || typeof payload.title !== "string") {
      return {
        status: "failed",
        message: "Remote document update payload is missing space or title",
      };
    }

    if (typeof payload.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: "Remote document update payload ID does not match operation entity ID",
      };
    }

    const existing = localDocs.get(operation.entity_id);
    if (!existing) {
      return {
        status: "failed",
        message: "Remote document update target does not exist locally",
      };
    }

    if (existing.space_id !== payload.space_id) {
      return {
        status: "failed",
        message: "Remote document update payload space does not match local document",
      };
    }

    requireLocalSpace(payload.space_id);
    let folderId = existing.folder_id;
    try {
      if ("folder_id" in payload) {
        folderId = normalizeLocalDocumentFolderId(payload.space_id, payload.folder_id);
      }
    } catch (error) {
      return {
        status: "blocked",
        message:
          error instanceof Error
            ? error.message
            : "Remote document update payload has an invalid folder",
      };
    }
    const updated: LocalDocState = {
      ...existing,
      title: normalizeTitle(payload.title),
      icon: typeof payload.icon === "string" ? payload.icon : existing.icon,
      cover_image:
        "cover_image" in payload
          ? typeof payload.cover_image === "string"
            ? payload.cover_image
            : undefined
          : existing.cover_image,
      folder_id: folderId,
      last_updated_by:
        typeof payload.last_updated_by === "string"
          ? payload.last_updated_by
          : existing.last_updated_by,
      updated_at:
        typeof payload.updated_at === "string" ? payload.updated_at : new Date().toISOString(),
    };

    localDocs.set(operation.entity_id, updated);
    localSearchIndex = buildLocalSearchIndexEntries();
    await localVault.saveDocument(updated);

    return {
      status: "applied",
      message: "Remote document update applied to the local vault",
    };
  }

  if (operation.action === "create") {
    if (!payload || typeof payload.space_id !== "string" || typeof payload.title !== "string") {
      return {
        status: "failed",
        message: "Remote document create payload is missing space or title",
      };
    }

    if (typeof payload.id === "string" && payload.id !== operation.entity_id) {
      return {
        status: "failed",
        message: "Remote document create payload ID does not match operation entity ID",
      };
    }

    requireLocalSpace(payload.space_id);
    const content = typeof payload.content === "string" ? payload.content : "";
    try {
      assertContentSize(content);
    } catch (error) {
      return {
        status: "failed",
        message: error instanceof Error ? error.message : "Remote document create content is invalid",
      };
    }
    let folderId: string | undefined;
    try {
      folderId = normalizeLocalDocumentFolderId(payload.space_id, payload.folder_id);
    } catch (error) {
      return {
        status: "blocked",
        message:
          error instanceof Error
            ? error.message
            : "Remote document create payload has an invalid folder",
      };
    }
    const now = new Date().toISOString();
    const created: LocalDocState = {
      id: operation.entity_id,
      space_id: payload.space_id,
      title: normalizeTitle(payload.title),
      content,
      icon: typeof payload.icon === "string" ? payload.icon : undefined,
      cover_image: typeof payload.cover_image === "string" ? payload.cover_image : undefined,
      folder_id: folderId,
      created_by: typeof payload.created_by === "string" ? payload.created_by : LOCAL_SYSTEM_USER,
      last_updated_by:
        typeof payload.last_updated_by === "string"
          ? payload.last_updated_by
          : typeof payload.created_by === "string"
            ? payload.created_by
            : LOCAL_SYSTEM_USER,
      created_at: typeof payload.created_at === "string" ? payload.created_at : now,
      updated_at: typeof payload.updated_at === "string" ? payload.updated_at : now,
    };

    const existing = localDocs.get(operation.entity_id);
    if (existing) {
      const matchesExisting =
        existing.space_id === created.space_id &&
        existing.title === created.title &&
        existing.content === created.content &&
        existing.icon === created.icon &&
        existing.cover_image === created.cover_image &&
        existing.folder_id === created.folder_id &&
        existing.created_by === created.created_by &&
        existing.last_updated_by === created.last_updated_by &&
        existing.created_at === created.created_at &&
        existing.updated_at === created.updated_at;
      return matchesExisting
        ? {
            status: "skipped",
            message: "Remote document create target already exists locally with matching content",
          }
        : {
            status: "blocked",
            message: "Remote document create target already exists locally with different content",
          };
    }

    localDocs.set(operation.entity_id, created);
    localSearchIndex = buildLocalSearchIndexEntries();
    await localVault.saveDocument(created);

    return {
      status: "applied",
      message: "Remote document create applied to the local vault",
    };
  }

  if (operation.action !== "update_content") {
    return {
      status: "blocked",
      message: `Remote import merge for ${operation.entity_type}:${operation.action} is not implemented yet`,
    };
  }

  if (!payload || typeof payload.content !== "string") {
    return {
      status: "failed",
      message: "Remote document content update payload is missing content",
    };
  }

  const existing = localDocs.get(operation.entity_id);
  if (!existing) {
    return {
      status: "failed",
      message: "Remote document content update target does not exist locally",
    };
  }

  if (typeof payload.id === "string" && payload.id !== operation.entity_id) {
    return {
      status: "failed",
      message: "Remote document content update payload ID does not match operation entity ID",
    };
  }

  if (typeof payload.space_id === "string" && payload.space_id !== existing.space_id) {
    return {
      status: "failed",
      message: "Remote document content update payload space does not match local document",
    };
  }

  let folderId = existing.folder_id;
  try {
    if ("folder_id" in payload) {
      folderId = normalizeLocalDocumentFolderId(existing.space_id, payload.folder_id);
    }
  } catch (error) {
    return {
      status: "blocked",
      message:
        error instanceof Error
          ? error.message
          : "Remote document content update payload has an invalid folder",
    };
  }

  try {
    assertContentSize(payload.content);
  } catch (error) {
    return {
      status: "failed",
      message:
        error instanceof Error ? error.message : "Remote document content update payload is invalid",
    };
  }
  const updated = {
    ...existing,
    content: payload.content,
    title: typeof payload.title === "string" ? normalizeTitle(payload.title) : existing.title,
    icon: typeof payload.icon === "string" ? payload.icon : existing.icon,
    cover_image:
      typeof payload.cover_image === "string" ? payload.cover_image : existing.cover_image,
    last_updated_by:
      typeof payload.last_updated_by === "string"
        ? payload.last_updated_by
        : existing.last_updated_by,
    folder_id: folderId,
    updated_at:
      typeof payload.updated_at === "string" ? payload.updated_at : new Date().toISOString(),
  };
  localDocs.set(operation.entity_id, updated);
  localSearchIndex = buildLocalSearchIndexEntries();
  await localVault.saveDocument(updated);

  return {
    status: "applied",
    message: "Remote document content update applied to the local vault",
  };
}

function describeUnavailableReplayPayload(
  operation: LocalSyncReplayOperationRecord,
): LocalSyncReplayUnavailablePayloadRecord {
  const payload =
    operation.payload && typeof operation.payload === "object"
      ? (operation.payload as Record<string, unknown>)
      : {};
  const sourceUrl =
    typeof payload.url === "string"
      ? payload.url
      : typeof payload.source_url === "string"
        ? payload.source_url
        : undefined;
  const vaultPath = typeof payload.vault_path === "string" ? payload.vault_path : undefined;
  const nativePackError =
    typeof payload.native_pack_error === "string" ? payload.native_pack_error : undefined;
  const byteSourcesChecked = [vaultPath, sourceUrl]
    .filter((value): value is string => Boolean(value))
    .filter((value, index, values) => values.indexOf(value) === index);

  return {
    operation_id: operation.id,
    entity_type: operation.entity_type,
    entity_id: operation.entity_id,
    action: operation.action,
    reason:
      operation.entity_type === "asset" && operation.action === "create"
        ? (nativePackError ?? "Asset binary bytes are unavailable for replay")
        : "Replay payload is unavailable",
    asset_name: typeof payload.name === "string" ? payload.name : undefined,
    document_id: typeof payload.document_id === "string" ? payload.document_id : undefined,
    source_url: sourceUrl,
    vault_path: vaultPath,
    byte_sources_checked: byteSourcesChecked.length > 0 ? byteSourcesChecked : undefined,
    recovery_action:
      operation.entity_type === "asset" && operation.action === "create"
        ? "Restore the asset file under the active vault or reattach the asset so replay can embed bytes."
        : undefined,
  };
}

async function rebuildProviderImportReviewWithLocalQueue(
  result: ProviderSyncResult,
): Promise<ProviderSyncResult> {
  const attempt = result.remote_mutation_attempt;
  const providerSyncManifest = attempt?.provider_sync_manifest;
  if (!attempt || attempt.direction !== "pull" || !providerSyncManifest) return result;

  const localQueuedOperations = await localVault.listQueuedSyncOperations();
  const importDryRun = buildProviderRemoteImportDryRun(
    providerSyncManifest,
    localQueuedOperations.map((operation) => operation.id),
    localQueuedOperations.map((operation) => ({
      id: operation.id,
      entity_type: operation.entity_type,
      entity_id: operation.entity_id,
      action: operation.action,
      payload: operation.payload,
    })),
  );
  const importApplyPlan = buildProviderRemoteImportApplyPlan(importDryRun);

  return {
    ...result,
    conflicts: Math.max(result.conflicts, importDryRun.conflict_count),
    remote_mutation_attempt: {
      ...attempt,
      import_dry_run: importDryRun,
      import_apply_plan: importApplyPlan,
    },
  };
}

export const dataClient = {
  async hydrateStaticSiteExportAssets(snapshot: LocalVaultSnapshot): Promise<LocalVaultSnapshot> {
    return hydrateStaticSiteExportAssets(snapshot);
  },
  async fetchBookmarkMetadata(url: string): Promise<BookmarkMetadataRecord> {
    if (inLocalMode()) {
      throw new Error("Bookmark previews require a connected Starlane Notes server");
    }
    return api.fetch<BookmarkMetadataRecord>("/bookmarks/metadata", {
      method: "POST",
      body: { url },
    });
  },

  async listSpaceMembers(spaceId: string): Promise<SpaceMemberRecord[]> {
    if (!inLocalMode()) return api.fetch<SpaceMemberRecord[]>(`/spaces/${spaceId}/members`);
    await ensureLocalData();
    requireLocalSpace(spaceId);
    return [{ user_id: LOCAL_SYSTEM_USER_ID, display_name: LOCAL_SYSTEM_USER, email: "local@starlane", role: "owner" }];
  },

  async listSpaces(): Promise<SpaceRecord[]> {
    if (!inLocalMode()) return api.fetch<SpaceRecord[]>("/spaces");
    await ensureLocalData();
    return Array.from(localSpaces.values()).map(localSpaceToRecord);
  },

  async getSpace(spaceId: string): Promise<SpaceRecord> {
    if (!inLocalMode()) return api.fetch<SpaceRecord>(`/spaces/${spaceId}`);
    await ensureLocalData();
    const space = requireLocalSpace(spaceId);
    return localSpaceToRecord(space);
  },

  async createSpace(name: string, importOptions: ImportIdentityOptions = {}): Promise<SpaceRecord> {
    if (!inLocalMode()) {
      return api.fetch<SpaceRecord>("/spaces", {
        method: "POST",
        body: { name, visibility: "private", ...importOptions },
      });
    }
    await ensureLocalData();
    const normalizedName = name.trim();
    if (!normalizedName) throw new Error("Space name is required");
    const id = makeId("space");
    const now = new Date().toISOString();
    const slug = normalizedName
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9-]/g, "");
    localSpaces.set(id, {
      id,
      name: normalizedName,
      slug: slug || id,
      created_at: now,
      updated_at: now,
    });
    await persistLocalSnapshot({ entity_type: "space", entity_id: id, action: "create" });
    return localSpaceToRecord(requireLocalSpace(id));
  },

  async updateSpace(spaceId: string, patch: { name?: string }): Promise<SpaceRecord> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}`, {
        method: "PUT",
        body: patch,
      });
      return this.getSpace(spaceId);
    }
    await ensureLocalData();
    const space = requireLocalSpace(spaceId);
    const normalizedName = patch.name?.trim();
    if (!normalizedName) throw new Error("Space name is required");
    const now = new Date().toISOString();
    const slug = normalizedName
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9-]/g, "");
    const updated = {
      ...space,
      name: normalizedName,
      slug: slug || space.slug || space.id,
      updated_at: now,
    };
    localSpaces.set(spaceId, updated);
    await persistLocalSnapshot({ entity_type: "space", entity_id: spaceId, action: "update" });
    return localSpaceToRecord(updated);
  },

  async listFolders(spaceId: string): Promise<FolderRecord[]> {
    if (!inLocalMode()) return api.fetch<FolderRecord[]>(`/spaces/${spaceId}/folders`);
    await ensureLocalData();
    requireLocalSpace(spaceId);
    return Array.from(localFolders.values())
      .filter((f) => f.space_id === spaceId)
      .map((f) => ({ id: f.id, name: f.name, parent_id: f.parent_id }));
  },

  async createFolder(
    spaceId: string,
    name: string,
    options: { parent_id?: string | null } & ImportIdentityOptions = {},
  ): Promise<FolderRecord> {
    if (!inLocalMode()) {
      const sourceId = options.source_id ?? makeId("folder");
      const created = await api.fetch<FolderRecord>(`/spaces/${spaceId}/folders`, {
        method: "POST",
        body: {
          name,
          parent_id: options.parent_id ?? null,
          source_id: sourceId,
          created_at: options.created_at,
          updated_at: options.updated_at,
        },
      });
      return created ?? { id: sourceId, name, parent_id: options.parent_id ?? undefined };
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const normalizedName = name.trim();
    if (!normalizedName) throw new Error("Folder name is required");
    const parentId = normalizeLocalFolderParentId(spaceId, options.parent_id);
    const id = options.source_id ?? makeId("folder");
    localFolders.set(id, {
      id,
      space_id: spaceId,
      name: normalizedName,
      parent_id: parentId,
    });
    await persistLocalSnapshot({ entity_type: "folder", entity_id: id, action: "create" });
    return { id, name: normalizedName, parent_id: parentId };
  },

  async updateFolder(
    spaceId: string,
    folderId: string,
    patch: { name?: string; parent_id?: string | null },
  ): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/folders/${folderId}`, {
        method: "PUT",
        body: patch,
      });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const folder = localFolders.get(folderId);
    if (!folder || folder.space_id !== spaceId) throw new Error("Folder not found");
    const nextName = patch.name ? patch.name.trim() : folder.name;
    if (!nextName) throw new Error("Folder name is required");
    const nextParentId = Object.prototype.hasOwnProperty.call(patch, "parent_id")
      ? normalizeLocalFolderParentId(spaceId, patch.parent_id, folderId)
      : folder.parent_id;
    localFolders.set(folderId, { ...folder, name: nextName, parent_id: nextParentId });
    await persistLocalSnapshot({ entity_type: "folder", entity_id: folderId, action: "update" });
  },

  async moveFolder(spaceId: string, folderId: string, input: MoveFolderInput): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/folders/${folderId}/move`, {
        method: "POST",
        body: input,
      });
      return;
    }

    await ensureLocalData();
    requireLocalSpace(spaceId);
    requireLocalSpace(input.target_space_id);
    const folder = requireLocalFolder(spaceId, folderId);
    const sourceSubtreeDepth = getLocalFolderSubtreeRelativeDepth(spaceId, folderId);
    const nextParentId = normalizeLocalFolderParentId(
      input.target_space_id,
      input.parent_id,
      input.target_space_id === spaceId ? folderId : undefined,
    );
    if (nextParentId && input.target_space_id !== spaceId) {
      const targetDepth = getLocalFolderDepth(input.target_space_id, nextParentId);
      if (targetDepth + sourceSubtreeDepth > MAX_LOCAL_FOLDER_DEPTH) {
        throw new Error(`Folders can be nested up to ${MAX_LOCAL_FOLDER_DEPTH} levels`);
      }
    }
    const folderIds = collectLocalFolderSubtreeIds(spaceId, folderId);
    const movedAt = new Date().toISOString();

    for (const id of folderIds) {
      const current = localFolders.get(id);
      if (!current) continue;
      localFolders.set(id, {
        ...current,
        space_id: input.target_space_id,
        parent_id: id === folder.id ? nextParentId : current.parent_id,
      });
    }

    for (const doc of Array.from(localDocs.values())) {
      if (doc.space_id !== spaceId || !doc.folder_id || !folderIds.has(doc.folder_id)) continue;
      localDocs.set(doc.id, {
        ...doc,
        space_id: input.target_space_id,
        last_updated_by: LOCAL_SYSTEM_USER,
        updated_at: movedAt,
      });
    }

    await persistLocalSnapshot({ entity_type: "folder", entity_id: folderId, action: "update" });
  },

  async deleteFolder(spaceId: string, folderId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/folders/${folderId}`, { method: "DELETE" });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const folder = localFolders.get(folderId);
    if (!folder || folder.space_id !== spaceId) throw new Error("Folder not found");

    const folderIds = new Set<string>([folderId]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const candidate of localFolders.values()) {
        if (
          candidate.space_id === spaceId &&
          candidate.parent_id &&
          folderIds.has(candidate.parent_id) &&
          !folderIds.has(candidate.id)
        ) {
          folderIds.add(candidate.id);
          changed = true;
        }
      }
    }

    for (const id of folderIds) {
      localFolders.delete(id);
    }
    const documentIds = Array.from(localDocs.values())
      .filter((doc) => doc.space_id === spaceId && doc.folder_id && folderIds.has(doc.folder_id))
      .map((doc) => doc.id);
    for (const id of documentIds) {
      localDocs.delete(id);
    }
    await persistLocalSnapshot({
      entity_type: "folder",
      entity_id: folderId,
      action: "delete",
      payload: folder,
    });
  },

  async listDocuments(
    spaceId: string,
    options: ListDocumentsOptions = {},
  ): Promise<DocumentRecord[]> {
    if (!inLocalMode()) {
      const params = new URLSearchParams();
      if (options.folder_id) params.set("folder_id", options.folder_id);
      if (options.include_nested) params.set("include_nested", "true");
      if (options.workflow_status) params.set("workflow_status", options.workflow_status);
      if (options.workflow_stage_id) params.set("workflow_stage_id", options.workflow_stage_id);
      if (options.workflow_template_id) {
        params.set("workflow_template_id", options.workflow_template_id);
      }
      const suffix = params.toString() ? `?${params}` : "";
      const docs = await api.fetch<DocumentRecord[]>(`/spaces/${spaceId}/documents${suffix}`);
      return docs.map((doc) => ({
        ...doc,
        folder_id: doc.folder_id ?? undefined,
        workflow_template_id: doc.workflow_template_id ?? undefined,
        workflow_template_name: doc.workflow_template_name ?? undefined,
        workflow_stage_id: doc.workflow_stage_id ?? undefined,
        workflow_stage_name: doc.workflow_stage_name ?? undefined,
        workflow_status: doc.workflow_status ?? undefined,
      }));
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    return Array.from(localDocs.values())
      .filter((d) => d.space_id === spaceId)
      .filter((d) => documentMatchesListOptions(d, options))
      .sort((a, b) => (a.updated_at < b.updated_at ? 1 : -1))
      .map((d) => ({
        id: d.id,
        title: d.title,
        icon: d.icon,
        folder_id: d.folder_id,
        parent_id: d.parent_id,
        ...workflowSummaryForDocument(d.id),
      }));
  },

  async createDocument(
    spaceId: string,
    input: {
      title: string;
      folder_id?: string | null;
      parent_id?: string | null;
      content?: string;
      template_id?: string;
    } & ImportIdentityOptions,
  ): Promise<{ id: string }> {
    if (!inLocalMode()) {
      const normalizedTitle = normalizeTitle(input.title);
      const normalizedContent = input.content ?? "";
      assertContentSize(normalizedContent);
      const sourceId = input.source_id ?? makeId("doc");
      const created = await api.fetch<{ id: string }>(`/spaces/${spaceId}/documents`, {
        method: "POST",
        body: {
          title: normalizedTitle,
          folder_id: input.folder_id ?? null,
          parent_id: input.parent_id ?? null,
          content: normalizedContent,
          template_id: input.template_id,
          source_id: sourceId,
          created_at: input.created_at,
          updated_at: input.updated_at,
        },
      });
      return created ?? { id: sourceId };
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const normalizedTitle = normalizeTitle(input.title);
    const normalizedContent = input.content ?? "";
    assertContentSize(normalizedContent);
    const folderId = normalizeLocalDocumentFolderId(spaceId, input.folder_id);
    const parentId = normalizeLocalDocumentParentId(spaceId, input.parent_id, folderId);
    const id = makeId("doc");
    const now = new Date().toISOString();
    const template = input.template_id
      ? localTemplateCache.find((item) => item.id === input.template_id && item.space_id === spaceId)
      : undefined;
    if (input.template_id && !template) throw new Error("Template not found");
    let templateTagIds: string[] = [];
    let templatePropertyValues: VaultPropertyValue[] = [];
    localDocs.set(id, {
      id,
      space_id: spaceId,
      title: normalizedTitle,
      content: normalizedContent,
      folder_id: folderId,
      parent_id: parentId,
      created_by: LOCAL_SYSTEM_USER,
      last_updated_by: LOCAL_SYSTEM_USER,
      created_at: now,
      updated_at: now,
    });
    if (template) {
      const tagNames = new Set((template.tags ?? []).map((tag) => tag.trim().toLowerCase()));
      templateTagIds = localTags
        .filter((tag) => tag.space_id === spaceId && tagNames.has(tag.name.trim().toLowerCase()))
        .map((tag) => tag.id);
      localDocTags = [
        ...localDocTags,
        ...templateTagIds.map((tag_id) => ({ document_id: id, tag_id })),
      ];
      const values =
        template.properties &&
        typeof template.properties === "object" &&
        !Array.isArray(template.properties)
          ? Object.entries(template.properties as Record<string, unknown>)
          : [];
      for (const [name, value] of values) {
        if (value === null) continue;
        const schema = localPropertySchemas.find(
          (item) =>
            item.space_id === spaceId &&
            item.name.trim().toLowerCase() === name.trim().toLowerCase(),
        );
        if (!schema) continue;
        const serialized = typeof value === "string" ? value : JSON.stringify(value);
        if (serialized === undefined) continue;
        templatePropertyValues = [
          ...templatePropertyValues,
          {
            id: makeId("propval"),
            document_id: id,
            property_id: schema.id,
            value: serialized,
            created_at: now,
            updated_at: now,
          },
        ];
      }
      localPropertyValues = [...localPropertyValues, ...templatePropertyValues];
    }
    const queuedAt = Date.now();
    await persistLocalSnapshot({
      entity_type: "document",
      entity_id: id,
      action: "create",
      queued_at: new Date(queuedAt).toISOString(),
    });
    for (const [index, tagId] of templateTagIds.entries()) {
      await persistLocalSnapshot({
        entity_type: "document_tag",
        entity_id: `${id}:${tagId}`,
        action: "add",
        queued_at: new Date(queuedAt + index + 1).toISOString(),
        payload: { document_id: id, tag_id: tagId, space_id: spaceId },
      });
    }
    if (templatePropertyValues.length > 0) {
      await persistLocalSnapshot({
        entity_type: "document_property_values",
        entity_id: id,
        action: "set",
        queued_at: new Date(queuedAt + templateTagIds.length + 1).toISOString(),
        payload: { document_id: id, space_id: spaceId, values: templatePropertyValues },
      });
    }
    return { id };
  },

  async listTemplates(spaceId: string): Promise<TemplateRecord[]> {
    if (!inLocalMode()) return api.fetch<TemplateRecord[]>(`/spaces/${spaceId}/templates`);
    await ensureLocalData();
    requireLocalSpace(spaceId);
    return localTemplateCache
      .filter((template) => template.space_id === spaceId)
      .sort((a, b) => a.name.localeCompare(b.name))
      .map(toTemplateRecord);
  },

  async getTemplate(spaceId: string, templateId: string): Promise<TemplateRecord> {
    if (!inLocalMode()) {
      return api.fetch<TemplateRecord>(`/spaces/${spaceId}/templates/${templateId}`);
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const template = localTemplateCache.find(
      (item) => item.id === templateId && item.space_id === spaceId,
    );
    if (!template) throw new Error("Template not found");
    return toTemplateRecord(template);
  },

  async createTemplate(
    spaceId: string,
    input: TemplateInput & ImportIdentityOptions,
  ): Promise<{ id: string }> {
    const normalized = normalizeTemplateInput(input);
    if (!inLocalMode()) {
      return api.fetch<{ id: string }>(`/spaces/${spaceId}/templates`, {
        method: "POST",
        body: {
          ...normalized,
          source_id: input.source_id,
          created_at: input.created_at,
          updated_at: input.updated_at,
        },
      });
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const now = new Date().toISOString();
    const template: VaultTemplate = {
      id: input.source_id ?? makeId("template"),
      space_id: spaceId,
      name: normalized.name,
      description: normalized.description,
      content: normalized.content,
      properties: normalized.properties,
      tags: normalized.tags ?? [],
      created_at: now,
      updated_at: now,
    };
    localTemplateCache = [...localTemplateCache, template];
    await localVault.enqueueSyncOperation({
      entity_type: "template",
      entity_id: template.id,
      action: "create",
      payload: template,
    });
    await localVault.saveTemplate(template);
    return { id: template.id };
  },

  async updateTemplate(spaceId: string, templateId: string, input: TemplateInput): Promise<void> {
    const normalized = normalizeTemplateInput(input);
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/templates/${templateId}`, {
        method: "PUT",
        body: normalized,
      });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const existing = localTemplateCache.find(
      (item) => item.id === templateId && item.space_id === spaceId,
    );
    if (!existing) throw new Error("Template not found");
    const updated: VaultTemplate = {
      ...existing,
      name: normalized.name,
      description: normalized.description,
      content: normalized.content,
      properties: normalized.properties,
      tags: normalized.tags ?? [],
      updated_at: new Date().toISOString(),
    };
    localTemplateCache = localTemplateCache.map((item) =>
      item.id === templateId ? updated : item,
    );
    await localVault.enqueueSyncOperation({
      entity_type: "template",
      entity_id: templateId,
      action: "update",
      payload: updated,
    });
    await localVault.saveTemplate(updated);
  },

  async deleteTemplate(spaceId: string, templateId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/templates/${templateId}`, { method: "DELETE" });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const existing = localTemplateCache.find(
      (item) => item.id === templateId && item.space_id === spaceId,
    );
    if (!existing) throw new Error("Template not found");
    localTemplateCache = localTemplateCache.filter((item) => item.id !== templateId);
    await localVault.enqueueSyncOperation({
      entity_type: "template",
      entity_id: templateId,
      action: "delete",
      payload: existing,
    });
    await localVault.deleteTemplate(templateId);
  },

  async listWorkflowTemplates(spaceId: string): Promise<WorkflowTemplateRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<WorkflowTemplateRecord[]>(`/spaces/${spaceId}/workflow-templates`);
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    return localWorkflowTemplateCache
      .filter((template) => template.space_id === spaceId)
      .sort((a, b) => a.name.localeCompare(b.name))
      .map(toWorkflowTemplateRecord);
  },

  async getWorkflowTemplate(spaceId: string, templateId: string): Promise<WorkflowTemplateRecord> {
    if (!inLocalMode()) {
      return api.fetch<WorkflowTemplateRecord>(
        `/spaces/${spaceId}/workflow-templates/${templateId}`,
      );
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const template = localWorkflowTemplateCache.find(
      (item) => item.id === templateId && item.space_id === spaceId,
    );
    if (!template) throw new Error("Workflow template not found");
    return toWorkflowTemplateRecord(template);
  },

  async createWorkflowTemplate(
    spaceId: string,
    input: WorkflowTemplateInput & ImportIdentityOptions,
  ): Promise<{ id: string }> {
    const normalized = normalizeWorkflowTemplateInput(input);
    if (!inLocalMode()) {
      return api.fetch<{ id: string }>(`/spaces/${spaceId}/workflow-templates`, {
        method: "POST",
        body: {
          ...normalized,
          source_id: input.source_id,
          created_at: input.created_at,
          updated_at: input.updated_at,
        },
      });
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const now = new Date().toISOString();
    const template: VaultWorkflowTemplate = {
      id: input.source_id ?? makeId("workflow-template"),
      space_id: spaceId,
      name: normalized.name,
      description: normalized.description,
      stages: normalized.stages,
      approvers: normalized.approvers ?? [],
      created_by: LOCAL_SYSTEM_USER,
      created_at: now,
      updated_at: now,
    };
    localWorkflowTemplateCache = [...localWorkflowTemplateCache, template];
    await localVault.enqueueSyncOperation({
      entity_type: "workflow_template",
      entity_id: template.id,
      action: "create",
      payload: template,
    });
    await localVault.saveWorkflowTemplate(template);
    return { id: template.id };
  },

  async updateWorkflowTemplate(
    spaceId: string,
    templateId: string,
    input: WorkflowTemplateInput,
  ): Promise<void> {
    const normalized = normalizeWorkflowTemplateInput(input);
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/workflow-templates/${templateId}`, {
        method: "PUT",
        body: normalized,
      });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const existing = localWorkflowTemplateCache.find(
      (item) => item.id === templateId && item.space_id === spaceId,
    );
    if (!existing) throw new Error("Workflow template not found");
    const updated: VaultWorkflowTemplate = {
      ...existing,
      name: normalized.name,
      description: normalized.description,
      stages: normalized.stages,
      approvers: normalized.approvers ?? [],
      updated_at: new Date().toISOString(),
    };
    localWorkflowTemplateCache = localWorkflowTemplateCache.map((item) =>
      item.id === templateId ? updated : item,
    );
    await localVault.enqueueSyncOperation({
      entity_type: "workflow_template",
      entity_id: templateId,
      action: "update",
      payload: updated,
    });
    await localVault.saveWorkflowTemplate(updated);
  },

  async deleteWorkflowTemplate(spaceId: string, templateId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/workflow-templates/${templateId}`, { method: "DELETE" });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const existing = localWorkflowTemplateCache.find(
      (item) => item.id === templateId && item.space_id === spaceId,
    );
    if (!existing) throw new Error("Workflow template not found");
    localWorkflowTemplateCache = localWorkflowTemplateCache.filter(
      (item) => item.id !== templateId,
    );
    await localVault.enqueueSyncOperation({
      entity_type: "workflow_template",
      entity_id: templateId,
      action: "delete",
      payload: existing,
    });
    await localVault.deleteWorkflowTemplate(templateId);
  },

  async getDocumentWorkflow(
    spaceId: string,
    docId: string,
  ): Promise<DocumentWorkflowRecord | null> {
    if (!inLocalMode()) {
      return api.fetch<DocumentWorkflowRecord | null>(
        `/spaces/${spaceId}/documents/${docId}/workflow`,
      );
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    const workflow =
      localDocumentWorkflowCache.find((item) => item.document_id === docId) ??
      (await localVault.getDocumentWorkflow(docId));
    if (!workflow) return null;
    const template = localWorkflowTemplateCache.find(
      (item) => item.id === workflow.workflow_template_id && item.space_id === spaceId,
    );
    if (!template) return null;
    return toDocumentWorkflowRecord(workflow, template);
  },

  async enableDocumentWorkflow(
    spaceId: string,
    docId: string,
    input: EnableDocumentWorkflowInput,
  ): Promise<DocumentWorkflowRecord> {
    if (!inLocalMode()) {
      return api.fetch<DocumentWorkflowRecord>(`/spaces/${spaceId}/documents/${docId}/workflow`, {
        method: "PUT",
        body: input,
      });
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    const template = localWorkflowTemplateCache.find(
      (item) => item.id === input.template_id && item.space_id === spaceId,
    );
    if (!template) throw new Error("Workflow template not found");
    const stageId = input.stage_id ?? template.stages[0]?.id;
    if (!stageId || !template.stages.some((stage) => stage.id === stageId)) {
      throw new Error("Workflow stage not found");
    }
    const now = new Date().toISOString();
    const existing = localDocumentWorkflowCache.find((item) => item.document_id === docId);
    const workflow: VaultDocumentWorkflow = {
      id: existing?.id ?? makeId("document-workflow"),
      document_id: docId,
      workflow_template_id: template.id,
      current_stage_id: stageId,
      status: "active",
      created_by: existing?.created_by ?? LOCAL_SYSTEM_USER,
      created_at: existing?.created_at ?? now,
      updated_at: now,
    };
    localDocumentWorkflowCache = [
      ...localDocumentWorkflowCache.filter((item) => item.document_id !== docId),
      workflow,
    ];
    const event: VaultDocumentWorkflowEvent = {
      id: makeId("workflow-event"),
      document_workflow_id: workflow.id,
      document_id: docId,
      to_stage_id: stageId,
      action: "enable",
      created_by: LOCAL_SYSTEM_USER,
      created_at: now,
    };
    localDocumentWorkflowEventCache = [
      ...localDocumentWorkflowEventCache.filter(
        (item) => item.document_workflow_id !== workflow.id || item.action !== "enable",
      ),
      event,
    ];
    await persistLocalSnapshot({
      entity_type: "document_workflow",
      entity_id: workflow.id,
      action: existing ? "update" : "create",
      payload: { ...workflow, space_id: spaceId, event },
    });
    return toDocumentWorkflowRecord(workflow, template);
  },

  async disableDocumentWorkflow(spaceId: string, docId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/workflow`, { method: "DELETE" });
      return;
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    const existing = localDocumentWorkflowCache.find((item) => item.document_id === docId);
    localDocumentWorkflowCache = localDocumentWorkflowCache.filter(
      (item) => item.document_id !== docId,
    );
    await localVault.enqueueSyncOperation({
      entity_type: "document_workflow",
      entity_id: docId,
      action: "delete",
      payload: existing
        ? { ...existing, space_id: spaceId }
        : { document_id: docId, space_id: spaceId },
    });
    await localVault.deleteDocumentWorkflow(docId);
  },

  async transitionDocumentWorkflow(
    spaceId: string,
    docId: string,
    input: TransitionDocumentWorkflowInput,
  ): Promise<DocumentWorkflowRecord> {
    if (!inLocalMode()) {
      return api.fetch<DocumentWorkflowRecord>(
        `/spaces/${spaceId}/documents/${docId}/workflow/transitions`,
        {
          method: "POST",
          body: input,
        },
      );
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    const workflow = localDocumentWorkflowCache.find((item) => item.document_id === docId);
    if (!workflow) throw new Error("Document workflow not found");
    const template = localWorkflowTemplateCache.find(
      (item) => item.id === workflow.workflow_template_id && item.space_id === spaceId,
    );
    if (!template) throw new Error("Workflow template not found");
    const stageId = input.stage_id.trim();
    if (!stageId || !template.stages.some((stage) => stage.id === stageId)) {
      throw new Error("Workflow stage not found");
    }
    if (
      !workflowApproverAllowsTransition(
        template.approvers,
        stageId,
        LOCAL_SYSTEM_USER,
        LOCAL_SYSTEM_ROLE,
      )
    ) {
      throw new Error("Workflow transition requires an assigned approver");
    }
    const now = new Date().toISOString();
    const updated: VaultDocumentWorkflow = {
      ...workflow,
      current_stage_id: stageId,
      status: isFinalWorkflowStage(template, stageId) ? "completed" : "active",
      updated_at: now,
    };
    const event: VaultDocumentWorkflowEvent = {
      id: makeId("workflow-event"),
      document_workflow_id: workflow.id,
      document_id: docId,
      from_stage_id: workflow.current_stage_id,
      to_stage_id: stageId,
      action: "transition",
      note: input.note?.trim() || undefined,
      created_by: LOCAL_SYSTEM_USER,
      created_at: now,
    };
    localDocumentWorkflowCache = localDocumentWorkflowCache.map((item) =>
      item.document_id === docId ? updated : item,
    );
    localDocumentWorkflowEventCache = [...localDocumentWorkflowEventCache, event];
    await persistLocalSnapshot({
      entity_type: "document_workflow",
      entity_id: workflow.id,
      action: "transition",
      payload: { ...updated, space_id: spaceId, event },
    });
    return toDocumentWorkflowRecord(updated, template);
  },

  async listDocumentWorkflowEvents(
    spaceId: string,
    docId: string,
  ): Promise<DocumentWorkflowEventRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<DocumentWorkflowEventRecord[]>(
        `/spaces/${spaceId}/documents/${docId}/workflow/events`,
      );
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    return localDocumentWorkflowEventCache
      .filter((event) => event.document_id === docId)
      .sort((a, b) => b.created_at.localeCompare(a.created_at));
  },

  async recordDocumentWorkflowEvent(
    spaceId: string,
    docId: string,
    input: RecordDocumentWorkflowEventInput,
  ): Promise<DocumentWorkflowEventRecord[]> {
    const note = input.note?.trim() ?? "";
    if (!note && !input.source_id) throw new Error("Workflow event note is required");
    if (!inLocalMode()) {
      return api.fetch<DocumentWorkflowEventRecord[]>(
        `/spaces/${spaceId}/documents/${docId}/workflow/events`,
        {
          method: "POST",
          body: {
            action: input.action,
            note,
            source_id: input.source_id,
            created_by: input.created_by,
            created_at: input.created_at,
            from_stage_id: input.from_stage_id,
            to_stage_id: input.to_stage_id,
          },
        },
      );
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    const workflow = localDocumentWorkflowCache.find((item) => item.document_id === docId);
    if (!workflow) throw new Error("Document workflow not found");
    const now = new Date().toISOString();
    const event: VaultDocumentWorkflowEvent = {
      id: input.source_id?.trim() || makeId("workflow-event"),
      document_workflow_id: workflow.id,
      document_id: docId,
      from_stage_id: input.from_stage_id?.trim() || undefined,
      to_stage_id: input.to_stage_id?.trim() || workflow.current_stage_id,
      action: input.action,
      note: note || undefined,
      created_by: input.created_by?.trim() || LOCAL_SYSTEM_USER,
      created_at: input.created_at?.trim() || now,
    };
    localDocumentWorkflowEventCache = [...localDocumentWorkflowEventCache, event];
    await persistLocalSnapshot({
      entity_type: "document_workflow_event",
      entity_id: event.id,
      action: input.action,
      payload: { ...event, space_id: spaceId },
    });
    return localDocumentWorkflowEventCache
      .filter((item) => item.document_id === docId)
      .sort((a, b) => b.created_at.localeCompare(a.created_at));
  },

  async listPropertySchemas(spaceId: string): Promise<PropertySchemaRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<PropertySchemaRecord[]>(`/spaces/${spaceId}/properties`);
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    return localPropertySchemas
      .filter((schema) => schema.space_id === spaceId)
      .sort((a, b) => a.sort_order - b.sort_order)
      .map(toPropertySchemaRecord);
  },

  async createPropertySchema(
    spaceId: string,
    input: PropertySchemaInput & Pick<ImportIdentityOptions, "source_id" | "created_at">,
  ): Promise<PropertySchemaRecord> {
    const normalized = normalizePropertySchemaInput(input);
    if (!inLocalMode()) {
      return api.fetch<PropertySchemaRecord>(`/spaces/${spaceId}/properties`, {
        method: "POST",
        body: {
          ...normalized,
          source_id: input.source_id,
          created_at: input.created_at,
        },
      });
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const sortOrder =
      Math.max(
        0,
        ...localPropertySchemas
          .filter((schema) => schema.space_id === spaceId)
          .map((schema) => schema.sort_order),
      ) + 1;
    const schema: VaultPropertySchema = {
      id: input.source_id ?? makeId("prop"),
      space_id: spaceId,
      name: normalized.name,
      property_type: normalized.property_type,
      options: normalized.options,
      sort_order: sortOrder,
    };
    localPropertySchemas = [...localPropertySchemas, schema];
    await persistLocalSnapshot({
      entity_type: "property_schema",
      entity_id: schema.id,
      action: "create",
      payload: schema,
    });
    return toPropertySchemaRecord(schema);
  },

  async updatePropertySchema(
    spaceId: string,
    propertyId: string,
    input: PropertySchemaInput,
  ): Promise<void> {
    const normalized = normalizePropertySchemaInput(input);
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/properties/${propertyId}`, {
        method: "PUT",
        body: normalized,
      });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    let updatedSchema: VaultPropertySchema | null = null;
    localPropertySchemas = localPropertySchemas.map((schema) => {
      if (schema.id !== propertyId || schema.space_id !== spaceId) return schema;
      updatedSchema = {
        ...schema,
        name: normalized.name,
        property_type: normalized.property_type,
        options: normalized.options,
      };
      return updatedSchema;
    });
    await persistLocalSnapshot({
      entity_type: "property_schema",
      entity_id: propertyId,
      action: "update",
      payload: updatedSchema,
    });
  },

  async reorderPropertySchemas(spaceId: string, input: ReorderPropertySchemasInput): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/properties/reorder`, {
        method: "PUT",
        body: input,
      });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const orderById = new Map(input.property_ids.map((id, index) => [id, index + 1]));
    localPropertySchemas = localPropertySchemas.map((schema) => {
      const nextOrder = schema.space_id === spaceId ? orderById.get(schema.id) : undefined;
      return nextOrder ? { ...schema, sort_order: nextOrder } : schema;
    });
    await persistLocalSnapshot({
      entity_type: "property_schema",
      entity_id: spaceId,
      action: "reorder",
      payload: {
        space_id: spaceId,
        property_ids: input.property_ids,
        schemas: localPropertySchemas.filter((schema) => schema.space_id === spaceId),
      },
    });
  },

  async deletePropertySchema(spaceId: string, propertyId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/properties/${propertyId}`, { method: "DELETE" });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const deletedSchema = localPropertySchemas.find(
      (schema) => schema.id === propertyId && schema.space_id === spaceId,
    );
    localPropertySchemas = localPropertySchemas.filter(
      (schema) => !(schema.id === propertyId && schema.space_id === spaceId),
    );
    localPropertyValues = localPropertyValues.filter((value) => value.property_id !== propertyId);
    await persistLocalSnapshot({
      entity_type: "property_schema",
      entity_id: propertyId,
      action: "delete",
      payload: deletedSchema ?? { id: propertyId, space_id: spaceId },
    });
  },

  async listDocumentProperties(
    spaceId: string,
    docId: string,
  ): Promise<DocumentPropertyValueRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<DocumentPropertyValueRecord[]>(
        `/spaces/${spaceId}/documents/${docId}/properties`,
      );
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    const schemas = localPropertySchemas
      .filter((schema) => schema.space_id === spaceId)
      .sort((a, b) => a.sort_order - b.sort_order);
    return schemas.map((schema) => {
      const value = localPropertyValues.find(
        (entry) => entry.document_id === docId && entry.property_id === schema.id,
      );
      return {
        property_id: schema.id,
        property_name: schema.name,
        property_type: schema.property_type,
        options: schema.options,
        value: value?.value ?? null,
      };
    });
  },

  async listDocumentBacklinks(spaceId: string, docId: string): Promise<BacklinkRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<BacklinkRecord[]>(`/spaces/${spaceId}/documents/${docId}/backlinks`);
    }
    await ensureLocalData();
    const target = requireLocalDoc(spaceId, docId);
    const targetTitle = target.title.trim().toLowerCase();
    const spaceName = localSpaces.get(spaceId)?.name ?? "Current space";

    return Array.from(localDocs.values())
      .filter((document) => {
        if (document.id === docId || document.space_id !== spaceId) return false;
        const wikiLink = new RegExp(`\\[\\[${escapeRegExp(target.title.trim())}(?:\\||\\]\\])`, "i");
        const documentLink = new RegExp(`\\[\\[[^\\]]*${escapeRegExp(docId)}[^\\]]*\\]\\]`, "i");
        return (
          wikiLink.test(document.content) ||
          documentLink.test(document.content) ||
          document.content.toLowerCase().includes(`[[${targetTitle}]]`)
        );
      })
      .map((document) => ({
        document_id: document.id,
        title: document.title,
        space_id: spaceId,
        space_name: spaceName,
      }));
  },

  async setDocumentPropertyValues(
    spaceId: string,
    docId: string,
    values: ({
      property_id: string;
      value: string | null;
    } & ImportIdentityOptions)[],
  ): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/properties`, {
        method: "PUT",
        body: { values },
      });
      return;
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    for (const entry of values) {
      const existing = localPropertyValues.find(
        (value) => value.document_id === docId && value.property_id === entry.property_id,
      );
      if (existing) {
        existing.value = entry.value;
      } else {
        localPropertyValues = [
          ...localPropertyValues,
          {
            id: entry.source_id ?? makeId("propval"),
            document_id: docId,
            property_id: entry.property_id,
            value: entry.value,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
          },
        ];
      }
    }
    await persistLocalSnapshot({
      entity_type: "document_property_values",
      entity_id: docId,
      action: "set",
      payload: {
        document_id: docId,
        space_id: spaceId,
        values: values.map((entry) => {
          const stored = localPropertyValues.find(
            (value) => value.document_id === docId && value.property_id === entry.property_id,
          );
          return {
            id: stored?.id ?? entry.source_id,
            property_id: entry.property_id,
            value: entry.value,
            created_at: stored?.created_at ?? entry.created_at,
            updated_at: stored?.updated_at ?? entry.updated_at,
          };
        }),
      },
    });
  },

  async listTags(spaceId: string): Promise<TagRecord[]> {
    if (!inLocalMode()) return api.fetch<TagRecord[]>(`/spaces/${spaceId}/tags`);
    await ensureLocalData();
    requireLocalSpace(spaceId);
    return localTags
      .filter((tag) => tag.space_id === spaceId)
      .sort((a, b) => a.name.localeCompare(b.name))
      .map((tag) => {
        const count = localDocTags.filter((docTag) => docTag.tag_id === tag.id).length;
        return toTagRecord(tag, count);
      });
  },

  async createTag(
    spaceId: string,
    input: TagInput & Pick<ImportIdentityOptions, "source_id" | "created_at">,
  ): Promise<TagRecord> {
    const name = input.name.trim();
    if (!name) throw new Error("Tag name is required");
    if (!inLocalMode()) {
      return api.fetch<TagRecord>(`/spaces/${spaceId}/tags`, {
        method: "POST",
        body: {
          name,
          color: input.color,
          source_id: input.source_id,
          created_at: input.created_at,
        },
      });
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const tag: VaultTag = {
      id: input.source_id ?? makeId("tag"),
      space_id: spaceId,
      name,
      color: input.color ?? "#6b7280",
      created_at: input.created_at ?? new Date().toISOString(),
    };
    localTags = [...localTags, tag];
    await persistLocalSnapshot({ entity_type: "tag", entity_id: tag.id, action: "create" });
    return toTagRecord(tag, 0);
  },

  async updateTag(spaceId: string, tagId: string, input: TagInput): Promise<void> {
    const name = input.name.trim();
    if (!name) throw new Error("Tag name is required");
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/tags/${tagId}`, {
        method: "PUT",
        body: { name, color: input.color },
      });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    localTags = localTags.map((tag) =>
      tag.id === tagId && tag.space_id === spaceId
        ? { ...tag, name, color: input.color ?? tag.color }
        : tag,
    );
    await persistLocalSnapshot({ entity_type: "tag", entity_id: tagId, action: "update" });
  },

  async deleteTag(spaceId: string, tagId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/tags/${tagId}`, { method: "DELETE" });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const tag = localTags.find((item) => item.id === tagId && item.space_id === spaceId);
    localTags = localTags.filter((item) => !(item.id === tagId && item.space_id === spaceId));
    localDocTags = localDocTags.filter((docTag) => docTag.tag_id !== tagId);
    await persistLocalSnapshot({
      entity_type: "tag",
      entity_id: tagId,
      action: "delete",
      payload: tag ?? { id: tagId, space_id: spaceId },
    });
  },

  async listDocumentTags(spaceId: string, docId: string): Promise<TagRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<TagRecord[]>(`/spaces/${spaceId}/documents/${docId}/tags`);
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    const assignedIds = new Set(
      localDocTags.filter((docTag) => docTag.document_id === docId).map((docTag) => docTag.tag_id),
    );
    return localTags
      .filter((tag) => tag.space_id === spaceId && assignedIds.has(tag.id))
      .sort((a, b) => a.name.localeCompare(b.name))
      .map((tag) => toTagRecord(tag, null));
  },

  async addDocumentTag(spaceId: string, docId: string, tagId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/tags`, {
        method: "POST",
        body: { tag_id: tagId },
      });
      return;
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    if (!localTags.some((tag) => tag.id === tagId && tag.space_id === spaceId)) {
      throw new Error("Tag not found");
    }
    if (!localDocTags.some((docTag) => docTag.document_id === docId && docTag.tag_id === tagId)) {
      localDocTags = [...localDocTags, { document_id: docId, tag_id: tagId }];
      await persistLocalSnapshot({
        entity_type: "document_tag",
        entity_id: `${docId}:${tagId}`,
        action: "add",
      });
    }
  },

  async removeDocumentTag(spaceId: string, docId: string, tagId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/tags/${tagId}`, {
        method: "DELETE",
      });
      return;
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    localDocTags = localDocTags.filter(
      (docTag) => !(docTag.document_id === docId && docTag.tag_id === tagId),
    );
    await persistLocalSnapshot({
      entity_type: "document_tag",
      entity_id: `${docId}:${tagId}`,
      action: "remove",
      payload: { document_id: docId, tag_id: tagId, space_id: spaceId },
    });
  },

  async deleteDocument(spaceId: string, docId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}`, { method: "DELETE" });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const existing = requireLocalDoc(spaceId, docId);
    localDocs.delete(existing.id);
    await persistLocalSnapshot({ entity_type: "document", entity_id: docId, action: "delete" });
  },

  async listDocumentVersions(spaceId: string, docId: string): Promise<DocumentVersionRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<DocumentVersionRecord[]>(`/spaces/${spaceId}/documents/${docId}/versions`);
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    return [];
  },

  async restoreDocumentVersion(
    spaceId: string,
    docId: string,
    versionId: string,
  ): Promise<RestoreDocumentVersionResponse> {
    if (!inLocalMode()) {
      return api.fetch<RestoreDocumentVersionResponse>(
        `/spaces/${spaceId}/documents/${docId}/versions/${versionId}/restore`,
        { method: "POST" },
      );
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    throw new Error("Version restore is only available in server mode.");
  },

  async uploadDocumentAttachment(
    spaceId: string,
    docId: string,
    asset: VaultAsset,
  ): Promise<AttachmentRecord> {
    if (inLocalMode()) {
      throw new Error("Server attachment upload is only available while connected to a server");
    }
    const blob = await vaultAssetToBlob(asset);
    if (!blob) {
      throw new Error("Asset binary data is unavailable");
    }
    const formData = new FormData();
    formData.append("file", blob, asset.name);
    return api.fetch<AttachmentRecord>(`/spaces/${spaceId}/documents/${docId}/attachments`, {
      method: "POST",
      body: formData,
    });
  },

  async listDocumentAttachments(spaceId: string, docId: string): Promise<AttachmentRecord[]> {
    if (inLocalMode()) return [];
    return api.fetch<AttachmentRecord[]>(`/spaces/${spaceId}/documents/${docId}/attachments`);
  },

  async saveDocumentTextAsset(input: DocumentTextAssetInput): Promise<LocalAssetRecord | AttachmentRecord> {
    const mimeType = input.mime_type ?? "text/plain";
    const dataUrl = textToDataUrl(input.text, mimeType);
    const size = new TextEncoder().encode(input.text).length;

    if (!inLocalMode()) {
      if (!input.space_id || !input.document_id) {
        throw new Error("Server text asset save requires a document context");
      }
      const asset: VaultAsset = {
        id: makeId("asset"),
        name: input.name,
        mime_type: mimeType,
        size,
        url: dataUrl,
        source: "browser",
        space_id: input.space_id,
        document_id: input.document_id,
        created_at: new Date().toISOString(),
      };
      return this.uploadDocumentAttachment(input.space_id, input.document_id, asset);
    }

    await ensureLocalData();
    if (!input.document_id) {
      return this.saveBrowserAsset({
        name: input.name,
        mime_type: mimeType,
        size,
        url: dataUrl,
        space_id: input.space_id,
        document_id: input.document_id,
      });
    }

    snapshotAssetCache = await localVault.listAssets();
    const existing = newestByCreatedAt(
      snapshotAssetCache.filter(
        (asset) =>
          asset.name === input.name &&
          asset.document_id === input.document_id &&
          (!input.space_id || asset.space_id === input.space_id),
      ),
    );

    if (existing && !canUseTauriNativeAssetImport()) {
      const updated: VaultAsset = {
        ...existing,
        mime_type: mimeType,
        size,
        url: dataUrl,
      };
      await localVault.enqueueSyncOperation({
        entity_type: "asset",
        entity_id: updated.id,
        action: "update",
        payload: {
          ...updated,
          data_url: dataUrl,
        },
      });
      await localVault.saveAsset(updated);
      snapshotAssetCache = await localVault.listAssets();
      return updated;
    }

    return this.saveBrowserAsset({
      name: input.name,
      mime_type: mimeType,
      size,
      url: dataUrl,
      space_id: input.space_id,
      document_id: input.document_id,
    });
  },

  async readDocumentTextAsset(
    name: string,
    context: LocalAssetContext = {},
  ): Promise<string | null> {
    if (!context.document_id) return null;

    if (!inLocalMode()) {
      if (!context.space_id) return null;
      const attachments = await this.listDocumentAttachments(context.space_id, context.document_id);
      const attachment = newestByCreatedAt(
        attachments.filter((item) => item.original_name === name || item.filename === name),
      );
      if (!attachment) return null;
      try {
        const token = api.getToken();
        const response = await fetch(resolveServerFileUrl(attachment.url), {
          headers: token ? { Authorization: `Bearer ${token}` } : undefined,
        });
        if (!response.ok) return null;
        return response.text();
      } catch {
        return null;
      }
    }

    await ensureLocalData();
    snapshotAssetCache = await localVault.listAssets();
    const asset = newestByCreatedAt(
      snapshotAssetCache.filter(
        (item) =>
          item.name === name &&
          item.document_id === context.document_id &&
          (!context.space_id || item.space_id === context.space_id),
      ),
    );
    return asset ? vaultAssetToText(asset) : null;
  },

  async listUserDirectory(): Promise<UserDirectoryRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<UserDirectoryRecord[]>("/users/directory");
    }
    return [
      {
        id: "local-user",
        email: "local@starlane.notes",
        display_name: LOCAL_SYSTEM_USER,
        avatar_url: null,
      },
    ];
  },

  async saveBrowserAsset(input: {
    name: string;
    mime_type?: string;
    size: number;
    url: string;
    space_id?: string;
    document_id?: string;
  }): Promise<LocalAssetRecord> {
    const createdAt = new Date().toISOString();
    let asset: VaultAsset = {
      id: makeId("asset"),
      name: input.name,
      mime_type: input.mime_type,
      size: input.size,
      url: input.url,
      source: "browser",
      space_id: input.space_id,
      document_id: input.document_id,
      created_at: createdAt,
    };
    if (!inLocalMode()) return asset;

    await ensureLocalData();
    if (canUseTauriNativeAssetImport() && input.url.startsWith("data:")) {
      asset = await invokeTauri<VaultAsset>("import_vault_asset_from_data_url", {
        provider: getSyncProvider(),
        name: input.name,
        mimeType: input.mime_type,
        dataUrl: input.url,
        spaceId: input.space_id,
        documentId: input.document_id,
      });
      asset = { ...asset, space_id: input.space_id, document_id: input.document_id };
    }
    await localVault.enqueueSyncOperation({
      entity_type: "asset",
      entity_id: asset.id,
      action: "create",
      payload: {
        ...asset,
        space_id: input.space_id,
        document_id: input.document_id,
        data_url: input.url.startsWith("data:") ? input.url : undefined,
      },
    });
    await localVault.saveAsset(asset);
    snapshotAssetCache = await localVault.listAssets();
    return asset;
  },

  async importAssetFromPath(
    sourcePath: string,
    context: LocalAssetContext = {},
  ): Promise<LocalAssetRecord> {
    if (!inLocalMode()) {
      throw new Error("Native vault asset import is only available in local mode");
    }
    await ensureLocalData();
    if (!canUseTauriNativeAssetImport()) {
      throw new Error("Native vault asset import is unavailable");
    }

    const asset = await invokeTauri<VaultAsset>("import_vault_asset_from_path", {
      provider: getSyncProvider(),
      sourcePath,
      spaceId: context.space_id,
      documentId: context.document_id,
    });
    const ownedAsset = { ...asset, space_id: context.space_id, document_id: context.document_id };
    await localVault.enqueueSyncOperation({
      entity_type: "asset",
      entity_id: ownedAsset.id,
      action: "create",
      payload: {
        ...ownedAsset,
        space_id: context.space_id,
        document_id: context.document_id,
      },
    });
    await localVault.saveAsset(ownedAsset);
    snapshotAssetCache = await localVault.listAssets();
    return ownedAsset;
  },

  async getLocalVaultStats(): Promise<LocalVaultStatsRecord> {
    if (!inLocalMode()) {
      return { spaces: 0, folders: 0, documents: 0, assets: 0, asset_bytes: 0 };
    }
    await ensureLocalData();
    const assets = await localVault.listAssets();
    snapshotAssetCache = assets;
    return {
      spaces: localSpaces.size,
      folders: localFolders.size,
      documents: localDocs.size,
      assets: assets.length,
      asset_bytes: assets.reduce((sum, asset) => sum + asset.size, 0),
    };
  },

  async exportLocalVaultSnapshot(): Promise<LocalVaultSnapshot> {
    if (!inLocalMode()) {
      throw new Error("Vault export is only available in local mode");
    }
    await ensureLocalData();
    return localVault.loadSnapshot();
  },

  async exportQueuedSyncReplayManifest(
    operationIds?: string[],
  ): Promise<LocalSyncReplayManifestRecord> {
    if (!inLocalMode()) {
      throw new Error("Queued sync replay export is only available in local mode");
    }
    await ensureLocalData();
    const snapshot = await localVault.loadSnapshot();
    const requestedIds = operationIds && operationIds.length > 0 ? new Set(operationIds) : null;
    const operations = (snapshot.sync_state?.queued_operations ?? []).filter(
      (operation) => !requestedIds || requestedIds.has(operation.id),
    );
    const recoveryCheckpoints = readSharedSpaceDownloadCheckpoints();
    const replayContract = buildNativeLocalDatabaseReplayContract();
    const replayOperations = await Promise.all(
      sortReplayOperationsForContract(operations).map(async (operation) => {
        const payload = await hydrateReplayOperationPayload(
          operation,
          findQueuedOperationPayload(snapshot, operation),
        );
        const payloadAvailable = isQueuedReplayPayloadAvailable(operation, payload);
        return {
          ...operation,
          payload_status: payloadAvailable ? "available" : "missing",
          payload,
        } satisfies LocalSyncReplayOperationRecord;
      }),
    );
    return {
      schema_version: LOCAL_VAULT_SCHEMA_VERSION,
      exported_at: new Date().toISOString(),
      provider: getSyncProvider(),
      replay_contract: replayContract,
      replay_readiness: preflightReplayOperationsForContract(replayOperations, replayContract),
      operation_count: operations.length,
      operation_summary: summarizeQueuedSyncReplayOperations(replayOperations),
      recovery_checkpoint_count: recoveryCheckpoints.length,
      recovery_checkpoints: recoveryCheckpoints,
      operations: replayOperations,
    };
  },

  async preflightQueuedSyncReplayManifest(
    operationIds?: string[],
  ): Promise<LocalSyncReplayServerPreflightRecord> {
    if (!inLocalMode()) {
      throw new Error("Queued sync replay preflight is only available in local mode");
    }
    const manifest = await this.exportQueuedSyncReplayManifest(operationIds);
    return api.fetch<LocalSyncReplayServerPreflightRecord>("/sync/replay/preflight", {
      method: "POST",
      body: manifest,
    });
  },

  async runQueuedSyncReplay(operationIds?: string[]): Promise<LocalSyncReplayServerResultRecord> {
    if (!inLocalMode()) {
      throw new Error("Queued sync replay is only available in local mode");
    }
    const manifest = await this.exportQueuedSyncReplayManifest(operationIds);
    const result = await api.fetch<LocalSyncReplayServerResultRecord>("/sync/replay", {
      method: "POST",
      body: manifest,
    });
    const queuedById = new Map(
      (await localVault.listQueuedSyncOperations()).map((operation) => [operation.id, operation]),
    );
    const recoveryUpdates = result.results
      .filter((operation) => operation.status === "failed" || operation.status === "unsupported")
      .flatMap((operation) => {
        const queued = queuedById.get(operation.operation_id);
        return queued
          ? [{
              id: queued.id,
              attempts: (queued.attempts ?? 0) + 1,
              last_error: operation.message,
            }]
          : [];
      });
    if (recoveryUpdates.length > 0) {
      await localVault.updateQueuedSyncOperations(recoveryUpdates);
      await localVault.setSyncState("error");
    }
    return result;
  },

  async listQueuedSyncReplayHistory(): Promise<LocalSyncReplayHistoryRecord> {
    if (!inLocalMode()) {
      throw new Error("Queued sync replay history is only available in local mode");
    }
    return api.fetch<LocalSyncReplayHistoryRecord>("/sync/replay/history");
  },

  async applyProviderRemoteImportManifest(
    manifest: ProviderRemoteSyncManifest,
    options: { allowDestructiveSpaceRecovery?: boolean } = {},
  ): Promise<ProviderRemoteImportApplyExecutionResult> {
    if (!inLocalMode()) {
      throw new Error("Provider remote import apply is only available in local mode");
    }
    await ensureLocalData();
    await localVault.setSyncState("syncing");
    try {
      const localQueuedOperations = await localVault.listQueuedSyncOperations();
      const dryRun = buildProviderRemoteImportDryRun(
        manifest,
        localQueuedOperations.map((operation) => operation.id),
        localQueuedOperations.map((operation) => ({
          id: operation.id,
          entity_type: operation.entity_type,
          entity_id: operation.entity_id,
          action: operation.action,
          payload: operation.payload,
        })),
        options,
      );
      const plan = buildProviderRemoteImportApplyPlan(dryRun);
      const result = await executeProviderRemoteImportApplyPlan(plan, {
        allowLocalMutation: true,
        mergeExecutor: {
          applyOperation: applyProviderRemoteImportOperationToLocalVault,
        },
      });
      if (result.status === "completed") {
        await localVault.setSyncState("idle");
      } else if (result.results.some((operation) => operation.status === "blocked")) {
        await localVault.setSyncState("conflict");
      } else {
        await localVault.setSyncState("error");
      }
      return result;
    } catch (error) {
      await localVault.setSyncState("error");
      throw error;
    }
  },

  previewVaultMigration(snapshot: unknown): VaultMigrationSummaryRecord {
    assertLocalVaultSnapshot(snapshot);
    return summarizeVaultSnapshot(normalizeLocalVaultSnapshot(snapshot));
  },

  async importLocalVaultSnapshot(snapshot: unknown): Promise<void> {
    if (!inLocalMode()) {
      throw new Error("Vault import is only available in local mode");
    }
    assertLocalVaultSnapshot(snapshot);
    const nextSnapshot = normalizeLocalVaultSnapshot(snapshot);
    await localVault.saveSnapshot(nextSnapshot);
    hydrateFromSnapshot(nextSnapshot);
    localInitialized = true;
  },

  async importVaultSnapshotToServer(snapshot: unknown): Promise<VaultMigrationSummaryRecord> {
    if (inLocalMode()) {
      throw new Error("Server migration is only available while connected to a server");
    }
    assertLocalVaultSnapshot(snapshot);
    const normalized = normalizeLocalVaultSnapshot(snapshot);
    const summary = summarizeVaultSnapshot(normalized);
    const spaceMap = new Map<string, string>();
    const folderMap = new Map<string, string>();
    const documentMap = new Map<string, string>();
    const workflowTemplateMap = new Map<string, string>();
    const propertyMap = new Map<string, string>();
    const tagMap = new Map<string, string>();
    const migratedDocumentContent = new Map<
      string,
      { space_id: string; document_id: string; original_content: string; content: string }
    >();

    for (const sourceSpace of normalized.spaces) {
      const createdSpace = await this.createSpace(sourceSpace.name, {
        source_id: sourceSpace.id,
        created_at: sourceSpace.created_at,
        updated_at: sourceSpace.updated_at,
      });
      spaceMap.set(sourceSpace.id, createdSpace.id);

      const sourceFolders = normalized.folders.filter(
        (folder) => folder.space_id === sourceSpace.id,
      );
      const pendingFolders = [...sourceFolders];
      let madeProgress = true;
      while (pendingFolders.length > 0 && madeProgress) {
        madeProgress = false;
        for (let index = pendingFolders.length - 1; index >= 0; index -= 1) {
          const folder = pendingFolders[index];
          const parentId = folder.parent_id ? folderMap.get(folder.parent_id) : undefined;
          if (folder.parent_id && !parentId) continue;

          const createdFolder = await this.createFolder(createdSpace.id, folder.name, {
            parent_id: parentId ?? null,
            source_id: folder.id,
          });
          folderMap.set(folder.id, createdFolder.id);
          pendingFolders.splice(index, 1);
          madeProgress = true;
        }
      }
      for (const folder of pendingFolders) {
        const createdFolder = await this.createFolder(createdSpace.id, folder.name, {
          source_id: folder.id,
        });
        folderMap.set(folder.id, createdFolder.id);
      }

      for (const doc of normalized.documents.filter((item) => item.space_id === sourceSpace.id)) {
        const createdDoc = await this.createDocument(createdSpace.id, {
          title: doc.title,
          content: doc.content,
          folder_id: doc.folder_id ? (folderMap.get(doc.folder_id) ?? null) : null,
          source_id: doc.id,
          created_at: doc.created_at,
          updated_at: doc.updated_at,
        });
        documentMap.set(doc.id, createdDoc.id);
        migratedDocumentContent.set(doc.id, {
          space_id: createdSpace.id,
          document_id: createdDoc.id,
          original_content: doc.content,
          content: doc.content,
        });
      }

      for (const template of normalized.templates.filter(
        (item) => item.space_id === sourceSpace.id,
      )) {
        await this.createTemplate(createdSpace.id, {
          name: template.name,
          description: template.description,
          content: template.content,
          properties: template.properties,
          tags: template.tags,
          source_id: template.id,
          created_at: template.created_at,
          updated_at: template.updated_at,
        });
      }

      const sourceProperties = (normalized.property_schemas ?? [])
        .filter((item) => item.space_id === sourceSpace.id)
        .sort((a, b) => a.sort_order - b.sort_order);
      for (const property of sourceProperties) {
        const createdProperty = await this.createPropertySchema(createdSpace.id, {
          name: property.name,
          property_type: property.property_type,
          options: property.options,
          source_id: property.id,
        });
        propertyMap.set(property.id, createdProperty.id);
      }
      const propertyOrder = sourceProperties
        .map((property) => propertyMap.get(property.id))
        .filter((id): id is string => Boolean(id));
      if (propertyOrder.length > 0) {
        await this.reorderPropertySchemas(createdSpace.id, { property_ids: propertyOrder });
      }

      for (const tag of (normalized.tags ?? []).filter(
        (item) => item.space_id === sourceSpace.id,
      )) {
        const createdTag = await this.createTag(createdSpace.id, {
          name: tag.name,
          color: tag.color,
          source_id: tag.id,
          created_at: tag.created_at,
        });
        tagMap.set(tag.id, createdTag.id);
      }

      for (const workflowTemplate of (normalized.workflow_templates ?? []).filter(
        (item) => item.space_id === sourceSpace.id,
      )) {
        const createdWorkflowTemplate = await this.createWorkflowTemplate(createdSpace.id, {
          name: workflowTemplate.name,
          description: workflowTemplate.description,
          stages: workflowTemplate.stages,
          approvers: workflowTemplate.approvers,
          source_id: workflowTemplate.id,
          created_at: workflowTemplate.created_at,
          updated_at: workflowTemplate.updated_at,
        });
        workflowTemplateMap.set(workflowTemplate.id, createdWorkflowTemplate.id);
      }
    }

    const skippedAssets: string[] = [];
    for (const asset of normalized.assets) {
      const sourceDocumentId = extractDocumentIdFromAssetPath(asset, normalized.documents);
      const migratedDocument = sourceDocumentId
        ? migratedDocumentContent.get(sourceDocumentId)
        : undefined;
      if (!sourceDocumentId || !migratedDocument) {
        skippedAssets.push(asset.name);
        continue;
      }

      try {
        const uploaded = await this.uploadDocumentAttachment(
          migratedDocument.space_id,
          migratedDocument.document_id,
          asset,
        );
        if (migratedDocument.content.includes(asset.url)) {
          migratedDocument.content = migratedDocument.content.split(asset.url).join(uploaded.url);
          migratedDocumentContent.set(sourceDocumentId, migratedDocument);
        }
      } catch {
        skippedAssets.push(asset.name);
      }
    }

    for (const migratedDocument of migratedDocumentContent.values()) {
      if (migratedDocument.original_content === migratedDocument.content) continue;
      await this.updateDocumentContent(
        migratedDocument.space_id,
        migratedDocument.document_id,
        migratedDocument.content,
      );
    }

    if (skippedAssets.length > 0) {
      summary.warnings = [
        ...summary.warnings,
        `${skippedAssets.length} asset binary import(s) were skipped because their binary data or document reference was unavailable.`,
      ];
    }

    for (const propertyValue of normalized.property_values ?? []) {
      const mappedDocumentId = documentMap.get(propertyValue.document_id);
      const mappedPropertyId = propertyMap.get(propertyValue.property_id);
      if (!mappedDocumentId || !mappedPropertyId) continue;
      const sourceDoc = normalized.documents.find((doc) => doc.id === propertyValue.document_id);
      const mappedSpaceId = sourceDoc?.space_id ? spaceMap.get(sourceDoc.space_id) : undefined;
      if (!mappedSpaceId) continue;
      await this.setDocumentPropertyValues(mappedSpaceId, mappedDocumentId, [
        {
          property_id: mappedPropertyId,
          value: propertyValue.value,
          source_id: propertyValue.id,
          created_at: propertyValue.created_at,
          updated_at: propertyValue.updated_at,
        },
      ]);
    }

    for (const docTag of normalized.doc_tags ?? []) {
      const mappedDocumentId = documentMap.get(docTag.document_id);
      const mappedTagId = tagMap.get(docTag.tag_id);
      if (!mappedDocumentId || !mappedTagId) continue;
      const sourceDoc = normalized.documents.find((doc) => doc.id === docTag.document_id);
      const mappedSpaceId = sourceDoc?.space_id ? spaceMap.get(sourceDoc.space_id) : undefined;
      if (!mappedSpaceId) continue;
      await this.addDocumentTag(mappedSpaceId, mappedDocumentId, mappedTagId);
    }

    for (const workflow of normalized.document_workflows ?? []) {
      const mappedDocumentId = documentMap.get(workflow.document_id);
      const mappedWorkflowTemplateId = workflowTemplateMap.get(workflow.workflow_template_id);
      if (!mappedDocumentId || !mappedWorkflowTemplateId) continue;
      const sourceDoc = normalized.documents.find((doc) => doc.id === workflow.document_id);
      const mappedSpaceId = sourceDoc?.space_id ? spaceMap.get(sourceDoc.space_id) : undefined;
      if (!mappedSpaceId) continue;
      const workflowEvents = (normalized.document_workflow_events ?? []).filter(
        (item) =>
          item.document_workflow_id === workflow.id && item.document_id === workflow.document_id,
      );
      const enableEvent = workflowEvents.find((event) => event.action === "enable");
      await this.enableDocumentWorkflow(mappedSpaceId, mappedDocumentId, {
        template_id: mappedWorkflowTemplateId,
        stage_id: workflow.current_stage_id,
        source_id: workflow.id,
        event_source_id: enableEvent?.id,
        event_created_by: enableEvent?.created_by,
        event_created_at: enableEvent?.created_at,
        created_at: workflow.created_at,
        updated_at: workflow.updated_at,
      });
      for (const event of workflowEvents) {
        if (event.action === "enable") continue;
        await this.recordDocumentWorkflowEvent(mappedSpaceId, mappedDocumentId, {
          action: event.action,
          note: event.note ?? "",
          source_id: event.id,
          created_by: event.created_by,
          created_at: event.created_at,
          from_stage_id: event.from_stage_id,
          to_stage_id: event.to_stage_id,
        });
      }
    }

    return summary;
  },

  async exportServerVaultSnapshot(): Promise<LocalVaultSnapshot> {
    if (inLocalMode()) {
      throw new Error("Server export is only available while connected to a server");
    }
    const now = new Date().toISOString();
    const spaces = await this.listSpaces();
    const snapshot: LocalVaultSnapshot = {
      schema_version: LOCAL_VAULT_SCHEMA_VERSION,
      spaces: spaces.map((space) => ({
        id: space.id,
        name: space.name,
        slug: space.slug,
        created_at: now,
        updated_at: now,
      })),
      folders: [],
      documents: [],
      assets: [],
      templates: [],
      workflow_templates: [],
      document_workflows: [],
      document_workflow_events: [],
      property_schemas: [],
      property_values: [],
      tags: [],
      doc_tags: [],
      sync_state: {
        state: "idle",
        updated_at: now,
        queued_operations: [],
      },
    };

    for (const space of spaces) {
      const [folders, documents, templates, workflowTemplates, propertySchemas, tags] =
        await Promise.all([
          this.listFolders(space.id),
          this.listDocuments(space.id, { include_nested: true }),
          this.listTemplates(space.id),
          this.listWorkflowTemplates(space.id),
          this.listPropertySchemas(space.id),
          this.listTags(space.id),
        ]);

      snapshot.folders.push(
        ...folders.map((folder) => ({
          id: folder.id,
          space_id: space.id,
          name: folder.name,
          parent_id: folder.parent_id,
        })),
      );
      snapshot.templates.push(...templates);
      snapshot.workflow_templates?.push(...workflowTemplates);
      snapshot.property_schemas?.push(
        ...propertySchemas.map((schema) => ({ ...schema, space_id: space.id })),
      );
      snapshot.tags?.push(
        ...tags.map((tag) => ({
          id: tag.id,
          space_id: space.id,
          name: tag.name,
          color: tag.color,
          created_at: tag.created_at,
        })),
      );

      for (const doc of documents) {
        const [content, workflow, workflowEvents, properties, docTags] = await Promise.all([
          this.getDocumentContent(space.id, doc.id),
          this.getDocumentWorkflow(space.id, doc.id).catch(() => null),
          this.listDocumentWorkflowEvents(space.id, doc.id).catch(() => []),
          this.listDocumentProperties(space.id, doc.id).catch(() => []),
          this.listDocumentTags(space.id, doc.id).catch(() => []),
        ]);
        snapshot.documents.push({
          id: doc.id,
          space_id: space.id,
          title: content.title,
          content: content.content,
          icon: doc.icon,
          cover_image: doc.cover_image,
          locked: doc.locked,
          folder_id: doc.folder_id,
          created_by: doc.created_by,
          last_updated_by: doc.last_updated_by,
          created_at: doc.created_at ?? doc.updated_at ?? now,
          updated_at: doc.updated_at ?? now,
        });
        if (workflow) {
          snapshot.document_workflows?.push({
            id: workflow.id,
            document_id: workflow.document_id,
            workflow_template_id: workflow.workflow_template_id,
            current_stage_id: workflow.current_stage_id,
            status: workflow.status,
            created_by: workflow.created_by,
            created_at: workflow.created_at,
            updated_at: workflow.updated_at,
          });
        }
        snapshot.document_workflow_events?.push(...workflowEvents);
        snapshot.property_values?.push(
          ...properties
            .filter((property) => property.value !== null)
            .map((property) => ({
              id: makeId("propval"),
              document_id: doc.id,
              property_id: property.property_id,
              value: property.value,
            })),
        );
        snapshot.doc_tags?.push(...docTags.map((tag) => ({ document_id: doc.id, tag_id: tag.id })));
      }
    }

    return snapshot;
  },

  async exportServerSpaceSnapshot(spaceId: string): Promise<LocalVaultSnapshot> {
    if (inLocalMode()) {
      throw new Error("Server space download is only available while connected to a server");
    }
    const fullSnapshot = await this.exportServerVaultSnapshot();
    const space = fullSnapshot.spaces.find((item) => item.id === spaceId);
    if (!space) throw new Error("Space not found");
    const documentIds = new Set(
      fullSnapshot.documents.filter((doc) => doc.space_id === spaceId).map((doc) => doc.id),
    );
    const workflowIds = new Set(
      (fullSnapshot.document_workflows ?? [])
        .filter((workflow) => documentIds.has(workflow.document_id))
        .map((workflow) => workflow.id),
    );
    const tagIds = new Set(
      (fullSnapshot.tags ?? []).filter((tag) => tag.space_id === spaceId).map((tag) => tag.id),
    );
    const propertySchemaIds = new Set(
      (fullSnapshot.property_schemas ?? [])
        .filter((schema) => schema.space_id === spaceId)
        .map((schema) => schema.id),
    );
    return {
      ...fullSnapshot,
      spaces: [space],
      folders: fullSnapshot.folders.filter((folder) => folder.space_id === spaceId),
      documents: fullSnapshot.documents.filter((doc) => doc.space_id === spaceId),
      assets: fullSnapshot.assets,
      templates: fullSnapshot.templates.filter((template) => template.space_id === spaceId),
      workflow_templates: (fullSnapshot.workflow_templates ?? []).filter(
        (template) => template.space_id === spaceId,
      ),
      document_workflows: (fullSnapshot.document_workflows ?? []).filter((workflow) =>
        documentIds.has(workflow.document_id),
      ),
      document_workflow_events: (fullSnapshot.document_workflow_events ?? []).filter((event) =>
        workflowIds.has(event.document_workflow_id),
      ),
      property_schemas: (fullSnapshot.property_schemas ?? []).filter((schema) =>
        propertySchemaIds.has(schema.id),
      ),
      property_values: (fullSnapshot.property_values ?? []).filter(
        (value) => documentIds.has(value.document_id) && propertySchemaIds.has(value.property_id),
      ),
      tags: (fullSnapshot.tags ?? []).filter((tag) => tagIds.has(tag.id)),
      doc_tags: (fullSnapshot.doc_tags ?? []).filter(
        (tag) => documentIds.has(tag.document_id) && tagIds.has(tag.tag_id),
      ),
      search_index: (fullSnapshot.search_index ?? []).filter((entry) =>
        documentIds.has(entry.document_id),
      ),
    };
  },

  async exportLocalVaultArchive(archivePath: string): Promise<void> {
    if (!inLocalMode()) {
      throw new Error("Vault export is only available in local mode");
    }
    if (!canUseTauriNativeVaultArchive()) {
      throw new Error("Vault archive export is only available in the desktop app");
    }
    await ensureLocalData();
    await invokeTauri<void>("export_vault_archive", {
      provider: getSyncProvider(),
      archivePath,
    });
  },

  async importLocalVaultArchive(archivePath: string): Promise<void> {
    if (!inLocalMode()) {
      throw new Error("Vault import is only available in local mode");
    }
    if (!canUseTauriNativeVaultArchive()) {
      throw new Error("Vault archive import is only available in the desktop app");
    }
    const snapshot = await invokeTauri<unknown>("import_vault_archive", {
      provider: getSyncProvider(),
      archivePath,
    });
    await this.importLocalVaultSnapshot(snapshot);
  },

  async reconcileLocalVaultMirror(): Promise<VaultMirrorReconciliationRecord> {
    if (!inLocalMode()) {
      throw new Error("Vault mirror reconciliation is only available in local mode");
    }
    if (!canUseTauriNativeVaultArchive()) {
      throw new Error("Vault mirror reconciliation is only available in the desktop app");
    }
    const result = await invokeTauri<VaultMirrorReconciliationRecord & { snapshot: unknown }>(
      "reconcile_vault_mirror",
      {
        provider: getSyncProvider(),
      },
    );
    await this.importLocalVaultSnapshot(result.snapshot);
    return {
      files_scanned: result.files_scanned,
      documents_updated: result.documents_updated,
      templates_updated: result.templates_updated,
      documents_imported: result.documents_imported,
      templates_imported: result.templates_imported,
    };
  },

  async getDocument(spaceId: string, docId: string): Promise<DocumentRecord> {
    if (!inLocalMode()) {
      return api.fetch<DocumentRecord>(`/spaces/${spaceId}/documents/${docId}`);
    }
    await ensureLocalData();
    const doc = requireLocalDoc(spaceId, docId);
    return {
      id: doc.id,
      title: doc.title,
      icon: doc.icon,
      folder_id: doc.folder_id,
      parent_id: doc.parent_id,
      cover_image: doc.cover_image,
      locked: false,
      public: false,
      created_by: doc.created_by,
      last_updated_by: doc.last_updated_by,
      created_at: doc.created_at,
      updated_at: doc.updated_at,
      space_id: doc.space_id,
    };
  },

  async getDocumentContent(spaceId: string, docId: string): Promise<DocumentContentRecord> {
    if (!inLocalMode()) {
      return api.fetch<DocumentContentRecord>(`/spaces/${spaceId}/documents/${docId}/content`);
    }
    await ensureLocalData();
    const doc = requireLocalDoc(spaceId, docId);
    return { id: doc.id, title: doc.title, content: doc.content };
  },

  async updateDocument(
    spaceId: string,
    docId: string,
    patch: { title?: string; cover_image?: string | null; folder_id?: string | null; parent_id?: string | null },
  ): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}`, {
        method: "PUT",
        body: patch,
      });
      return;
    }
    await ensureLocalData();
    const doc = requireLocalDoc(spaceId, docId);
    const nextTitle = patch.title ? normalizeTitle(patch.title) : doc.title;
    const nextFolderId = Object.prototype.hasOwnProperty.call(patch, "folder_id")
      ? normalizeLocalDocumentFolderId(spaceId, patch.folder_id)
      : doc.folder_id;
    const nextParentId = Object.prototype.hasOwnProperty.call(patch, "parent_id")
      ? normalizeLocalDocumentParentId(spaceId, patch.parent_id, nextFolderId, docId)
      : doc.parent_id;
    localDocs.set(docId, {
      ...doc,
      title: nextTitle,
      folder_id: nextFolderId,
      parent_id: nextParentId,
      cover_image: "cover_image" in patch ? (patch.cover_image ?? undefined) : doc.cover_image,
      last_updated_by: LOCAL_SYSTEM_USER,
      updated_at: new Date().toISOString(),
    });
    await persistLocalSnapshot({ entity_type: "document", entity_id: docId, action: "update" });
  },

  async moveDocument(spaceId: string, docId: string, input: MoveDocumentInput): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/move`, {
        method: "POST",
        body: input,
      });
      return;
    }

    await ensureLocalData();
    requireLocalSpace(spaceId);
    requireLocalSpace(input.target_space_id);
    const doc = requireLocalDoc(spaceId, docId);
    const nextFolderId = normalizeLocalDocumentFolderId(input.target_space_id, input.folder_id);
    const nextParentId = normalizeLocalDocumentParentId(
      input.target_space_id,
      input.parent_id,
      nextFolderId,
      input.target_space_id === spaceId ? docId : undefined,
    );
    const docIds = collectLocalDocumentSubtreeIds(spaceId, docId);
    const movedAt = new Date().toISOString();

    for (const id of docIds) {
      const current = localDocs.get(id);
      if (!current) continue;
      localDocs.set(id, {
        ...current,
        space_id: input.target_space_id,
        folder_id: nextFolderId,
        parent_id: id === doc.id ? nextParentId : current.parent_id,
        last_updated_by: LOCAL_SYSTEM_USER,
        updated_at: movedAt,
      });
    }

    await persistLocalSnapshot({ entity_type: "document", entity_id: docId, action: "update" });
  },

  async updateDocumentContent(spaceId: string, docId: string, content: string): Promise<void> {
    if (!inLocalMode()) {
      assertContentSize(content);
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/content`, {
        method: "PUT",
        body: { content },
      });
      return;
    }
    await ensureLocalData();
    const doc = requireLocalDoc(spaceId, docId);
    assertContentSize(content);
    const updated = {
      ...doc,
      content,
      last_updated_by: LOCAL_SYSTEM_USER,
      updated_at: new Date().toISOString(),
    };
    localDocs.set(docId, updated);
    await persistLocalSnapshot({
      entity_type: "document",
      entity_id: docId,
      action: "update_content",
    });
  },

  async getGraph(spaceId: string): Promise<GraphRecord> {
    if (!inLocalMode()) return api.fetch<GraphRecord>(`/spaces/${spaceId}/graph`);
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const nodes = Array.from(localDocs.values())
      .filter((doc) => doc.space_id === spaceId)
      .map<GraphNodeRecord>((doc) => ({
        id: doc.id,
        title: doc.title,
        folder_id: doc.folder_id,
        connection_count: 0,
        orphan: true,
      }));
    return { nodes, edges: [] };
  },

  async listConflicts(): Promise<DocumentConflictRecord[]> {
    if (!inLocalMode()) return [];
    await ensureLocalData();
    return listStoredConflicts();
  },

  async searchDocuments(
    query: string,
    limit = 25,
    filters: SearchFilters = {},
  ): Promise<SearchResultRecord[]> {
    const normalized = query.trim();
    const normalizedFilters = normalizeSearchFilters(filters);
    const hasFilters = Object.keys(normalizedFilters).length > 0;
    if (!normalized && !hasFilters) return [];

    if (!inLocalMode()) {
      const params = new URLSearchParams({ q: normalized, limit: String(limit) });
      Object.entries(normalizedFilters).forEach(([key, value]) => params.set(key, value));
      return api.fetch<SearchResultRecord[]>(`/search?${params.toString()}`);
    }

    await ensureLocalData();
    const needle = normalized.toLowerCase();
    const spaceById = new Map(Array.from(localSpaces.values()).map((s) => [s.id, s.name]));
    const searchIndex = await ensureLocalSearchIndex();

    const results = searchIndex
      .filter((entry) => {
        const doc = localDocs.get(entry.document_id);
        if (!doc) return false;
        if (normalizedFilters.space_id && entry.space_id !== normalizedFilters.space_id) {
          return false;
        }
        if (needle) {
          if (!entry.search_text.includes(needle)) return false;
        }
        return localDocMatchesSearchFilters(entry.document_id, normalizedFilters);
      })
      .sort((a, b) => (a.updated_at < b.updated_at ? 1 : -1))
      .slice(0, limit)
      .map<SearchResultRecord>((entry) => ({
        document_id: entry.document_id,
        title: entry.title,
        space_id: entry.space_id,
        space_name: spaceById.get(entry.space_id) ?? PERSONAL_SPACE_NAME,
        excerpt: entry.content.slice(0, 180).trim(),
        updated_at: entry.updated_at,
        author: "Local User",
      }));

    return results;
  },

  async queryDataview(query: string, limit = 50): Promise<SearchResultRecord[]> {
    const normalized = query.trim();
    if (!normalized) return [];

    if (!inLocalMode()) {
      const params = new URLSearchParams({ q: normalized, limit: String(limit) });
      return api.fetch<SearchResultRecord[]>(`/dataview?${params.toString()}`);
    }

    await ensureLocalData();
    const documents = Array.from(localDocs.values())
      .sort((a, b) => (a.updated_at < b.updated_at ? 1 : -1))
      .map<DataviewDocument>((doc) => ({
        id: doc.id,
        title: doc.title,
        content: doc.content,
        space_id: doc.space_id,
        space_name: localSpaces.get(doc.space_id)?.name ?? PERSONAL_SPACE_NAME,
        tags: localDocTags
          .filter((docTag) => docTag.document_id === doc.id)
          .map((docTag) => localTags.find((tag) => tag.id === docTag.tag_id)?.name)
          .filter((tag): tag is string => Boolean(tag)),
        properties: Object.fromEntries(
          localPropertyValues
            .filter((value) => value.document_id === doc.id)
            .map((value) => {
              const schema = localPropertySchemas.find((item) => item.id === value.property_id);
              return [schema?.name ?? value.property_id, value.value ?? ""];
            }),
        ),
        created_at: doc.created_at,
        updated_at: doc.updated_at,
      }));

    return filterDataviewDocuments(documents, normalized, limit).map<SearchResultRecord>((doc) => ({
      document_id: doc.id,
      title: doc.title,
      space_id: doc.space_id ?? "local-space",
      space_name: doc.space_name ?? PERSONAL_SPACE_NAME,
      excerpt: (doc.content ?? "").replace(/\s+/g, " ").trim().slice(0, 180),
      updated_at: doc.updated_at ?? new Date().toISOString(),
      author: LOCAL_SYSTEM_USER,
    }));
  },

  async listNotifications(options: NotificationListOptions = {}): Promise<NotificationRecord[]> {
    if (!inLocalMode()) {
      const params = new URLSearchParams();
      if (options.limit !== undefined) params.set("limit", String(options.limit));
      if (options.before) params.set("before", options.before);
      if (options.unreadOnly) params.set("unread_only", "true");
      const query = params.toString();
      return api.fetch<NotificationRecord[]>(`/notifications${query ? `?${query}` : ""}`);
    }
    return [];
  },

  async markAllNotificationsRead(): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch("/notifications/read-all", { method: "POST" });
    }
  },

  async markNotificationRead(notificationId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/notifications/${notificationId}/read`, { method: "POST" });
    }
  },

  async createSystemNotification(input: {
    title: string;
    body?: string;
  }): Promise<NotificationRecord | null> {
    if (!inLocalMode()) {
      return api.fetch<NotificationRecord | null>("/notifications/system", {
        method: "POST",
        body: input,
      });
    }
    return null;
  },

  async listNotificationPreferences(): Promise<NotificationPreferenceRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<NotificationPreferenceRecord[]>("/notifications/preferences");
    }
    return [];
  },

  async updateNotificationPreferences(
    preferences: Pick<NotificationPreferenceRecord, "notification_type" | "channel">[],
  ): Promise<NotificationPreferenceRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<NotificationPreferenceRecord[]>("/notifications/preferences", {
        method: "PUT",
        body: { preferences },
      });
    }
    return [];
  },

  async listSessions(): Promise<UserSessionRecord[]> {
    if (!inLocalMode()) return api.fetch<UserSessionRecord[]>("/auth/sessions");
    return [];
  },

  async revokeSession(sessionId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/auth/sessions/${encodeURIComponent(sessionId)}`, { method: "DELETE" });
    }
  },

  async renameSession(sessionId: string, deviceName: string): Promise<UserSessionRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<UserSessionRecord[]>(`/auth/sessions/${encodeURIComponent(sessionId)}`, {
        method: "PUT",
        body: { device_name: deviceName },
      });
    }
    return [];
  },

  async setupTotp(): Promise<TotpSetupResponse> {
    if (!inLocalMode()) return api.fetch<TotpSetupResponse>("/auth/2fa/totp/setup", { method: "POST" });
    throw new Error("Two-factor authentication is unavailable in local mode.");
  },

  async verifyTotp(code: string): Promise<TotpStatusResponse> {
    if (!inLocalMode()) {
      return api.fetch<TotpStatusResponse>("/auth/2fa/totp/verify", {
        method: "POST",
        body: { code },
      });
    }
    throw new Error("Two-factor authentication is unavailable in local mode.");
  },

  async disableTotp(code: string): Promise<TotpStatusResponse> {
    if (!inLocalMode()) {
      return api.fetch<TotpStatusResponse>("/auth/2fa/totp/disable", {
        method: "POST",
        body: { code },
      });
    }
    throw new Error("Two-factor authentication is unavailable in local mode.");
  },

  async listPasskeys(): Promise<PasskeyRecord[]> {
    if (!inLocalMode()) return api.fetch<PasskeyRecord[]>("/auth/passkeys");
    return [];
  },

  async startPasskeyRegistration(name?: string): Promise<PasskeyChallengeResponse> {
    if (!inLocalMode()) {
      return api.fetch<PasskeyChallengeResponse>("/auth/passkeys/register/start", {
        method: "POST",
        body: { name },
      });
    }
    throw new Error("Passkeys are unavailable in local mode.");
  },

  async finishPasskeyRegistration(
    challengeId: string,
    credential: unknown,
    name?: string,
  ): Promise<PasskeyRecord> {
    if (!inLocalMode()) {
      return api.fetch<PasskeyRecord>("/auth/passkeys/register/finish", {
        method: "POST",
        body: { challenge_id: challengeId, credential, name },
      });
    }
    throw new Error("Passkeys are unavailable in local mode.");
  },

  async deletePasskey(passkeyId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/auth/passkeys/${encodeURIComponent(passkeyId)}`, { method: "DELETE" });
      return;
    }
    throw new Error("Passkeys are unavailable in local mode.");
  },

  async startPasskeyLogin(email: string): Promise<PasskeyChallengeResponse> {
    if (!inLocalMode()) {
      return api.fetch<PasskeyChallengeResponse>("/auth/passkeys/login/start", {
        method: "POST",
        body: { email },
      });
    }
    throw new Error("Passkey sign-in is unavailable in local mode.");
  },

  async finishPasskeyLogin(
    challengeId: string,
    credential: unknown,
    session?: { session_kind: "browser" | "app"; device_name: string },
  ): Promise<PasskeyAuthResponse> {
    if (!inLocalMode()) {
      return api.fetch<PasskeyAuthResponse>("/auth/passkeys/login/finish", {
        method: "POST",
        body: { challenge_id: challengeId, credential, ...session },
      });
    }
    throw new Error("Passkey sign-in is unavailable in local mode.");
  },

  async getSyncState(): Promise<LocalVaultSyncState> {
    if (!inLocalMode()) return "idle";
    await ensureLocalData();
    if (await hasPendingConflicts()) return "conflict";
    return localVault.getSyncState();
  },

  async listQueuedSyncOperations(): Promise<VaultQueuedSyncOperation[]> {
    if (!inLocalMode()) return [];
    await ensureLocalData();
    return localVault.listQueuedSyncOperations();
  },

  async clearQueuedSyncOperations(operationIds?: string[]): Promise<void> {
    if (!inLocalMode()) return;
    await ensureLocalData();
    await localVault.clearQueuedSyncOperations(operationIds);
  },

  async getProviderSyncReadiness(): Promise<LocalProviderSyncReadinessRecord> {
    const provider = getSyncProvider();
    if (!inLocalMode()) {
      return {
        provider,
        connected: false,
        ready: false,
        sync_state: "idle",
        queued_operation_count: 0,
        ready_replay_operation_count: 0,
        missing_payload_count: 0,
        pending_conflict_count: 0,
        blockers: ["Provider sync readiness is only available in local mode"],
        remote_mutation_plans: [],
      };
    }

    await ensureLocalData();
    const adapter = createProviderAdapter(provider);
    const status = await adapter.getStatus();
    const queuedOperations = await localVault.listQueuedSyncOperations();
    const pendingConflictCount = listStoredConflicts().length;
    const blockers: string[] = [];
    let readyReplayOperationCount = 0;
    let missingPayloadCount = 0;

    if (provider !== "none" && !status.connected) {
      blockers.push(`${adapter.label} is not connected`);
    }
    if (pendingConflictCount > 0) {
      blockers.push(
        `${pendingConflictCount} sync conflict${pendingConflictCount === 1 ? "" : "s"} must be resolved before provider sync`,
      );
    }
    if (queuedOperations.length > 0) {
      try {
        const manifest = await this.exportQueuedSyncReplayManifest();
        readyReplayOperationCount = manifest.operation_summary.ready_operation_count;
        missingPayloadCount = manifest.operation_summary.missing_payload_count;
        if (!manifest.replay_readiness.ready) {
          blockers.push(...manifest.replay_readiness.blockers);
        }
      } catch (err) {
        blockers.push(
          err instanceof Error ? err.message : "Queued replay readiness could not be checked",
        );
      }
    }

    const syncState = await this.getSyncState();
    return {
      provider,
      connected: status.connected,
      ready: blockers.length === 0,
      sync_state: syncState,
      queued_operation_count: queuedOperations.length,
      ready_replay_operation_count: readyReplayOperationCount,
      missing_payload_count: missingPayloadCount,
      pending_conflict_count: pendingConflictCount,
      blockers,
      remote_mutation_plans: status.remote_mutation_plans,
    };
  },

  async syncWithProvider(direction: "pull" | "push"): Promise<ProviderSyncResult> {
    if (!inLocalMode()) {
      return {
        synced_at: new Date().toISOString(),
        pulled: 0,
        pushed: 0,
        conflicts: 0,
      };
    }

    await ensureLocalData();
    const provider = getSyncProvider();
    const adapter = createProviderAdapter(provider);
    const readiness = await this.getProviderSyncReadiness();
    if (!readiness.ready) {
      throw new Error(`Provider sync is not ready: ${readiness.blockers.join("; ")}`);
    }
    await localVault.setSyncState("syncing");

    try {
      const status = await adapter.getStatus();
      const providerSyncPayload =
        direction === "push" && provider === "google-drive" && status.remote_sync_enablement.enabled
          ? await this.exportQueuedSyncReplayManifest()
          : undefined;
      const transportOptions: ProviderTransportOptions | undefined =
        provider === "google-drive" &&
        status.remote_sync_enablement.enabled &&
        (direction === "push" || direction === "pull")
          ? {
              apiTransport: createGoogleDriveApiFetchTransport(),
              session: readProviderSession(provider),
              providerSyncPayload,
            }
          : undefined;
      const result =
        direction === "pull"
          ? await adapter.pull(transportOptions)
          : await adapter.push(transportOptions);
      const reviewedResult = await rebuildProviderImportReviewWithLocalQueue(result);
      if (reviewedResult.remote_mutation_attempt?.status === "blocked") {
        await localVault.setSyncState("error");
      } else if (reviewedResult.conflicts > 0) {
        await localVault.setSyncState("conflict");
      } else {
        await localVault.setSyncState("idle");
      }
      return reviewedResult;
    } catch (error) {
      await localVault.setSyncState("error");
      throw error;
    }
  },

  async connectProvider(): Promise<ProviderStatus> {
    if (!inLocalMode()) return UNAVAILABLE_PROVIDER_STATUS;
    await ensureLocalData();
    const adapter = createProviderAdapter(getSyncProvider());
    await adapter.connect();
    return adapter.getStatus();
  },

  async disconnectProvider(): Promise<ProviderStatus> {
    if (!inLocalMode()) return UNAVAILABLE_PROVIDER_STATUS;
    await ensureLocalData();
    const adapter = createProviderAdapter(getSyncProvider());
    await adapter.disconnect();
    return adapter.getStatus();
  },

  async enableProviderRemoteSync(): Promise<ProviderStatus> {
    if (!inLocalMode()) return UNAVAILABLE_PROVIDER_STATUS;
    await ensureLocalData();
    const adapter = createProviderAdapter(getSyncProvider());
    await adapter.enableRemoteSync();
    return adapter.getStatus();
  },

  async disableProviderRemoteSync(reason?: string): Promise<ProviderStatus> {
    if (!inLocalMode()) return UNAVAILABLE_PROVIDER_STATUS;
    await ensureLocalData();
    const adapter = createProviderAdapter(getSyncProvider());
    await adapter.disableRemoteSync(reason);
    return adapter.getStatus();
  },

  async getProviderStatus(): Promise<ProviderStatus> {
    if (!inLocalMode()) return UNAVAILABLE_PROVIDER_STATUS;
    await ensureLocalData();
    const adapter = createProviderAdapter(getSyncProvider());
    return adapter.getStatus();
  },

  async simulateSyncConflict(): Promise<void> {
    await this.simulateSyncConflictForDocument();
  },

  async simulateSyncConflictForDocument(
    documentId?: string,
  ): Promise<DocumentConflictRecord | null> {
    if (!inLocalMode()) return null;
    await ensureLocalData();
    const targetDoc = documentId ? findLocalDocById(documentId) : null;
    const doc =
      targetDoc ??
      (() => {
        const docs = Array.from(localDocs.values());
        if (docs.length === 0) {
          throw new Error("No local document available to conflict");
        }
        return docs.sort((a, b) => (a.updated_at < b.updated_at ? 1 : -1))[0];
      })();
    if (!doc) {
      throw new Error("Document not found");
    }

    const record = queueConflict(buildConflictRecordForDoc(doc));
    await localVault.setSyncState("conflict");
    return record;
  },

  async clearSyncIssue(): Promise<void> {
    if (!inLocalMode()) return;
    await ensureLocalData();
    clearConflicts();
    await localVault.setSyncState("idle");
  },

  async resolveConflict(
    conflictId: string,
    strategy: ConflictResolutionStrategy,
  ): Promise<DocumentConflictRecord> {
    if (!inLocalMode()) {
      throw new Error("Conflict resolution is only available in local mode");
    }

    await ensureLocalData();
    const conflict = getConflict(conflictId);
    if (!conflict) {
      throw new Error("Conflict not found");
    }
    if (conflict.status !== "pending") {
      throw new Error("Conflict already resolved");
    }

    const doc = requireLocalDoc(conflict.space_id, conflict.document_id);
    const nextDoc = applyConflictResolutionToDocument(doc, conflict, strategy);
    localDocs.set(nextDoc.id, nextDoc);
    await persistLocalSnapshot({
      entity_type: "document",
      entity_id: nextDoc.id,
      action: `resolve_conflict:${strategy}`,
    });

    const resolved = resolveConflictRecord(conflictId, strategy);
    await refreshSyncStateFromConflicts();
    return resolved;
  },

  async resolveSyncConflict(strategy: ConflictResolutionStrategy): Promise<void> {
    if (!inLocalMode()) return;
    await ensureLocalData();
    const pending = listStoredConflicts();
    if (pending.length === 0) {
      throw new Error("No active sync conflict to resolve");
    }
    await this.resolveConflict(pending[0].id, strategy);
  },
};

function normalizeSearchFilters(filters: SearchFilters): Record<string, string> {
  return Object.fromEntries(
    Object.entries(filters)
      .map(([key, value]) => [key, value?.trim() ?? ""])
      .filter((entry): entry is [string, string] => Boolean(entry[1])),
  );
}

function localDocMatchesSearchFilters(
  documentId: string,
  filters: Record<string, string>,
): boolean {
  if (filters.title) {
    const title = localDocs.get(documentId)?.title.toLowerCase() ?? "";
    if (!title.includes(filters.title.toLowerCase())) return false;
  }

  if (filters.tag) {
    const tagName = filters.tag.toLowerCase();
    const matchesTag = localDocTags.some((docTag) => {
      if (docTag.document_id !== documentId) return false;
      const tag = localTags.find((item) => item.id === docTag.tag_id);
      return tag?.name.toLowerCase() === tagName;
    });
    if (!matchesTag) return false;
  }

  if (filters.folder) {
    const folder = localFolders.get(localDocs.get(documentId)?.folder_id ?? "");
    const folderFilter = filters.folder.toLowerCase();
    if (
      !folder ||
      (folder.id.toLowerCase() !== folderFilter && folder.name.toLowerCase() !== folderFilter)
    ) {
      return false;
    }
  }

  if (filters.property_name || filters.property_value) {
    const propertyName = filters.property_name?.toLowerCase();
    const propertyValue = filters.property_value?.toLowerCase();
    const matchesProperty = localPropertyValues.some((value) => {
      if (value.document_id !== documentId) return false;
      const schema = localPropertySchemas.find((item) => item.id === value.property_id);
      if (propertyName && schema?.name.toLowerCase() !== propertyName) return false;
      if (
        propertyValue &&
        !String(value.value ?? "")
          .toLowerCase()
          .includes(propertyValue)
      ) {
        return false;
      }
      return true;
    });
    if (!matchesProperty) return false;
  }

  if (filters.has_attachment) {
    const hasAttachment = snapshotAssetCache.some(
      (asset) => extractDocumentIdFromAssetPath(asset, Array.from(localDocs.values())) === documentId,
    );
    if (hasAttachment !== (filters.has_attachment === "true")) return false;
  }

  if (filters.has_cover) {
    const hasCover = Boolean(localDocs.get(documentId)?.cover_image);
    if (hasCover !== (filters.has_cover === "true")) return false;
  }

  if (filters.has_workflow) {
    const hasWorkflow = localDocumentWorkflowCache.some(
      (workflow) => workflow.document_id === documentId,
    );
    if (hasWorkflow !== (filters.has_workflow === "true")) return false;
  }

  if (filters.has_tag) {
    const hasTag = localDocTags.some((docTag) => docTag.document_id === documentId);
    if (hasTag !== (filters.has_tag === "true")) return false;
  }

  if (filters.has_property) {
    const hasProperty = localPropertyValues.some((value) => value.document_id === documentId);
    if (hasProperty !== (filters.has_property === "true")) return false;
  }

  if (filters.is_locked) {
    const isLocked = Boolean(localDocs.get(documentId)?.locked);
    if (isLocked !== (filters.is_locked === "true")) return false;
  }

  if (filters.created_on) {
    const createdAt = localDocs.get(documentId)?.created_at ?? "";
    if (!isSameIsoDate(createdAt, filters.created_on)) return false;
  }

  if (filters.updated_on) {
    const updatedAt = localDocs.get(documentId)?.updated_at ?? "";
    if (!isSameIsoDate(updatedAt, filters.updated_on)) return false;
  }

  return true;
}

function isSameIsoDate(value: string, date: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(date) && value.startsWith(date);
}

export async function resetLocalAppData(): Promise<void> {
  await localVault.reset();
  clearConflicts();
  localSpaces.clear();
  localDocs.clear();
  localFolders.clear();
  snapshotAssetCache = [];
  localInitialized = false;
}

export async function resetAllLocalAppData(): Promise<void> {
  await Promise.all(
    RESETTABLE_SYNC_PROVIDERS.map((provider) => createLocalVaultAdapter(provider).reset()),
  );
  clearConflicts();
  localSpaces.clear();
  localDocs.clear();
  localFolders.clear();
  snapshotAssetCache = [];
  localInitialized = false;
}

export async function __resetLocalDataForTests(): Promise<void> {
  await resetLocalAppData();
}
