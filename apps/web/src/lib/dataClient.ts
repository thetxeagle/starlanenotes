import { buildPageIndex, createPageTraversalBudget, getPageAncestors, getPageSubtree, validatePageDestination } from "./pageHierarchy";
import { buildFolderIndex, createFolderTraversalBudget, getFolderAncestors, getFolderSubtree, validateFolderDestination } from "./folderHierarchy";
import { discoverPendingUploads, retainedPendingUploadCleanups, cleanupPendingUpload, type PendingUploadSelection } from "./pendingUploadCleanup";
import { reconcileAcknowledgedCleanup } from "./originalCleanupTransport";
import type { CleanupResult, CleanupAttempt } from "./permanentDocumentCleanup";
import { listPendingDocumentDeletions, type PendingDocumentDeletion } from "./pendingDocumentDelete";
import { deleteOriginalNativeDocument, listOriginalNativeTrashDocuments, originalNativeTrashBinding, reconcileOriginalNativeDocumentDelete } from "./nativeDocumentDelete";
import { permanentlyDeleteOriginalServerDocument, reconcileRetainedServerDocumentDelete } from "./permanentDocumentDelete";
import { canonicalDeleteOperation, originalDeleteTarget, requirePermanentDeleteTarget } from "./documentDeleteIntent";
import { captureNativeArchiveLocalSession, previewCurrentImportedNativeAttachmentText } from "./nativeArchiveCurrentContext";
import {retainDirectUploadReview,markDirectUploadUnknown,acknowledgeDirectUpload,rememberDirectUploadAcknowledgement} from "./directUploadReview";
import { previewBrowserAttachmentImage } from "./browserAttachmentImage";
import { portableAttachmentReferences } from "./portableAttachmentReferences";
import {isTauri} from "./api";
import {fetchNativeAttachmentHistory,fetchExactAttachmentVersion,imageSelectedAttachment,saveSelectedAttachment,previewSelectedAttachmentText,type SelectedAttachmentVersion} from "./nativeAttachmentProduct";
export { retainedUploadOperationId } from "./uploadOperationStore";
import { prepareUploadOperation, setUploadOperationState, readUploadOperation, validateUploadAcknowledgement, uploadDraftHeaders, type DraftAttachmentAudience, attachRetainedUploadOperation, retainedUploadOperationId } from "./uploadOperationStore";
import { preparePublicationSnapshotRecovery, recoverPreparedPublicationSnapshot, retryPublicationSnapshotRecovery, inspectPublicationSnapshotRecoveries, cancelPreparedPublicationSnapshotRecovery, type SnapshotRecoveryIntent } from "./publicationSnapshotRecovery";
export type { SnapshotRecoveryIntent, SnapshotRecoverySource, SnapshotRecoveryReceipt, SnapshotRecoveryRecord } from "./publicationSnapshotRecovery";
import type { PublicationSnapshotDetail } from "./documentPublishedHistory";
import { listDocumentPublicationSnapshots, getDocumentPublicationSnapshot, readDocumentPublicationSnapshotRaw } from "./documentPublishedHistory";
export type { PublicationSnapshotSummary, PublicationSnapshotDetail, PublicationSnapshotRaw } from "./documentPublishedHistory";
import { prepareDocumentDraftPublication, publishPreparedDocumentDraft, retryDocumentDraftPublication, inspectDocumentDraftPublications, cancelPreparedDocumentDraftPublication, type DraftPublicationIntent } from "./documentDraftPublication";
import { prepareArchiveAssetReferences, assertNativeArchivePlan, assertNativeArchiveReceipt, assertNativeArchiveInventory, type NativeArchivePlan, type NativeArchiveReceipt, type NativeArchiveInventory, type NativeArchiveInventoryRow, type NativeArchiveInspection, type NativeArchiveBinding } from "./nativeArchiveReferences";
import { buildLocalMovePreview, type LocalMovePreviewIntent, type LocalMovePreview } from "./reviewedMoves";
import { prepareGoogleReviewedMove, prepareLocalReviewedMove, type LocalReviewedMoveDecisions, type LocalReviewedMoveRecord } from "./localReviewedMove";
import type { ReviewedDraftTransferRecord } from "./reviewedDraftTransfer";
import { inspectServerDraftBirth, createServerDraftBirth, type DraftBirthOptions } from "./draftBirth";
import { sameOfflineReplayOrigin, verifyOfflineReplayContext, type OfflineReplayOrigin, type OfflineReplayContext } from "./offlineReplayOrigin";
import { inspectPageRestrictions, previewPageRestrictions, applyPageRestrictions } from "@/lib/pageRestrictionManagement";
import { inspectDocumentAccess } from "@/lib/pageAccessInspection";
import { normalizeDocumentMaintenanceFields, normalizeDocumentMaintenance, maintenanceFields, parseDocumentMaintenanceIntent, normalizeDocumentMaintenanceTargets, normalizeDocumentMaintenanceCatalogs, type DocumentMaintenance, type DocumentMaintenanceSource, type DocumentMaintenanceTargets, type DocumentMaintenanceCatalog } from "@/lib/documentMaintenance";
export type { DocumentMaintenance, DocumentMaintenanceSource, DocumentMaintenanceTargets } from "@/lib/documentMaintenance";
import { normalizeHumanIdentifier, normalizeDocumentIdentifiers, humanIdentifierKey, parseDocumentIdentifierIntent, type DocumentIdentifierSource } from "@/lib/humanIdentifier";
import {
  validatePropertySchema,
  validatePropertySchemaChange,
  validatePropertyValue,
} from "@/lib/propertyValidation";
import { stripTemplateGuidance } from "@/lib/templatePublication";
import { api, captureOfflineServerContext, getApiBase, isApiError, ApiError } from "@/lib/api";
import {
  createLocalVaultAdapter,
  assertReviewedMoveRecords,
  assertReviewedDraftTransferRecords,
  assertLocalVaultSnapshot,
  assertImportedQueuedSyncOperations,
  LOCAL_VAULT_SCHEMA_VERSION,
  type LocalVaultSnapshot,
  type LocalVaultAdapter,
  type LocalVaultSyncState,
  type VaultAsset,
  type VaultDocumentWorkflowEvent,
  type VaultDocumentWorkflow,
  type VaultDocument,
  type VaultDocumentDraft,
  type VaultDocTag,
  type VaultFolder,
  type VaultPropertySchema,
  type VaultPropertyValue,
  type VaultQueuedSyncOperation,
  type VaultSearchIndexEntry,
  type VaultTag,
  type VaultTemplate,
  type VaultWorkflowTemplate,
  type VaultSnapshotMutationBuilder,
  withGoogleReviewedMovePublicationLock,
  requireNoPendingGoogleReviewedMoves,
} from "@/lib/localVault";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { queueOfflineServerMutation, queueOfflineServerAttachment, cacheServerResponse, readCachedServerResponse, serverCacheScopeForSession } from "@/lib/offlineServerCache";
import { getStorageSelection, getSyncProvider, type SyncProviderKind } from "@/lib/storageProviders";
import {
  buildProviderRemoteImportApplyPlan,
  buildProviderRemoteImportDryRun,
  createProviderAdapter,
  parseDocumentMetadataIntent,
  providerPropertySchemaFieldsFailure,
  executeProviderRemoteImportApplyPlan,
  type ProviderRemoteImportApplyExecutionResult,
  type ProviderRemoteImportApplyPlanOperation,
  type ProviderTransportOptions,
  type ProviderRemoteMutationPlan,
  type ProviderRemoteSyncManifest,
  assertNoReviewedMoveProviderManifest,
  type ProviderStatus,
  type ProviderSyncResult,
} from "@/lib/providerAdapters";
import { captureGoogleDriveReviewedMoveEnrollment, createGoogleDriveApiFetchTransport, readProviderSession, type GoogleDriveReviewedMoveEnrollment } from "@/lib/providerSession";
import { filterDataviewDocuments, type DataviewDocument } from "@/lib/dataview";
import {
  readSharedSpaceDownloadCheckpoints,
  type SharedSpaceDownloadCheckpoint,
} from "@/lib/sharedSpaceDownloads";
import {
  buildNativeLocalDatabaseReplayContract,
  NATIVE_LOCAL_DATABASE_COMMANDS,
  nativeJsonValueEquals,
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
import { DEFAULT_NEW_DOCUMENT_CONTENT } from "@/lib/documentContent";

const MAX_DOCUMENT_CONTENT_BYTES = 5 * 1024 * 1024;
const PERSONAL_SPACE_NAME = "Personal Space";
const CROSS_SPACE_MOVE_REVIEW_REQUIRED =
  "Cross-space moves require a reviewed metadata and access preview before relocation. Keep this item in its current space until reviewed moves are available.";

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
  description?: string;
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

export type { DocumentIdentifierSource } from "@/lib/humanIdentifier";

export interface DocumentRecord {
  page_access_source?: "direct_page_grant";
  page_edit_allowed?: boolean;
  identifier_edit_allowed?: boolean;
  maintenance_edit_allowed?: boolean;
  human_identifier?: string | null;
  maintenance_user_id?: string | null;
  maintenance_group_id?: string | null;
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
  published_at?: string;
  archived_at?: string;
  deleted_at?: string;
  draft_id?: string;
  draft_source_digest?: string;
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
  published_metadata?: import("@starlane-notes/shared-types").PublishedDocumentMetadata;
  published_source_digest?: string;
  id: string;
  title: string;
  content: string;
}

export interface DraftSummaryRecord {
  id: string;
  document_id: string;
  space_id: string;
  owner_id: string;
  owner_name: string;
  document_title: string;
  title: string;
  access_role: "owner" | "editor" | "viewer";
  can_publish: boolean;
  created_at: string;
  updated_at: string;
}

export interface DocumentDraftEditSource {
  schema_version:1;space_id:string;document_id:string;draft_id:string;draft_source_digest:string;
  draft:{title:string;content:string;icon:string|null;cover_image:string|null};
  selected_draft:DraftRecord;locked:boolean;locked_by:string|null;page_edit_allowed:true;page_updated_at:string;page_published:boolean;
}

export type DraftSaveHoldReason = "source_required" | "source_conflict" | "outcome_unknown" | "queued";
export class DraftSaveHeldError extends Error {
  constructor(public readonly reason: DraftSaveHoldReason, message: string) { super(message); this.name = "DraftSaveHeldError"; }
}
export function isDraftSaveHeldError(error: unknown): error is DraftSaveHeldError { return error instanceof DraftSaveHeldError; }
const validDraftSourceDigest = (value: unknown): value is string => typeof value === "string" && /^[0-9a-f]{64}$/.test(value);

/** Source selection provenance only: no bearer, refresh secret or new authorization. */
export interface DraftSaveSelection {
  mode: ReturnType<typeof getCurrentLaunchMode>;
  accountId: string;
  serverBase: string;
  storageSelection: string;
  origin: OfflineReplayOrigin | null;
}
export function captureDraftSaveSelection(accountId?: string): DraftSaveSelection {
  const mode = getCurrentLaunchMode();
  const context = mode === "server" ? captureOfflineServerContext() : null;
  if (context && accountId && context.origin.accountId !== accountId) throw new DraftSaveHeldError("source_conflict", "Draft session changed. Keep your edits and review the page before saving.");
  return { mode, accountId: accountId ?? context?.origin.accountId ?? "local-workspace", serverBase: mode === "server" ? getApiBase() : "local-workspace", storageSelection: JSON.stringify(getStorageSelection()), origin: context ? { ...context.origin } : null };
}
export function verifyDraftSaveSelection(selection: DraftSaveSelection, accountId?: string): void {
  const current = captureDraftSaveSelection(accountId);
  if (selection.mode !== current.mode || selection.accountId !== current.accountId || selection.serverBase !== current.serverBase || selection.storageSelection !== current.storageSelection || selection.mode === "server" && !sameOfflineReplayOrigin(selection.origin, current.origin)) throw new DraftSaveHeldError("source_conflict", "Draft session or storage changed. Your edits remain retained; review the original page before saving.");
}

export interface DraftRecord extends DraftSummaryRecord {
  birth_operation_id?: string;
  source_digest?: string;
  icon?: string;
  cover_image?: string;
  content: string;
  base_updated_at: string;
}

export interface DraftAccessRecord {
  user_id: string;
  display_name: string;
  email: string;
  role: "viewer" | "editor";
  created_at: string;
  updated_at: string;
}

export interface DraftAccessCandidateRecord {
  user_id: string;
  display_name: string;
  email: string;
}

export interface BacklinkRecord {
  document_id: string;
  title: string;
  space_id: string;
  space_name: string;
}

interface WikiLinkReference {
  title: string;
  documentId?: string;
}

function extractWikiLinkReferences(content: string): WikiLinkReference[] {
  const references: WikiLinkReference[] = [];
  const pattern = /\[\[([^\]\n]*)\]\]/g;
  for (const match of content.matchAll(pattern)) {
    const raw = match[1] ?? "";
    const separator = raw.indexOf("|");
    const title = (separator < 0 ? raw : raw.slice(0, separator)).trim();
    if (!title) continue;
    const documentId = separator < 0 ? "" : raw.slice(separator + 1).trim();
    references.push({ title, ...(documentId ? { documentId } : {}) });
  }
  return references;
}

function containsDocumentBacklink(
  content: string,
  targetTitle: string,
  targetDocumentId: string,
): boolean {
  const normalizedTargetTitle = targetTitle.trim().toLowerCase();
  return extractWikiLinkReferences(content).some((reference) =>
    reference.documentId
      ? reference.documentId === targetDocumentId
      : reference.title.trim().toLowerCase() === normalizedTargetTitle,
  );
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
  human_identifier?: string | null;
  maintenance_user_id?: string | null;
  maintenance_group_id?: string | null;
  document_id: string;
  title: string;
  space_id: string;
  space_name: string;
  excerpt: string;
  updated_at: string;
  author: string;
  author_id?: string;
  folder_id?: string;
  folder_name?: string;
  is_draft?: boolean;
  draft_id?: string;
}

export interface SpaceMemberRecord {
  user_id: string;
  display_name: string;
  email: string;
  role: string;
}

export interface SearchFilters {
  identifier?: string;
  maintenance_user?: string;
  maintenance_group?: string;
  title?: string;
  space_id?: string;
  folder?: string;
  user?: string;
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
  is_draft?: string;
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
  device_name: string;
  ip_address?: string;
  user_agent?: string;
  session_kind?: "browser" | "app";
  client_platform: "web" | "mobile_app" | "desktop_app";
  is_current: boolean;
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
  audience?:DraftAttachmentAudience|null;
  logical_attachment_id?: string | null;
  document_id?: string;
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
  draft_id?:string;
}

export interface DocumentDataAssetInput {
  name: string;
  mime_type: string;
  data_url: string;
  space_id?: string;
  document_id?: string;
  draft_id?:string;
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

export interface TemplateDraftRecord {
  id: string;
  template_id: string;
  space_id: string;
  owner_id: string;
  snapshot: TemplateInput;
  created_at: string;
  updated_at: string;
  last_autosaved_at?: string | null;
  lock_owner_id?: string | null;
  lock_expires_at?: string | null;
}

export interface TemplateRevisionRecord {
  id: string;
  template_id: string;
  revision_number: number;
  snapshot: TemplateInput;
  created_by: string;
  created_at: string;
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
  draft_id?:string;
}

/** Local provenance only. A reviewed server transfer is required before scoped replay. */
export interface LocalDraftAssetSource {
  schema_version: 1;
  space_id: string;
  document_id: string;
  draft_id: string;
  draft_created_at: string;
}

function localDraftAssetSource(context: LocalAssetContext): LocalDraftAssetSource | undefined {
  if (context.draft_id === undefined) return undefined;
  const draft = localDrafts.find(row => row.id === context.draft_id && row.space_id === context.space_id && row.document_id === context.document_id);
  if (!draft || !draft.created_at) throw new Error("The original local draft is unavailable; retain the selected file and review its source before importing");
  return { schema_version: 1, space_id: draft.space_id, document_id: draft.document_id, draft_id: draft.id, draft_created_at: draft.created_at };
}

export class LocalAssetImportHeldError extends Error {
  readonly asset: LocalAssetRecord;
  constructor(asset: LocalAssetRecord, readonly original: { selection: DraftSaveSelection; source?: LocalDraftAssetSource }) {
    super("The original asset import could not be adopted in this context. Keep the original file and review this held import, including its original queue outcome; no asset was redirected to the new selection.");
    this.name = "LocalAssetImportHeldError";
    this.asset = { ...asset };
  }
}

function verifyLocalAssetSource(selection: DraftSaveSelection, source: LocalDraftAssetSource | undefined, token: string | null): void {
  verifyDraftSaveSelection(selection);
  if (api.getToken() !== token) throw new Error("The original import account changed");
  if (source && JSON.stringify(localDraftAssetSource(source)) !== JSON.stringify(source)) throw new Error("The original local draft incarnation changed");
}

async function persistOriginalScopedLocalAsset(
  adapter: LocalVaultAdapter,
  asset: LocalAssetRecord,
  source: LocalDraftAssetSource,
  selection: DraftSaveSelection,
  token: string | null,
  dataUrl?: string,
): Promise<LocalAssetRecord> {
  const operation: VaultQueuedSyncOperation = {
    id: makeId("syncop"), entity_type: "asset", entity_id: asset.id,
    action: "create", queued_at: new Date().toISOString(), attempts: 0,
    payload: { ...asset, space_id: source.space_id, document_id: source.document_id,
      draft_id: source.draft_id, local_draft_source: source,
      ...(dataUrl === undefined ? {} : { data_url: dataUrl }) },
  };
  try {
    verifyLocalAssetSource(selection, source, token);
    const saved = await adapter.commitSnapshotMutation(snapshot => {
      verifyLocalAssetSource(selection, source, token);
      const actual = snapshot.drafts?.find(row => row.id === source.draft_id && row.space_id === source.space_id && row.document_id === source.document_id);
      if (!actual || actual.created_at !== source.draft_created_at || !snapshot.documents.some(row => row.id === source.document_id && row.space_id === source.space_id)) throw new Error("The persisted original local draft incarnation changed");
      if (snapshot.assets.some(row => row.id === asset.id)) throw new Error("The original imported asset identity is already occupied");
      snapshot.assets = [...snapshot.assets, { ...asset }];
      return { snapshot, operation };
    });
    // A completed original commit is retained even if selection changed while
    // awaiting its ACK. Never update the new context's cache or infer adoption.
    verifyLocalAssetSource(selection, source, token);
    snapshotAssetCache = saved.assets;
    return asset;
  } catch { throw new LocalAssetImportHeldError(asset, { selection, source }); }
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
  draft_bindings?: LocalSyncReplayDraftBindingSidecarRecord;
  operations: LocalSyncReplayOperationRecord[];
}

export interface LocalSyncReplayDraftBindingSidecarRecord {
  schema_version: 1;
  binding_count: number;
  bindings: LocalSyncReplayDraftBindingRecord[];
}

export interface LocalSyncReplayDraftBindingRecord {
  operation_id: string;
  operation_fingerprint: string;
  transfer_operation_id: string;
  transfer_request_fingerprint: string;
  local_source: {
    provider: string;
    root_identity: string;
    space_id: string;
    document_id: string;
    draft_id: string;
    draft_created_at: string;
    snapshot_digest: string;
  };
  target_space_id: string;
  target_document_id: string;
  target_draft_id: string;
  target_draft_created_at: string;
  target_source_digest: string;
}

export interface ReviewedDraftTransferRequestRecord {
  schema_version: 1;
  operation_id: string;
  local_source: LocalSyncReplayDraftBindingRecord["local_source"];
  server_source:
    | { kind: "draft"; draft_id: string; draft_created_at: string; source_digest: string }
    | { kind: "published_page"; page_updated_at: string; source_digest: string };
  reviewed_draft: { title: string; icon: string | null; cover_image: string | null; content: string };
  selected_operation_fingerprints: string[];
}

export interface ReviewedDraftTransferReceiptRecord {
  schema_version: 1;
  operation_id: string;
  request_fingerprint: string;
  server_actor_id: string;
  server_space_id: string;
  server_document_id: string;
  target_draft_id: string;
  target_draft_created_at: string;
  target_source_digest: string;
  local_source: LocalSyncReplayDraftBindingRecord["local_source"];
  selected_operation_fingerprints: string[];
  created_at: string;
}

export interface LocalSyncReplayOperationResultRecord {
  operation_id: string;
  entity_type: string;
  action: string;
  status: "applied" | "skipped" | "failed" | "unsupported" | string;
  message: string;
  published_source?: { published_source_digest: string; payload_fingerprint: string; document_id: string; space_id: string; original_source_digest: string };
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

// Validate the portable raw property boundary before legacy collection defaults.
// Omitted optional collections/order remain legacy-compatible; present malformed
// fields must never be normalized into an empty vault or a different order.
function assertRawImportedProperties(snapshot: LocalVaultSnapshot): void {
  for (const field of ["property_schemas", "property_values"] as const) {
    if (Object.prototype.hasOwnProperty.call(snapshot, field) && !Array.isArray(snapshot[field]))
      throw new Error(`Imported ${field} must be an array when present`);
  }
  const ids = new Set<string>();
  for (const raw of snapshot.property_schemas ?? []) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw))
      throw new Error("Imported property schema must be an object");
    if (typeof raw.id !== "string" || !raw.id.trim())
      throw new Error("Imported property schema id must be a nonempty string");
    if (ids.has(raw.id)) throw new Error("Duplicate imported property schema ids are not allowed");
    ids.add(raw.id);
    if (typeof raw.name !== "string") throw new Error("Imported property name must be a string");
    const failure = providerPropertySchemaFieldsFailure(raw as unknown as Record<string, unknown>);
    if (failure) throw new Error(`Imported property schema has invalid ${failure}`);
    validatePropertySchema(raw);
  }
  const valueIds = new Set<string>();
  for (const raw of snapshot.property_values ?? []) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw) || typeof raw.id !== "string" || !raw.id.trim())
      throw new Error("Imported property value id must be a nonempty string");
    if (valueIds.has(raw.id)) throw new Error("Duplicate property value import ids are not allowed");
    valueIds.add(raw.id);
  }
}

function prepareImportedSnapshot(snapshot: unknown): LocalVaultSnapshot {
  assertLocalVaultSnapshot(snapshot);
  assertImportedQueuedSyncOperations(snapshot);
  assertRawImportedProperties(snapshot);
  const candidate = normalizeLocalVaultSnapshot(snapshot);
  validateImportedSnapshotProperties(candidate);
  return candidate;
}

function normalizeLocalVaultSnapshot(snapshot: LocalVaultSnapshot): LocalVaultSnapshot {
  return {
    schema_version: LOCAL_VAULT_SCHEMA_VERSION,
    ...(snapshot.reviewed_move_records !== undefined ? { reviewed_move_records: JSON.parse(JSON.stringify(snapshot.reviewed_move_records)) as LocalReviewedMoveRecord[] } : {}),
    ...(snapshot.reviewed_draft_transfer_records !== undefined ? { reviewed_draft_transfer_records: JSON.parse(JSON.stringify(snapshot.reviewed_draft_transfer_records)) as ReviewedDraftTransferRecord[] } : {}),
    spaces: snapshot.spaces.map(normalizeVaultSpace),
    folders: snapshot.folders,
    ...(snapshot.maintenance_target_catalogs !== undefined ? { maintenance_target_catalogs: normalizeDocumentMaintenanceCatalogs(snapshot.maintenance_target_catalogs) } : {}),
    documents: normalizeDocumentIdentifiers(snapshot.documents).map(normalizeDocumentMaintenanceFields).map((doc) =>
      normalizeVaultDocument(
        doc,
        snapshot.drafts?.some((draft) => draft.document_id === doc.id),
      ),
    ),
    drafts: Array.isArray(snapshot.drafts) ? snapshot.drafts : [],
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

async function blobToDataUrl(blob: Blob): Promise<string> {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    binary += String.fromCharCode(...bytes.slice(index, index + chunkSize));
  }
  return `data:${blob.type || "application/octet-stream"};base64,${btoa(binary)}`;
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
    return (
      (Number.isFinite(rightTime) ? rightTime : 0) - (Number.isFinite(leftTime) ? leftTime : 0)
    );
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

async function hydrateStaticSiteExportAssets(
  snapshot: LocalVaultSnapshot,
): Promise<LocalVaultSnapshot> {
  if (!canUseTauriNativeAssetImport()) return snapshot;
  const assets = await Promise.all(
    snapshot.assets.map(async (asset) => {
      if (asset.url.startsWith("data:") || (!asset.vault_path && !asset.url)) return asset;
      const packed = await packNativeReplayAssetPayload(asset);
      const dataUrl =
        packed &&
        typeof packed === "object" &&
        typeof (packed as { data_url?: unknown }).data_url === "string"
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
  description?: string;
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
    description: space.description,
    member_role: "owner",
    owner_username: space.owner_username,
    owner_display_name: space.owner_display_name,
    created_by: space.created_by,
  };
}

interface LocalDocState {
  published_source?: VaultDocument["published_source"];
  human_identifier?: string | null;
  maintenance_user_id?: string | null;
  maintenance_group_id?: string | null;
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
  published_at?: string;
  archived_at?: string;
  deleted_at?: string;
}

interface LocalFolderState {
  id: string;
  space_id: string;
  name: string;
  icon?: string;
  parent_id?: string;
}

const localVault = createLocalVaultAdapter(getSyncProvider());
const localSpaces = new Map<string, LocalSpaceState>();
const localDocs = new Map<string, LocalDocState>();
let localDrafts: VaultDocumentDraft[] = [];
const localFolders = new Map<string, LocalFolderState>();
let localInitialized = false;
let hydratedLocalProvider: SyncProviderKind | null = null;
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

// A modern shell may support atomic commits without preserving generic replay provenance.
// This gate covers imported queues and newly bound published content producers;
// it never upgrades legacy queue records into authority.
async function requireNativeReplayQueueCapability(
  snapshot: LocalVaultSnapshot,
  provider: SyncProviderKind,
  prospectiveBoundQueue = false,
): Promise<void> {
  if ((!snapshot.sync_state?.queued_operations?.length && !prospectiveBoundQueue) || !canUseTauriNativeVaultArchive()) return;
  const requireContext = () => {
    if (!inLocalMode() || getSyncProvider() !== provider) {
      throw new Error("Vault replay queue context changed before persistence");
    }
  };
  requireContext();
  let descriptor: unknown;
  try {
    descriptor = await invokeTauri<unknown>(NATIVE_LOCAL_DATABASE_COMMANDS.describe, { provider });
  } catch {
    requireContext();
    throw new Error("Update the desktop shell before retaining a pending replay queue");
  }
  requireContext();
  if (!descriptor || typeof descriptor !== "object" || Array.isArray(descriptor)) {
    throw new Error("Update the desktop shell before retaining a pending replay queue");
  }
  const actual = descriptor as Record<string, unknown>;
  if (actual.schema_version !== LOCAL_VAULT_SCHEMA_VERSION || actual.provider !== provider ||
      actual.store_kind !== "sqlite" || actual.supports_atomic_snapshot_commit !== true ||
      actual.supports_operation_queue !== true || actual.supports_lossless_replay_queue !== true) {
    throw new Error("Update the desktop shell before retaining a pending replay queue");
  }
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
  hydratedLocalProvider = localVault.provider;
  if (purgeExpiredLocalDrafts()) await persistLocalSnapshot();

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
      published_at: now,
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
      description: space.description,
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
      icon: folder.icon,
      parent_id: folder.parent_id,
    })),
    documents: Array.from(localDocs.values()).map((doc) => ({
      id: doc.id,
      human_identifier: doc.human_identifier,
      maintenance_user_id: doc.maintenance_user_id,
      maintenance_group_id: doc.maintenance_group_id,
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
      published_at: doc.published_at,
      published_source: doc.published_source,
      archived_at: doc.archived_at,
      deleted_at: doc.deleted_at,
    })),
    maintenance_target_catalogs: localMaintenanceCatalogs,
    ...(localReviewedMoveRecords !== undefined ? { reviewed_move_records: JSON.parse(JSON.stringify(localReviewedMoveRecords)) as LocalReviewedMoveRecord[] } : {}),
    ...(localReviewedDraftTransferRecords !== undefined ? { reviewed_draft_transfer_records: JSON.parse(JSON.stringify(localReviewedDraftTransferRecords)) as ReviewedDraftTransferRecord[] } : {}),
    drafts: localDrafts,
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
let localMaintenanceCatalogs: DocumentMaintenanceCatalog[] = [];
let localReviewedMoveRecords: LocalReviewedMoveRecord[] | undefined;
let localReviewedDraftTransferRecords: ReviewedDraftTransferRecord[] | undefined;
let localTemplateCache: VaultTemplate[] = [];
let localWorkflowTemplateCache: VaultWorkflowTemplate[] = [];
let localDocumentWorkflowCache: VaultDocumentWorkflow[] = [];
let localDocumentWorkflowEventCache: VaultDocumentWorkflowEvent[] = [];
let localPropertySchemas: VaultPropertySchema[] = [];
let localPropertyValues: VaultPropertyValue[] = [];
let committedPropertySchemas: VaultPropertySchema[] = [];
let committedPropertyValues: VaultPropertyValue[] = [];
let localPersistenceTail: Promise<unknown> = Promise.resolve();

function serializeLocalPersistence<T>(work: () => Promise<T>): Promise<T> {
  const next = localPersistenceTail.catch(() => undefined).then(work);
  localPersistenceTail = next;
  return next;
}

function publishCommittedProperties(snapshot: LocalVaultSnapshot): void {
  localPropertySchemas = snapshot.property_schemas ?? [];
  localPropertyValues = snapshot.property_values ?? [];
  committedPropertySchemas = JSON.parse(JSON.stringify(localPropertySchemas));
  committedPropertyValues = JSON.parse(JSON.stringify(localPropertyValues));
}

function mergePropertyChanges<T extends { id: string }>(
  baseline: T[],
  local: T[],
  committed: T[],
): T[] {
  const same = (left: T | undefined, right: T | undefined) =>
    JSON.stringify(left) === JSON.stringify(right);
  const baseById = new Map(baseline.map((item) => [item.id, item]));
  const localById = new Map(local.map((item) => [item.id, item]));
  const merged = new Map(committed.map((item) => [item.id, item]));
  for (const id of new Set([...baseById.keys(), ...localById.keys()])) {
    const before = baseById.get(id);
    const after = localById.get(id);
    if (same(before, after)) continue;
    if (!same(merged.get(id), before) && !same(merged.get(id), after)) {
      throw new Error("Properties changed in another window; reload before retrying");
    }
    if (after) merged.set(id, after);
    else merged.delete(id);
  }
  return Array.from(merged.values());
}

async function commitLocalPropertyMutation(
  mutate: VaultSnapshotMutationBuilder,
): Promise<LocalVaultSnapshot> {
  return serializeLocalPersistence(async () => {
    const snapshot = await localVault.commitSnapshotMutation(mutate);
    publishCommittedProperties(snapshot);
    return snapshot;
  });
}
/** Commit folder topology and its replay operation together against latest durable state. */
async function commitLocalFolderWrite(
  folder: LocalFolderState,
  expected: LocalFolderState | undefined,
  queue: boolean,
): Promise<void> {
  await serializeLocalPersistence(async () => {
    const saved = await localVault.commitSnapshotMutation(snapshot => {
      if (!snapshot.spaces.some(space => space.id === folder.space_id)) throw new Error("Space not found");
      const current = snapshot.folders.find(row => row.id === folder.id);
      if (!expected && current) throw new Error("A folder with this import id already exists");
      if (expected && (!current || Object.entries(expected).some(([key, value]) => JSON.stringify(current[key as keyof typeof current]) !== JSON.stringify(value))))
        throw new Error("Folder changed in another window; reload before retrying");
      const budget = createFolderTraversalBudget();
      budget.visit(folder.id);
      validateFolderDestination(buildFolderIndex(snapshot.folders), folder.space_id, folder.parent_id, expected ? folder.id : undefined, budget);
      snapshot.folders = expected ? snapshot.folders.map(row => row.id === folder.id ? { ...row, ...folder } : row) : [...snapshot.folders, folder];
      return { snapshot, operation: queue ? { entity_type: "folder", entity_id: folder.id, action: expected ? "update" : "create", payload: folder } : undefined };
    });
    const committed = saved.folders.find(row => row.id === folder.id);
    if (committed) localFolders.set(folder.id, { ...committed });
  });
}

// Folder deletion removes organization only. Build against the latest durable
// snapshot and publish caches after data plus its optional replay operation commit.
async function commitLocalFolderDeletion(
  requestedSpaceId: string | undefined,
  folderId: string,
  queue: boolean,
): Promise<boolean> {
  return serializeLocalPersistence(async () => {
    let removed = false;
    const cachedBefore = new Map(
      buildSnapshot().documents.map((document) => [document.id, document]),
    );
    const cachedFoldersBefore = new Map(
      buildSnapshot().folders.map((folder) => [folder.id, folder]),
    );
    const removedFolders = new Set<string>();
    const affectedPages = new Set<string>();
    const saved = await localVault.commitSnapshotMutation((snapshot) => {
      const folder = snapshot.folders.find((item) => item.id === folderId);
      if (!folder) {
        if (queue) throw new Error("Folder not found");
        const cached = cachedFoldersBefore.get(folderId);
        if (cached) {
          if (requestedSpaceId !== undefined && cached.space_id !== requestedSpaceId)
            throw new Error("Folder not found");
          for (const item of getFolderSubtree(buildFolderIndex(cachedFoldersBefore.values()), cached.space_id, cached.id))
            if (!snapshot.folders.some(current => current.id === item.id)) removedFolders.add(item.id);
        }
        return { snapshot };
      }
      const spaceId = requestedSpaceId ?? folder.space_id;
      if (!snapshot.spaces.some((space) => space.id === spaceId))
        throw new Error("Space not found");
      if (folder.space_id !== spaceId) throw new Error("Folder not found");
      const folderIds = new Set(getFolderSubtree(buildFolderIndex(snapshot.folders), spaceId, folderId).map(item => item.id));
      for (const document of snapshot.documents) {
        if (!document.folder_id || !folderIds.has(document.folder_id)) continue;
        if (document.space_id !== spaceId)
          throw new Error("Folder page associations are inconsistent");
        if (document.locked || document.archived_at || document.deleted_at)
          throw new Error("Unlock or restore affected pages before deleting their folder");
      }
      for (const id of folderIds) removedFolders.add(id);
      for (const document of snapshot.documents)
        if (document.folder_id && folderIds.has(document.folder_id)) affectedPages.add(document.id);
      snapshot.folders = snapshot.folders.filter((item) => !folderIds.has(item.id));
      snapshot.documents = snapshot.documents.map((document) =>
        document.folder_id && folderIds.has(document.folder_id)
          ? { ...document, folder_id: undefined }
          : document,
      );
      removed = true;
      return {
        snapshot,
        operation: queue
          ? { entity_type: "folder", entity_id: folderId, action: "delete", payload: folder }
          : undefined,
      };
    });
    // Generic local writers can stage cache changes while atomic I/O awaits.
    // Publish only this deletion's affected records, retaining their staged field
    // deltas and every unrelated cache/metadata intent for its queued persistence.
    for (const id of removedFolders) localFolders.delete(id);
    for (const current of localDocs.values())
      if (current.folder_id && removedFolders.has(current.folder_id)) affectedPages.add(current.id);
    for (const document of saved.documents) {
      if (!affectedPages.has(document.id)) continue;
      const before = cachedBefore.get(document.id);
      const current = localDocs.get(document.id);
      if (before && !current) continue;
      const pending: Partial<LocalDocState> = {};
      if (before && current) {
        for (const key of Object.keys(current) as Array<keyof LocalDocState>)
          if (JSON.stringify(current[key]) !== JSON.stringify(before[key]))
            Object.assign(pending, { [key]: current[key] });
      }
      const normalized = normalizeVaultDocument(
        document,
        (saved.drafts ?? []).some((draft) => draft.document_id === document.id),
      );
      const folder = Object.prototype.hasOwnProperty.call(pending, "folder_id")
        ? pending.folder_id
        : normalized.folder_id;
      localDocs.set(document.id, {
        ...normalized,
        created_by: normalized.created_by ?? LOCAL_SYSTEM_USER,
        last_updated_by: normalized.last_updated_by ?? LOCAL_SYSTEM_USER,
        created_at: normalized.created_at ?? normalized.updated_at,
        ...pending,
        folder_id: folder && !removedFolders.has(folder) ? folder : undefined,
      });
    }
    // Intent staged before acknowledgement is ordered before organization removal:
    // retain its content/identity and detach any newly staged child from removed folders.
    for (const [id, current] of localDocs)
      if (current.folder_id && removedFolders.has(current.folder_id))
        localDocs.set(id, { ...current, folder_id: undefined });
    for (const [id, current] of localFolders) {
      if (!current.parent_id || !removedFolders.has(current.parent_id)) continue;
      const canonical = saved.folders.find((folder) => folder.id === id);
      const before = cachedFoldersBefore.get(id);
      const pending: Partial<LocalFolderState> = {};
      if (canonical && before)
        for (const key of Object.keys(current) as Array<keyof LocalFolderState>)
          if (JSON.stringify(current[key]) !== JSON.stringify(before[key]))
            Object.assign(pending, { [key]: current[key] });
      const next = canonical ? { ...canonical, ...pending } : current;
      localFolders.set(id, {
        ...next,
        parent_id:
          next.parent_id && !removedFolders.has(next.parent_id) ? next.parent_id : undefined,
      });
    }
    return removed;
  });
}
let localTags: VaultTag[] = [];
let localDocTags: VaultDocTag[] = [];
let localSearchIndex: VaultSearchIndexEntry[] = [];

type LocalQueuedSyncOperationInput = Pick<
  VaultQueuedSyncOperation,
  "entity_type" | "entity_id" | "action" | "payload"
> &
  Partial<Pick<VaultQueuedSyncOperation, "queued_at" | "replay_provider" | "server_context" | "last_error">>;

function removeLocalDocumentMetadata(documentIds: Set<string>): void {
  if (documentIds.size === 0) return;
  localDrafts = localDrafts.filter((draft) => !documentIds.has(draft.document_id));
  localDocTags = localDocTags.filter((docTag) => !documentIds.has(docTag.document_id));
  localPropertyValues = localPropertyValues.filter((value) => !documentIds.has(value.document_id));
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

function purgeExpiredLocalDrafts(): boolean {
  const cutoff = Date.now() - 120 * 24 * 60 * 60 * 1000;
  const remaining = localDrafts.filter((draft) => {
    const updated = Date.parse(draft.updated_at);
    return !Number.isFinite(updated) || updated > cutoff;
  });
  const changed = remaining.length !== localDrafts.length;
  localDrafts = remaining;
  return changed;
}

function hydrateFromSnapshot(snapshot: LocalVaultSnapshot): void {
  localSpaces.clear();
  localFolders.clear();
  localDocs.clear();
  localDrafts = snapshot.drafts ?? [];
  assertReviewedMoveRecords(snapshot.reviewed_move_records);
  localReviewedMoveRecords = snapshot.reviewed_move_records === undefined ? undefined : JSON.parse(JSON.stringify(snapshot.reviewed_move_records)) as LocalReviewedMoveRecord[];
  assertReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
  localReviewedDraftTransferRecords = snapshot.reviewed_draft_transfer_records === undefined ? undefined : JSON.parse(JSON.stringify(snapshot.reviewed_draft_transfer_records)) as ReviewedDraftTransferRecord[];
  localMaintenanceCatalogs = normalizeDocumentMaintenanceCatalogs(snapshot.maintenance_target_catalogs);

  for (const space of snapshot.spaces) {
    const normalizedSpace = normalizeVaultSpace(space);
    localSpaces.set(space.id, {
      id: normalizedSpace.id,
      name: normalizedSpace.name,
      slug: normalizedSpace.slug,
      description: normalizedSpace.description,
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
      icon: folder.icon,
      parent_id: folder.parent_id,
    });
  }

  for (const doc of snapshot.documents) {
    const normalizedDoc = normalizeVaultDocument(
      doc,
      localDrafts.some((draft) => draft.document_id === doc.id),
    );
    localDocs.set(doc.id, {
      id: normalizedDoc.id,
      human_identifier: normalizedDoc.human_identifier,
      maintenance_user_id: normalizedDoc.maintenance_user_id,
      maintenance_group_id: normalizedDoc.maintenance_group_id,
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
      published_at: normalizedDoc.published_at,
      published_source: normalizedDoc.published_source,
      archived_at: normalizedDoc.archived_at,
      deleted_at: normalizedDoc.deleted_at,
    });
  }

  snapshotAssetCache = snapshot.assets;
  localTemplateCache = snapshot.templates ?? [];
  localWorkflowTemplateCache = snapshot.workflow_templates ?? [];
  localDocumentWorkflowCache = snapshot.document_workflows ?? [];
  localDocumentWorkflowEventCache = snapshot.document_workflow_events ?? [];
  publishCommittedProperties(snapshot);
  localTags = snapshot.tags ?? [];
  localDocTags = snapshot.doc_tags ?? [];
  localSearchIndex = snapshot.search_index ?? buildLocalSearchIndexEntries();
}

export interface LocalMetadataDraftContext {
  id: string;
  updated_at: string;
  base_updated_at: string;
}
type LocalMetadataField = "title" | "icon" | "cover_image" | "folder_id" | "parent_id";
type LocalMetadataPatch = Partial<Record<LocalMetadataField, string | null>>;

function metadataRevisionEqual(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return !a && !b;
  const left = Date.parse(a), right = Date.parse(b);
  return Number.isFinite(left) && Number.isFinite(right) ? left === right : a === b;
}

// Source, intended delta and queued operation are one canonical snapshot commit.
// The caller supplies exact selected draft context; this layer has no global
// selected-draft state and never chooses an arbitrary shared draft.
function identifierCredentialAccount(token: string | null): string {
  try {
    const parts = token?.split(".");
    if (!parts || parts.length !== 3) throw new Error("opaque credential");
    const payload = JSON.parse(atob(parts[1].replace(/-/g, "+").replace(/_/g, "/"))) as { sub?: unknown };
    if (typeof payload.sub !== "string" || !payload.sub) throw new Error("missing subject");
    // This binds recovery to the credential's identity hint, never its roles or permissions.
    // The server independently validates the actual credential and operation authority.
    return payload.sub;
  } catch {
    throw new Error("This credential cannot bind offline identifier recovery to an account. Sign in with a user session before retaining identifier edits.");
  }
}

function identifierSourcesEqual(left: DocumentIdentifierSource, right: DocumentIdentifierSource): boolean {
  return left.page_updated_at === right.page_updated_at && left.published_at === right.published_at && left.human_identifier === right.human_identifier;
}

function identifierContextsEqual(left: VaultQueuedSyncOperation["server_context"], right: VaultQueuedSyncOperation["server_context"]): boolean {
  return Boolean(left && right && left.server_base === right.server_base && left.account_id === right.account_id && left.session_scope === right.session_scope);
}

function identifierPayloadsEqual(left: unknown, right: unknown): boolean {
  const a = parseDocumentIdentifierIntent(left), b = parseDocumentIdentifierIntent(right);
  return a.id === b.id && a.space_id === b.space_id && a.human_identifier === b.human_identifier && identifierSourcesEqual(a.source, b.source);
}

interface DocumentIdentifierOutcome {
  operation_id: string;
  human_identifier: string | null;
  page_updated_at: string;
  published_at: string | null;
  historical: boolean;
}

async function commitServerDocumentIdentifier(
  spaceId: string,
  docId: string,
  value: unknown,
  expectedSource?: DocumentIdentifierSource,
): Promise<{
  operation_id: string;
  disposition: "queued" | "applied" | "historical";
  pending_error?: string;
}> {
  const display = normalizeHumanIdentifier(value);
  if (!expectedSource)
    throw new Error(
      "Keep the page source captured when identifier editing began; reload before saving without that source",
    );
  const initialToken = api.getToken();
  if (!initialToken)
    throw new Error(
      "Sign in before saving an identifier for this server account",
    );
  const credentialAccount = identifierCredentialAccount(initialToken);
  const serverBase = getApiBase();
  const sessionScope = serverCacheScopeForSession(
    initialToken,
    api.getRefreshToken(),
  );
  const account = await api.fetch<{ id: string }>("/auth/me");
  if (
    getApiBase() !== serverBase ||
    api.getToken() !== initialToken ||
    serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !==
      sessionScope
  )
    throw new Error(
      "Server account changed while resolving the identifier edit. Retain the input and retry in its original account.",
    );
  if (typeof account.id !== "string" || !account.id || account.id !== credentialAccount)
    throw new Error("Current server account is unavailable");
  const context = {
    server_base: serverBase,
    account_id: account.id,
    session_scope: sessionScope,
  };
  const provider = getSyncProvider();
  const path = `/spaces/${spaceId}/documents/${docId}`;
  const page = await api.fetch<DocumentRecord>(path);
  if (
    page.id !== docId ||
    (page.space_id !== undefined && page.space_id !== spaceId) ||
    !page.updated_at
  )
    throw new Error(
      "Download this page's current source before saving its identifier offline",
    );
  if (
    getApiBase() !== serverBase ||
    api.getToken() !== initialToken ||
    serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !==
      sessionScope
  )
    throw new Error(
      "Server account changed while loading the page source. Retain the input and retry.",
    );
  const adapter = createLocalVaultAdapter(localVault.provider);
  await adapter.init();
  if (!(await adapter.supportsAtomicSnapshotCommit()))
    throw new Error(
      "Update this native shell before queuing atomic page identifier changes",
    );
  let operation!: VaultQueuedSyncOperation;
  await adapter.commitSnapshotMutation((snapshot) => {
    snapshot.documents = normalizeDocumentIdentifiers(snapshot.documents);
    const pending = snapshot.sync_state?.queued_operations.find(
      (op) =>
        op.entity_type === "document" &&
        op.entity_id === docId &&
        op.action === "update_identifier" &&
        op.server_context?.server_base === serverBase &&
        op.server_context?.account_id === account.id,
    );
    if (pending) {
      const intent = parseDocumentIdentifierIntent(pending.payload);
      if (
        intent.space_id !== spaceId ||
        intent.human_identifier !== display ||
        !identifierSourcesEqual(intent.source, expectedSource) ||
        pending.replay_provider !== provider ||
        pending.server_context?.session_scope !== sessionScope
      )
        throw new Error(
          "An earlier identifier edit still needs recovery. Retry it before changing this identifier again.",
        );
      operation = pending;
      return { snapshot };
    }
    const payload = parseDocumentIdentifierIntent({
      id: docId,
      space_id: spaceId,
      human_identifier: display,
      source: expectedSource,
    });
    if (
      !metadataRevisionEqual(page.updated_at, payload.source.page_updated_at) ||
      !metadataRevisionEqual(page.published_at, payload.source.published_at) ||
      (page.human_identifier ?? null) !== payload.source.human_identifier
    )
      throw new Error(
        "Page identifier source changed since editing began. Retain the input and review the current page.",
      );
    operation = {
      id: makeId("identifier"),
      queued_at: new Date().toISOString(),
      replay_provider: provider,
      server_context: context,
      entity_type: "document",
      entity_id: docId,
      action: "update_identifier",
      payload,
    };
    return { snapshot, operation };
  });
  const payload = parseDocumentIdentifierIntent(operation.payload);
  let outcome: DocumentIdentifierOutcome;
  try {
    if (
      getApiBase() !== serverBase ||
      api.getToken() !== initialToken ||
      serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !==
        sessionScope
    )
      throw new Error("Server account changed while saving the identifier");
    outcome = await api.fetch<DocumentIdentifierOutcome>(`${path}/identifier`, {
      method: "PATCH",
      body: {
        provider: operation.replay_provider,
        operation_id: operation.id,
        human_identifier: payload.human_identifier,
        source: payload.source,
      },
    });
  } catch (error) {
    await adapter.updateQueuedSyncOperations([
      {
        id: operation.id,
        last_error:
          error instanceof Error
            ? error.message
            : "Identifier request requires recovery",
      },
    ]);
    return {
      operation_id: operation.id,
      disposition: "queued",
      ...(isApiError(error) ? { pending_error: error.message } : {}),
    };
  }
  if (
    outcome.operation_id !== operation.id ||
    typeof outcome.historical !== "boolean" ||
    normalizeHumanIdentifier(outcome.human_identifier) !==
      payload.human_identifier ||
    !Number.isFinite(Date.parse(outcome.page_updated_at)) ||
    !(
      outcome.published_at === null ||
      (typeof outcome.published_at === "string" &&
        Number.isFinite(Date.parse(outcome.published_at)))
    )
  )
    throw new Error(
      "Identifier acknowledgement is unproven; retain its original queued operation for recovery",
    );
  const confirmedAccount = await api.fetch<{ id: string }>("/auth/me");
  if (
    getApiBase() !== serverBase ||
    confirmedAccount.id !== account.id ||
    serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !==
      sessionScope
  )
    throw new Error(
      "Server account changed; retain the identifier operation in its original account",
    );
  await adapter.commitSnapshotMutation((snapshot) => {
    const queued = snapshot.sync_state?.queued_operations.find(
      (op) => op.id === operation.id,
    );
    if (
      !queued ||
      !identifierPayloadsEqual(queued.payload, operation.payload) ||
      !identifierContextsEqual(queued.server_context, context) ||
      queued.replay_provider !== provider
    )
      throw new Error(
        "Identifier recovery record changed before acknowledgement",
      );
    const downloadedForThisAccount = readSharedSpaceDownloadCheckpoints().some(
      (checkpoint) =>
        checkpoint.spaceId === spaceId &&
        checkpoint.downloadedByUserId === account.id &&
        `${checkpoint.sourceServerUrl?.replace(/\/+$/, "")}/api/v1` ===
          serverBase,
    );
    if (!outcome.historical && downloadedForThisAccount) {
      snapshot.documents = normalizeDocumentIdentifiers(
        snapshot.documents.map((doc) =>
          doc.id === docId &&
          doc.space_id === spaceId &&
          (doc.human_identifier ?? null) === payload.source.human_identifier &&
          metadataRevisionEqual(
            doc.updated_at,
            payload.source.page_updated_at,
          ) &&
          metadataRevisionEqual(doc.published_at, payload.source.published_at)
            ? {
                ...doc,
                human_identifier: outcome.human_identifier,
                updated_at: outcome.page_updated_at,
                published_at: outcome.published_at ?? undefined,
              }
            : doc,
        ),
      );
    }
    if (snapshot.sync_state)
      snapshot.sync_state.queued_operations =
        snapshot.sync_state.queued_operations.filter(
          (op) => op.id !== operation.id,
        );
    return { snapshot };
  });
  if (!outcome.historical) {
    const current = readCachedServerResponse<DocumentRecord>(
      serverBase,
      path,
      sessionScope,
    );
    if (
      current &&
      (current.human_identifier ?? null) === payload.source.human_identifier &&
      metadataRevisionEqual(
        current.updated_at,
        payload.source.page_updated_at,
      ) &&
      metadataRevisionEqual(current.published_at, payload.source.published_at)
    )
      cacheServerResponse(
        serverBase,
        path,
        {
          ...current,
          human_identifier: outcome.human_identifier,
          updated_at: outcome.page_updated_at,
          published_at: outcome.published_at ?? undefined,
        },
        sessionScope,
      );
  }
  return {
    operation_id: operation.id,
    disposition: outcome.historical ? "historical" : "applied",
  };
}

async function commitLocalDocumentIdentifier(
  spaceId: string,
  docId: string,
  value: unknown,
  importPayload?: Record<string, unknown>,
  expectedSource?: DocumentIdentifierSource,
): Promise<{ operation_id: string; disposition: "queued" | "imported" }> {
  const display = normalizeHumanIdentifier(value);
  return serializeLocalPersistence(async () => {
    if (!(await localVault.supportsAtomicSnapshotCommit()))
      throw new Error(
        "Update this native shell before queuing atomic page identifier changes",
      );
    const before = localDocs.get(docId);
    const operationId = makeId("identifier");
    const saved = await localVault.commitSnapshotMutation((snapshot) => {
      snapshot.documents = normalizeDocumentIdentifiers(snapshot.documents);
      const source = snapshot.documents.find((page) => page.id === docId);
      if (
        !source ||
        source.space_id !== spaceId ||
        source.archived_at ||
        source.deleted_at ||
        source.locked
      )
        throw new Error("Page is unavailable or locked for identifier changes");
      if (importPayload || expectedSource) {
        const intent = parseDocumentIdentifierIntent(
          importPayload ?? {
            id: docId,
            space_id: spaceId,
            human_identifier: display,
            source: expectedSource,
          },
        );
        if (
          intent.id !== docId ||
          intent.space_id !== spaceId ||
          !metadataRevisionEqual(
            source.updated_at,
            intent.source.page_updated_at,
          ) ||
          !metadataRevisionEqual(
            source.published_at,
            intent.source.published_at,
          ) ||
          (source.human_identifier ?? null) !== intent.source.human_identifier
        )
          throw new Error(
            "Page identifier source changed; retain the pending operation and review it",
          );
      }
      const payload = {
        id: docId,
        space_id: spaceId,
        human_identifier: display,
        source: {
          page_updated_at: source.updated_at,
          published_at: source.published_at ?? null,
          human_identifier: source.human_identifier ?? null,
        },
      };
      const changed = (source.human_identifier ?? null) !== display;
      const next = {
        ...source,
        human_identifier: display,
        ...(changed
          ? {
              updated_at: new Date().toISOString(),
              last_updated_by: LOCAL_SYSTEM_USER,
            }
          : {}),
      };
      snapshot.documents = normalizeDocumentIdentifiers(
        snapshot.documents.map((page) => (page.id === docId ? next : page)),
      );
      return {
        snapshot,
        operation: importPayload
          ? undefined
          : {
              id: operationId,
              replay_provider: getSyncProvider(),
              entity_type: "document",
              entity_id: docId,
              action: "update_identifier",
              payload,
            },
      };
    });
    const committed = saved.documents.find((page) => page.id === docId)!;
    const current = localDocs.get(docId);
    if (!(before && !current)) {
      const next = { ...(current ?? committed) } as LocalDocState;
      for (const field of [
        "human_identifier",
        "updated_at",
        "last_updated_by",
      ] as const) {
        if (before && current && current[field] !== before[field]) continue;
        (next as unknown as Record<string, unknown>)[field] = committed[field];
      }
      localDocs.set(docId, next);
    }
    return {
      operation_id: operationId,
      disposition: importPayload ? "imported" : "queued",
    };
  });
}

interface DocumentMaintenanceOutcome {
  operation_id: string;
  maintenance_user_id: string | null;
  maintenance_group_id: string | null;
  page_updated_at: string;
  published_at: string | null;
  historical: boolean;
}
function maintenanceSourcesEqual(
  a: DocumentMaintenanceSource,
  b: DocumentMaintenanceSource,
): boolean {
  return (
    a.page_updated_at === b.page_updated_at &&
    a.published_at === b.published_at &&
    a.maintenance_user_id === b.maintenance_user_id &&
    a.maintenance_group_id === b.maintenance_group_id
  );
}
function maintenancePayloadsEqual(a: unknown, b: unknown): boolean {
  const x = parseDocumentMaintenanceIntent(a),
    y = parseDocumentMaintenanceIntent(b);
  return (
    x.id === y.id &&
    x.space_id === y.space_id &&
    JSON.stringify(x.maintenance) === JSON.stringify(y.maintenance) &&
    maintenanceSourcesEqual(x.source, y.source)
  );
}
function requireLocalMaintenanceTarget(
  snapshot: LocalVaultSnapshot,
  spaceId: string,
  target: DocumentMaintenance,
): void {
  if (
    target.kind === "unassigned" ||
    (target.kind === "user" && target.user_id === LOCAL_SYSTEM_USER_ID)
  )
    return;
  const catalogs = normalizeDocumentMaintenanceCatalogs(
    snapshot.maintenance_target_catalogs,
  ).filter((record) => record.space_id === spaceId);
  if (catalogs.length !== 1)
    throw new Error(
      "Download this space's exact maintenance target catalog before assigning a person or team offline",
    );
  const catalog = catalogs[0].targets;
  if (
    target.kind === "user" &&
    catalog.users.some((user) => user.id === target.user_id && user.active)
  )
    return;
  if (
    target.kind === "group" &&
    catalog.groups_available &&
    catalog.groups.some(
      (group) =>
        group.id === target.group_id &&
        (group.managed_space_id === null || group.managed_space_id === spaceId),
    )
  )
    return;
  throw new Error(
    "Maintenance target is unavailable; retain the previous assignment and review it",
  );
}
async function commitServerDocumentMaintenance(
  spaceId: string,
  docId: string,
  value: DocumentMaintenance,
  expectedSource?: DocumentMaintenanceSource,
): Promise<{
  operation_id: string;
  disposition: "queued" | "applied" | "historical";
  pending_error?: string;
}> {
  const intended = normalizeDocumentMaintenance(value);
  const fields = maintenanceFields(intended);
  if (!expectedSource)
    throw new Error(
      "Keep the page source captured when maintenance editing began; reload before saving without that source",
    );
  const initialToken = api.getToken();
  if (!initialToken)
    throw new Error("Sign in before saving an maintenance for this server account");
  const credentialAccount = identifierCredentialAccount(initialToken);
  const serverBase = getApiBase();
  const sessionScope = serverCacheScopeForSession(initialToken, api.getRefreshToken());
  const account = await api.fetch<{ id: string }>("/auth/me");
  if (
    getApiBase() !== serverBase ||
    api.getToken() !== initialToken ||
    serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== sessionScope
  )
    throw new Error(
      "Server account changed while resolving the maintenance edit. Retain the input and retry in its original account.",
    );
  if (typeof account.id !== "string" || !account.id || account.id !== credentialAccount)
    throw new Error("Current server account is unavailable");
  const context = {
    server_base: serverBase,
    account_id: account.id,
    session_scope: sessionScope,
  };
  const requireOriginalAccount = () => {
    if (
      api.getToken() !== initialToken ||
      getApiBase() !== serverBase ||
      serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !==
        sessionScope ||
      identifierCredentialAccount(api.getToken()) !== account.id
    )
      throw new Error(
        "Server account changed; retain maintenance recovery in its original account",
      );
  };
  const provider = getSyncProvider();
  const path = `/spaces/${spaceId}/documents/${docId}`;
  const page = await api.fetch<DocumentRecord>(path);
  if (
    page.id !== docId ||
    (page.space_id !== undefined && page.space_id !== spaceId) ||
    !page.updated_at
  )
    throw new Error(
      "Download this page's current source before saving its maintenance offline",
    );
  if (
    getApiBase() !== serverBase ||
    api.getToken() !== initialToken ||
    serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== sessionScope
  )
    throw new Error(
      "Server account changed while loading the page source. Retain the input and retry.",
    );
  const adapter = createLocalVaultAdapter(localVault.provider);
  await adapter.init();
  if (!(await adapter.supportsAtomicSnapshotCommit()))
    throw new Error(
      "Update this native shell before queuing atomic page maintenance changes",
    );
  let operation!: VaultQueuedSyncOperation;
  await adapter.commitSnapshotMutation((snapshot) => {
    requireOriginalAccount();
    snapshot.documents = snapshot.documents.map(normalizeDocumentMaintenanceFields);
    const pending = snapshot.sync_state?.queued_operations.find(
      (op) =>
        op.entity_type === "document" &&
        op.entity_id === docId &&
        op.action === "update_maintenance" &&
        op.server_context?.server_base === serverBase &&
        op.server_context?.account_id === account.id,
    );
    if (pending) {
      const intent = parseDocumentMaintenanceIntent(pending.payload);
      if (
        intent.space_id !== spaceId ||
        JSON.stringify(intent.maintenance) !== JSON.stringify(intended) ||
        !maintenanceSourcesEqual(intent.source, expectedSource) ||
        pending.replay_provider !== provider ||
        pending.server_context?.session_scope !== sessionScope
      )
        throw new Error(
          "An earlier maintenance edit still needs recovery. Retry it before changing this maintenance again.",
        );
      operation = pending;
      return { snapshot };
    }
    const payload = parseDocumentMaintenanceIntent({
      id: docId,
      space_id: spaceId,
      maintenance: intended,
      source: expectedSource,
    });
    if (
      !metadataRevisionEqual(page.updated_at, payload.source.page_updated_at) ||
      !metadataRevisionEqual(page.published_at, payload.source.published_at) ||
      (page.maintenance_user_id ?? null) !== payload.source.maintenance_user_id ||
      (page.maintenance_group_id ?? null) !== payload.source.maintenance_group_id
    )
      throw new Error(
        "Page maintenance source changed since editing began. Retain the input and review the current page.",
      );
    operation = {
      id: makeId("maintenance"),
      queued_at: new Date().toISOString(),
      replay_provider: provider,
      server_context: context,
      entity_type: "document",
      entity_id: docId,
      action: "update_maintenance",
      payload,
    };
    return { snapshot, operation };
  });
  requireOriginalAccount();
  const payload = parseDocumentMaintenanceIntent(operation.payload);
  let outcome: DocumentMaintenanceOutcome;
  try {
    if (
      getApiBase() !== serverBase ||
      api.getToken() !== initialToken ||
      serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== sessionScope
    )
      throw new Error("Server account changed while saving the maintenance");
    outcome = await api.fetch<DocumentMaintenanceOutcome>(`${path}/maintenance`, {
      method: "PATCH",
      body: {
        provider: operation.replay_provider,
        operation_id: operation.id,
        maintenance: payload.maintenance,
        source: payload.source,
      },
    });
  } catch (error) {
    await adapter.updateQueuedSyncOperations([
      {
        id: operation.id,
        last_error:
          error instanceof Error
            ? error.message
            : "Maintenance request requires recovery",
      },
    ]);
    requireOriginalAccount();
    return {
      operation_id: operation.id,
      disposition: "queued",
      ...(isApiError(error) ? { pending_error: error.message } : {}),
    };
  }
  if (
    outcome.operation_id !== operation.id ||
    typeof outcome.historical !== "boolean" ||
    outcome.maintenance_user_id !== fields.maintenance_user_id ||
    outcome.maintenance_group_id !== fields.maintenance_group_id ||
    typeof outcome.page_updated_at !== "string" ||
    !Number.isFinite(Date.parse(outcome.page_updated_at)) ||
    !(
      outcome.published_at === null ||
      (typeof outcome.published_at === "string" &&
        Number.isFinite(Date.parse(outcome.published_at)))
    )
  )
    throw new Error(
      "Maintenance acknowledgement is unproven; retain its original queued operation for recovery",
    );
  const confirmedAccount = await api.fetch<{ id: string }>("/auth/me");
  if (
    getApiBase() !== serverBase ||
    confirmedAccount.id !== account.id ||
    identifierCredentialAccount(api.getToken()) !== account.id ||
    serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== sessionScope
  )
    throw new Error(
      "Server account changed; retain the maintenance operation in its original account",
    );
  await adapter.commitSnapshotMutation((snapshot) => {
    requireOriginalAccount();
    const queued = snapshot.sync_state?.queued_operations.find(
      (op) => op.id === operation.id,
    );
    if (
      !queued ||
      !maintenancePayloadsEqual(queued.payload, operation.payload) ||
      !identifierContextsEqual(queued.server_context, context) ||
      queued.replay_provider !== provider
    )
      throw new Error("Maintenance recovery record changed before acknowledgement");
    const downloadedForThisAccount = readSharedSpaceDownloadCheckpoints().some(
      (checkpoint) =>
        checkpoint.spaceId === spaceId &&
        checkpoint.downloadedByUserId === account.id &&
        `${checkpoint.sourceServerUrl?.replace(/\/+$/, "")}/api/v1` === serverBase,
    );
    if (!outcome.historical && downloadedForThisAccount) {
      snapshot.documents = normalizeDocumentIdentifiers(
        snapshot.documents.map((doc) =>
          doc.id === docId &&
          doc.space_id === spaceId &&
          (doc.maintenance_user_id ?? null) === payload.source.maintenance_user_id &&
          (doc.maintenance_group_id ?? null) === payload.source.maintenance_group_id &&
          metadataRevisionEqual(doc.updated_at, payload.source.page_updated_at) &&
          metadataRevisionEqual(doc.published_at, payload.source.published_at)
            ? {
                ...doc,
                maintenance_user_id: outcome.maintenance_user_id,
                maintenance_group_id: outcome.maintenance_group_id,
                updated_at: outcome.page_updated_at,
                published_at: outcome.published_at ?? undefined,
              }
            : doc,
        ),
      );
    }
    if (snapshot.sync_state)
      snapshot.sync_state.queued_operations =
        snapshot.sync_state.queued_operations.filter((op) => op.id !== operation.id);
    return { snapshot };
  });
  requireOriginalAccount();
  if (
    !outcome.historical &&
    getApiBase() === serverBase &&
    identifierCredentialAccount(api.getToken()) === account.id &&
    serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) === sessionScope
  ) {
    const current = readCachedServerResponse<DocumentRecord>(
      serverBase,
      path,
      sessionScope,
    );
    if (
      current &&
      (current.maintenance_user_id ?? null) === payload.source.maintenance_user_id &&
      (current.maintenance_group_id ?? null) === payload.source.maintenance_group_id &&
      metadataRevisionEqual(current.updated_at, payload.source.page_updated_at) &&
      metadataRevisionEqual(current.published_at, payload.source.published_at)
    )
      cacheServerResponse(
        serverBase,
        path,
        {
          ...current,
          maintenance_user_id: outcome.maintenance_user_id,
          maintenance_group_id: outcome.maintenance_group_id,
          updated_at: outcome.page_updated_at,
          published_at: outcome.published_at ?? undefined,
        },
        sessionScope,
      );
  }
  return {
    operation_id: operation.id,
    disposition: outcome.historical ? "historical" : "applied",
  };
}

async function commitLocalDocumentMaintenance(
  spaceId: string,
  docId: string,
  value: DocumentMaintenance,
  importPayload?: Record<string, unknown>,
  expectedSource?: DocumentMaintenanceSource,
): Promise<{ operation_id: string; disposition: "queued" | "imported" }> {
  const intended = normalizeDocumentMaintenance(value);
  const fields = maintenanceFields(intended);
  return serializeLocalPersistence(async () => {
    if (!(await localVault.supportsAtomicSnapshotCommit()))
      throw new Error(
        "Update this native shell before queuing atomic page maintenance changes",
      );
    const before = localDocs.get(docId);
    const operationId = makeId("maintenance");
    const saved = await localVault.commitSnapshotMutation((snapshot) => {
      snapshot.documents = snapshot.documents.map(normalizeDocumentMaintenanceFields);
      const source = snapshot.documents.find((page) => page.id === docId);
      if (
        !source ||
        source.space_id !== spaceId ||
        source.archived_at ||
        source.deleted_at ||
        source.locked
      )
        throw new Error("Page is unavailable or locked for maintenance changes");
      if (importPayload || expectedSource) {
        const intent = parseDocumentMaintenanceIntent(
          importPayload ?? {
            id: docId,
            space_id: spaceId,
            maintenance: intended,
            source: expectedSource,
          },
        );
        if (
          intent.id !== docId ||
          intent.space_id !== spaceId ||
          !metadataRevisionEqual(source.updated_at, intent.source.page_updated_at) ||
          !metadataRevisionEqual(source.published_at, intent.source.published_at) ||
          (source.maintenance_user_id ?? null) !== intent.source.maintenance_user_id ||
          (source.maintenance_group_id ?? null) !== intent.source.maintenance_group_id
        )
          throw new Error(
            "Page maintenance source changed; retain the pending operation and review it",
          );
      }
      const payload = {
        id: docId,
        space_id: spaceId,
        maintenance: intended,
        source: {
          page_updated_at: source.updated_at,
          published_at: source.published_at ?? null,
          maintenance_user_id: source.maintenance_user_id ?? null,
          maintenance_group_id: source.maintenance_group_id ?? null,
        },
      };
      const changed =
        (source.maintenance_user_id ?? null) !== fields.maintenance_user_id ||
        (source.maintenance_group_id ?? null) !== fields.maintenance_group_id;
      if (changed) requireLocalMaintenanceTarget(snapshot, spaceId, intended);
      const next = {
        ...source,
        ...fields,
        ...(changed
          ? {
              updated_at: new Date().toISOString(),
              last_updated_by: LOCAL_SYSTEM_USER,
            }
          : {}),
      };
      snapshot.documents = normalizeDocumentIdentifiers(
        snapshot.documents.map((page) => (page.id === docId ? next : page)),
      );
      return {
        snapshot,
        operation: importPayload
          ? undefined
          : {
              id: operationId,
              replay_provider: getSyncProvider(),
              entity_type: "document",
              entity_id: docId,
              action: "update_maintenance",
              payload,
            },
      };
    });
    const committed = saved.documents.find((page) => page.id === docId)!;
    const current = localDocs.get(docId);
    if (!(before && !current)) {
      const next = { ...(current ?? committed) } as LocalDocState;
      for (const field of [
        "maintenance_user_id",
        "maintenance_group_id",
        "updated_at",
        "last_updated_by",
      ] as const) {
        if (before && current && current[field] !== before[field]) continue;
        (next as unknown as Record<string, unknown>)[field] = committed[field];
      }
      localDocs.set(docId, next);
    }
    return {
      operation_id: operationId,
      disposition: importPayload ? "imported" : "queued",
    };
  });
}

type LocalLifecycleAction = "trash" | "archive" | "restore" | "unarchive";

/** Patch one latest durable row, with its replay intent in the same canonical commit.
 * Provider import carries its own authority and never queues a local echo. */
async function commitLocalDocumentLifecycle(
  spaceId: string,
  docId: string,
  action: LocalLifecycleAction,
  imported?: Record<string, unknown>,
): Promise<void> {
  const verifySelection = capturePublishedProducerContext();
  const selection = captureDraftSaveSelection();
  const adapter = localVault;
  const verify = () => {
    verifySelection(); verifyDraftSaveSelection(selection);
    if (localVault !== adapter || adapter.provider !== getSyncProvider() || (hydratedLocalProvider !== null && hydratedLocalProvider !== adapter.provider)) throw new Error("Original lifecycle adapter or selected provider changed; reload the original workspace");
  };
  verify();
  await ensureLocalData(); verify();
  requireLocalSpace(spaceId);
  const cachedBefore = requireLocalDoc(spaceId, docId);
  // Preserve the actual optional legacy birth field; cache hydration supplies a
  // display fallback which is not an immutable source witness.
  const original = (await adapter.loadSnapshot()).documents.find(page => page.id === docId && page.space_id === spaceId);
  verify();
  if (!original) throw new Error("Original lifecycle source changed; reload before retrying");
  await serializeLocalPersistence(async () => {
    verify();
    if (!(await adapter.supportsAtomicSnapshotCommit())) throw new Error("Update this native shell before changing page lifecycle atomically");
    verify();
    const saved = await adapter.commitSnapshotMutation(snapshot => {
      verify();
      if (!snapshot.spaces.some(space => space.id === spaceId)) throw new Error("Space not found");
      const source = snapshot.documents.find(page => page.id === docId && page.space_id === spaceId);
      if (!source || source.created_at !== original.created_at) throw new Error("Original lifecycle source changed; reload before retrying");
      if ((source.deleted_at ?? null) !== (cachedBefore.deleted_at ?? null) || (source.archived_at ?? null) !== (cachedBefore.archived_at ?? null)) throw new Error("Page lifecycle changed; reload before retrying");
      if (!imported && ((action === "restore" && !source.deleted_at) || (action !== "restore" && source.deleted_at) || (action === "unarchive" && !source.archived_at))) throw new Error("Page lifecycle is unavailable for this transition");
      const now = new Date().toISOString();
      const stamp = action === "archive" && typeof imported?.archived_at === "string" ? imported.archived_at
        : action === "trash" && typeof imported?.deleted_at === "string" ? imported.deleted_at : now;
      const updated = { ...source, updated_at: now };
      if (action === "trash") { updated.deleted_at = stamp; updated.archived_at = undefined; }
      if (action === "archive") updated.archived_at = stamp;
      if (action === "restore") updated.deleted_at = undefined;
      if (action === "unarchive") updated.archived_at = undefined;
      snapshot.documents = snapshot.documents.map(page => page.id === docId ? updated : page);
      snapshot.search_index = (snapshot.search_index ?? []).map(entry => entry.document_id === docId
        ? { ...entry, title: updated.title, content: updated.content.replace(/\s+/g, " ").trim(), search_text: `${updated.title}\n${updated.content}`.toLowerCase(), updated_at: now } : entry);
      const payload = action === "trash" ? { deleted_at: stamp } : action === "archive" ? { archived_at: stamp } : updated;
      return { snapshot, operation: imported ? undefined : { entity_type: "document", entity_id: docId, action, payload } };
    });
    verify();
    const committed = saved.documents.find(page => page.id === docId && page.space_id === spaceId)!;
    const current = localDocs.get(docId);
    if (current === cachedBefore) {
      localDocs.set(docId, { ...committed, created_by: committed.created_by ?? LOCAL_SYSTEM_USER, last_updated_by: committed.last_updated_by ?? LOCAL_SYSTEM_USER, created_at: committed.created_at ?? committed.updated_at });
      localSearchIndex = [...localSearchIndex.filter(entry => entry.document_id !== docId), ...(saved.search_index ?? []).filter(entry => entry.document_id === docId)];
    } else if (current && current.space_id === spaceId && current.created_at === cachedBefore.created_at) {
      // Another writer can stage cache edits while this commit awaits its ACK.
      // Publish only lifecycle fields whose original cache witness still matches.
      const next = { ...current };
      for (const field of ["deleted_at", "archived_at"] as const) if (current[field] === cachedBefore[field]) next[field] = committed[field];
      if (current.updated_at === cachedBefore.updated_at) next.updated_at = committed.updated_at;
      localDocs.set(docId, next);
    }
  });
}

function requireLocalEditableHierarchyPage(snapshot: LocalVaultSnapshot, page: VaultDocument): void {
  const drafts = (snapshot.drafts ?? []).filter(draft => draft.document_id === page.id && draft.space_id === page.space_id);
  const ownedLegacy = !drafts.length && (!page.created_by || page.created_by === LOCAL_SYSTEM_USER || page.created_by === LOCAL_SYSTEM_USER_ID);
  if (page.locked || page.archived_at || page.deleted_at || (!page.published_at && !ownedLegacy && !drafts.some(draft => draft.owner_id === LOCAL_SYSTEM_USER_ID)))
    throw new Error("Page hierarchy is unavailable or locked");
}

async function commitLocalDocumentMetadata(
  spaceId: string,
  docId: string,
  patch: LocalMetadataPatch,
  draftContext?: LocalMetadataDraftContext,
  importPayload?: Record<string, unknown>,
): Promise<void> {
  await serializeLocalPersistence(async () => {
    if (!(await localVault.supportsAtomicSnapshotCommit()))
      throw new Error("Update this native shell before queuing atomic page metadata changes");
    const fields = Object.keys(patch).sort() as LocalMetadataField[];
    if (!fields.length) return;
    const cachedBefore = new Map(localDocs);
    const cachedDraftsBefore = new Map(localDrafts.map((draft) => [draft.id, draft]));
    let changedIds: string[] = [];
    const documentProjection = new Map<string, (keyof LocalDocState)[]>();
    let changedDraft: string | undefined;
    const saved = await localVault.commitSnapshotMutation((snapshot) => {
      const pages = new Map(snapshot.documents.map((page) => [page.id, page]));
      const source = pages.get(docId);
      if (!source || source.space_id !== spaceId || source.archived_at || source.deleted_at)
        throw new Error("Page is unavailable for queued metadata changes");
      const active = (page: VaultDocument | undefined): VaultDocument => {
        if (!page || page.space_id !== spaceId || page.deleted_at || page.archived_at || page.locked)
          throw new Error("Page hierarchy is unavailable or locked");
        return page;
      };
      active(source);
      if (importPayload) {
        const intent = parseDocumentMetadataIntent(importPayload);
        if (!metadataRevisionEqual(source.updated_at, intent.source.page_updated_at) || !metadataRevisionEqual(source.published_at, intent.source.published_at))
          throw new Error("Page source changed; retain remote metadata operation");
      }
      const rawRevision = importPayload?.updated_at;
      if (rawRevision !== undefined && (typeof rawRevision !== "string" || !Number.isFinite(Date.parse(rawRevision))))
        throw new Error("Remote metadata updated_at must be a valid timestamp");
      const revision = typeof rawRevision === "string" ? new Date(rawRevision).toISOString() : new Date().toISOString();
      const nextFolder = "folder_id" in patch ? patch.folder_id || undefined : source.folder_id;
      const nextParent = "parent_id" in patch ? patch.parent_id || undefined : source.parent_id;
      const moving = nextFolder !== source.folder_id || nextParent !== source.parent_id;
      const ids = new Set<string>([docId]);
      if (moving) {
      if (nextFolder) getFolderAncestors(buildFolderIndex(snapshot.folders), spaceId, nextFolder);
      const index = buildPageIndex(snapshot.documents), budget = createPageTraversalBudget();
      validatePageDestination(index, spaceId, nextFolder, nextParent, docId, budget);
      ids.clear();
      for (const page of getPageSubtree(index, spaceId, docId, budget)) {
        requireLocalEditableHierarchyPage(snapshot, page);
        active(page); ids.add(page.id);
      }
      if (nextParent) for (const page of getPageAncestors(index, spaceId, nextParent, budget)) {
        requireLocalEditableHierarchyPage(snapshot, page); active(page);
      }
      }
      const presentation = fields.some((field) => ["title", "icon", "cover_image"].includes(field));
      let draft: VaultDocumentDraft | undefined;
      if (draftContext) {
        draft = snapshot.drafts?.find((item) => item.id === draftContext.id);
        if (!draft || draft.document_id !== docId || draft.space_id !== spaceId ||
            !metadataRevisionEqual(draft.updated_at, draftContext.updated_at) || !metadataRevisionEqual(draft.base_updated_at, draftContext.base_updated_at))
          throw new Error("Selected draft changed; retain your metadata edit and reload that draft");
      } else if (presentation && (!source.published_at || importPayload && parseDocumentMetadataIntent(importPayload).draft?.mode === "own_latest")) {
        draft = [...(snapshot.drafts ?? [])].filter((item) => item.document_id === docId && item.space_id === spaceId && item.owner_id === LOCAL_SYSTEM_USER_ID)
          .sort((a, b) => b.updated_at.localeCompare(a.updated_at) || b.id.localeCompare(a.id))[0];
        if (!draft) throw new Error("No owned draft is available for this private presentation edit");
      }
      const operationPayload: Record<string, unknown> = {
        id: docId, space_id: spaceId, updated_at: revision,
        metadata_intent: {
          version: 1, fields,
          source: { page_updated_at: source.updated_at, published_at: source.published_at ?? null },
          ...(draftContext ? { draft: { mode: "exact", ...draftContext } } : {}),
        },
      };
      for (const field of fields) operationPayload[field] = patch[field] ?? null;
      parseDocumentMetadataIntent(operationPayload);
      const replacements = new Map<string, VaultDocument>();
      for (const id of moving ? ids : [docId]) {
        const page = pages.get(id)!;
        const next = { ...page };
        if (moving) { next.folder_id = nextFolder; if (id === docId) next.parent_id = nextParent; }
        if (id === docId && !(draft && source.published_at)) {
          if ("title" in patch) next.title = patch.title!;
          if ("icon" in patch) next.icon = patch.icon || undefined;
          if ("cover_image" in patch) next.cover_image = patch.cover_image || undefined;
        }
        if (moving || presentation && !(draft && source.published_at)) { next.updated_at = revision; next.last_updated_by = LOCAL_SYSTEM_USER; }
        const projected: (keyof LocalDocState)[] = [];
        if (moving) { projected.push("folder_id"); if (id === docId) projected.push("parent_id"); }
        if (id === docId && presentation && !(draft && source.published_at)) projected.push(...fields.filter((field) => ["title", "icon", "cover_image"].includes(field)));
        if (moving || presentation && !(draft && source.published_at)) projected.push("updated_at", "last_updated_by");
        documentProjection.set(id, projected);
        replacements.set(id, next);
      }
      snapshot.documents = snapshot.documents.map((page) => replacements.get(page.id) ?? page);
      changedIds = [...replacements.keys()];
      if (draft && presentation) {
        const next = { ...draft, updated_at: revision };
        if ("title" in patch) next.title = patch.title!;
        if ("icon" in patch) next.icon = patch.icon || undefined;
        if ("cover_image" in patch) next.cover_image = patch.cover_image || undefined;
        snapshot.drafts = (snapshot.drafts ?? []).map((item) => item.id === next.id ? next : item);
        changedDraft = next.id;
      }
      return { snapshot, operation: importPayload ? undefined : { entity_type: "document", entity_id: docId, action: "update", payload: operationPayload } };
    });
    for (const id of changedIds) {
      const committed = saved.documents.find((page) => page.id === id)!;
      const before = cachedBefore.get(id), current = localDocs.get(id);
      if (before && !current) continue;
      const next = { ...(current ?? committed) } as LocalDocState;
      for (const field of documentProjection.get(id) ?? []) {
        if (before && current && current[field] !== before[field]) continue;
        (next as unknown as Record<string, unknown>)[field] = committed[field];
      }
      localDocs.set(id, next);
    }
    if (changedDraft) {
      const committed = saved.drafts!.find((draft) => draft.id === changedDraft)!;
      localDrafts = localDrafts.map((draft) => {
        if (draft.id !== changedDraft) return draft;
        const before = cachedDraftsBefore.get(draft.id);
        const next = { ...draft };
        const projected: (keyof VaultDocumentDraft)[] = ["updated_at", ...fields.filter((field): field is "title" | "icon" | "cover_image" => ["title", "icon", "cover_image"].includes(field))];
        for (const field of projected) {
          if (before && draft[field] !== before[field]) continue;
          (next as unknown as Record<string, unknown>)[field] = committed[field];
        }
        return next;
      });
    }
    localSearchIndex = buildLocalSearchIndexEntries();
  });
}

async function persistLocalSnapshot(operation?: LocalQueuedSyncOperationInput, requireOriginalContext?: () => void, adapter: LocalVaultAdapter = localVault): Promise<void> {
  await serializeLocalPersistence(async () => {
    try {
      requireOriginalContext?.();
      if (!isLocalSearchIndexFresh()) localSearchIndex = buildLocalSearchIndexEntries();
      const atomicSnapshot = await adapter.supportsAtomicSnapshotCommit();
      requireOriginalContext?.();
      if (atomicSnapshot) {
        const saved = await adapter.commitSnapshotMutation((committed) => {
          requireOriginalContext?.();
          if (
            operation?.entity_type === "folder" &&
            operation.action === "update" &&
            !committed.folders.some((folder) => folder.id === operation.entity_id)
          )
            throw new Error("Folder was deleted before this update could commit");
          const snapshot = buildSnapshot();
          snapshot.property_schemas = mergePropertyChanges(
            committedPropertySchemas,
            localPropertySchemas,
            committed.property_schemas ?? [],
          );
          snapshot.property_values = mergePropertyChanges(
            committedPropertyValues,
            localPropertyValues,
            committed.property_values ?? [],
          );
          snapshot.maintenance_target_catalogs = committed.maintenance_target_catalogs;
          snapshot.reviewed_move_records = committed.reviewed_move_records;
          snapshot.sync_state = committed.sync_state;
          return {
            snapshot,
            operation: operation
              ? {
                  ...operation,
                  payload: operation.payload ?? findQueuedOperationPayload(snapshot, operation),
                }
              : undefined,
          };
        });
        requireOriginalContext?.();
        publishCommittedProperties(saved);
        return;
      }
      // Older native shells retain their existing ordinary-write compatibility.
      // Property/schema mutations always require the atomic API instead.
      if (
        (operation &&
          ["property_schema", "document_property_values"].includes(operation.entity_type)) ||
        JSON.stringify(localPropertySchemas) !== JSON.stringify(committedPropertySchemas) ||
        JSON.stringify(localPropertyValues) !== JSON.stringify(committedPropertyValues)
      ) {
        throw new Error(
          "This native shell needs an update before atomic property writes are available",
        );
      }
      const snapshot = buildSnapshot();
      if (operation)
        await adapter.enqueueSyncOperation({
          ...operation,
          payload: operation.payload ?? findQueuedOperationPayload(snapshot, operation),
        });
      snapshot.sync_state = (await adapter.loadSnapshot()).sync_state;
      await adapter.saveSnapshot(snapshot);
      publishCommittedProperties(snapshot);
    } catch (cause) {
      // Provider/import code can stage property arrays before requesting persistence.
      // Never expose those tentative values after a rejected canonical commit.
      localPropertySchemas = JSON.parse(JSON.stringify(committedPropertySchemas));
      localPropertyValues = JSON.parse(JSON.stringify(committedPropertyValues));
      throw cause;
    }
  });
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

function normalizeLocalFolderParentId(
  spaceId: string,
  parentId: string | null | undefined,
  movingFolderId?: string,
): string | undefined {
  validateFolderDestination(buildFolderIndex(localFolders.values()), spaceId, parentId, movingFolderId);
  return parentId || undefined;
}

function normalizeLocalDocumentFolderId(spaceId: string, folderId: string | null | undefined): string | undefined {
  if (!folderId) return undefined;
  getFolderAncestors(buildFolderIndex(localFolders.values()), spaceId, folderId);
  return folderId;
}

function normalizeLocalDocumentParentId(spaceId: string, parentId: string | null | undefined, folderId: string | undefined, movingDocumentId?: string): string | undefined {
  validatePageDestination(buildPageIndex(localDocs.values()), spaceId, folderId, parentId, movingDocumentId);
  return parentId || undefined;
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

function normalizeVaultDocument(doc: VaultDocument, hasDraft = false): VaultDocument {
  return {
    ...doc,
    created_by: doc.created_by ?? LOCAL_SYSTEM_USER,
    last_updated_by: doc.last_updated_by ?? LOCAL_SYSTEM_USER,
    created_at: doc.created_at ?? doc.updated_at,
    published_at: doc.published_at ?? (hasDraft ? undefined : doc.updated_at),
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

export function inLocalMode(): boolean {
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
  const normalized = { name, property_type: input.property_type || "text", options: input.options };
  validatePropertySchema(normalized);
  return normalized;
}

function validatePropertyImportIdentity(input: ImportIdentityOptions): void {
  if (input.source_id !== undefined && !/^[A-Za-z0-9_-]{1,128}$/.test(input.source_id.trim()))
    throw new Error(
      "Import source id must contain 1 to 128 letters, numbers, hyphens, or underscores",
    );
  for (const timestamp of [input.created_at, input.updated_at]) {
    if (timestamp === undefined) continue;
    if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(timestamp))
      throw new Error("Import timestamp must be RFC3339");
    validatePropertyValue({ name: "Timestamp", property_type: "datetime" }, timestamp);
  }
}

function validateRecordedPropertyIdentity(
  id: string | undefined,
  metadata: ImportIdentityOptions = {},
): void {
  // Replay IDs are stable legacy identities, not newly supplied REST source IDs.
  if (id !== undefined && (typeof id !== "string" || !id.trim()))
    throw new Error("Property record id must not be empty");
  validatePropertyImportIdentity({
    created_at: metadata.created_at,
    updated_at: metadata.updated_at,
  });
}

function validateLocalPropertyAssignments(
  spaceId: string,
  values: { property_id: string; value: string | null }[],
  schemas = localPropertySchemas,
  documents: VaultDocument[] = Array.from(localDocs.values()),
  drafts = localDrafts,
  enforceLocalIdentity = true,
): void {
  if (values.length > 100) throw new Error("At most 100 property values may be supplied");
  const seen = new Set<string>();
  for (const entry of values) {
    if (seen.has(entry.property_id)) throw new Error("Duplicate property values are not allowed");
    seen.add(entry.property_id);
    const schema = schemas.find(
      (item) => item.id === entry.property_id && item.space_id === spaceId,
    );
    if (!schema) throw new Error("Property binding is not in this space");
    validatePropertyValue(schema, entry.value);
    if (entry.value === null || entry.value === "") continue;
    if (
      enforceLocalIdentity &&
      schema.property_type === "user" &&
      entry.value !== LOCAL_SYSTEM_USER_ID
    )
      throw new Error("User property must reference an active account");
    if (schema.property_type === "relation") {
      const target = documents.find((doc) => doc.id === entry.value && doc.space_id === spaceId);
      const targetDrafts = drafts.filter(
        (draft) => draft.document_id === entry.value && draft.space_id === spaceId,
      );
      // Legacy standalone pages predate published_at. They remain readable locally.
      const available =
        target &&
        !target.archived_at &&
        !target.deleted_at &&
        (!enforceLocalIdentity ||
          target.published_at ||
          (targetDrafts.length === 0 &&
            (!target.created_by ||
              target.created_by === LOCAL_SYSTEM_USER ||
              target.created_by === LOCAL_SYSTEM_USER_ID)) ||
          targetDrafts.some((draft) => draft.owner_id === LOCAL_SYSTEM_USER_ID));
      if (!available) throw new Error("Relation target is unavailable in this space");
    }
  }
}

function validateImportedSnapshotProperties(
  snapshot: LocalVaultSnapshot,
  enforceLocalIdentity = true,
): void {
  const schemas = snapshot.property_schemas ?? [];
  for (const schema of schemas) {
    validatePropertySchema(schema);
    if (enforceLocalIdentity) validateRecordedPropertyIdentity(schema.id);
    else validatePropertyImportIdentity({ source_id: schema.id });
  }
  const batches = new Map<string, { property_id: string; value: string | null }[]>();
  const importedIds = new Set<string>();
  for (const value of snapshot.property_values ?? []) {
    if (enforceLocalIdentity) validateRecordedPropertyIdentity(value.id, value);
    else
      validatePropertyImportIdentity({
        source_id: value.id,
        created_at: value.created_at,
        updated_at: value.updated_at,
      });
    if (importedIds.has(value.id))
      throw new Error("Duplicate property value import ids are not allowed");
    importedIds.add(value.id);
    if (!snapshot.documents.some((doc) => doc.id === value.document_id))
      throw new Error("Property value references a missing document");
    const batch = batches.get(value.document_id) ?? [];
    batch.push(value);
    batches.set(value.document_id, batch);
  }
  for (const [documentId, batch] of batches) {
    const document = snapshot.documents.find((doc) => doc.id === documentId)!;
    validateLocalPropertyAssignments(
      document.space_id,
      batch,
      schemas,
      snapshot.documents,
      snapshot.drafts ?? [],
      enforceLocalIdentity,
    );
  }
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

function capturePublishedProducerContext(): () => void {
  const mode = getCurrentLaunchMode(), provider = getSyncProvider(), selection = JSON.stringify(getStorageSelection());
  const base = getApiBase(), token = api.getToken(), diskToken = localStorage.getItem("access_token");
  return () => {
    if (getCurrentLaunchMode() !== mode || getSyncProvider() !== provider || JSON.stringify(getStorageSelection()) !== selection || getApiBase() !== base || api.getToken() !== token || localStorage.getItem("access_token") !== diskToken) throw new Error("Published edit context changed before persistence; retain your input and review its original workspace.");
  };
}

function publishedReplayJson(value: unknown): string {
  const keyOrder = (a: string, b: string): number => {
    const left = Array.from(a), right = Array.from(b);
    for (let i = 0; i < Math.min(left.length, right.length); i += 1) { const difference = left[i].codePointAt(0)! - right[i].codePointAt(0)!; if (difference) return difference; }
    return left.length - right.length;
  };
  // Emit entries directly: JSON.stringify(object) reorders integer-like keys.
  const encode = (input: unknown): string => {
    if (Array.isArray(input)) return `[${input.map(encode).join(",")}]`;
    if (input && typeof input === "object") return `{${Object.entries(input).sort(([a], [b]) => keyOrder(a, b)).filter(([, child]) => child !== undefined).map(([key, child]) => `${JSON.stringify(key)}:${encode(child)}`).join(",")}}`;
    const encoded = JSON.stringify(input);
    if (typeof encoded !== "string") throw new Error("Replay payload is not a JSON value");
    return encoded;
  };
  return encode(value);
}
function publishedReplayEqual(a: unknown, b: unknown): boolean { return publishedReplayJson(a) === publishedReplayJson(b); }
export async function publishedReplayFingerprint(payload: unknown): Promise<string> {
  const wireValue: unknown = JSON.parse(JSON.stringify(payload));
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(publishedReplayJson(wireValue)));
  return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** Copies immutable source/payload before persistence waits; absent legacy
 * source stays absent and requires explicit review on published server replay. */
function originalPublishedContentOperation(before: LocalDocState, next: LocalDocState, action: string): LocalQueuedSyncOperationInput {
  const original = before.published_source;
  let currentAccount: string | null = null;
  try { currentAccount = identifierCredentialAccount(api.getToken()); } catch { /* unbound retained edit, not authentication */ }
  const source = original && original.schema_version === 1 && typeof original.digest === "string" && /^[0-9a-f]{64}$/.test(original.digest) && original.space_id === before.space_id && original.document_id === before.id
    && original.server_context && original.server_context.server_base === getApiBase()
    && typeof original.server_context.account_id === "string" && original.server_context.account_id.trim().length > 0
    && api.getToken() !== null && localStorage.getItem("access_token") === api.getToken()
    && currentAccount === original.server_context.account_id
    ? JSON.parse(JSON.stringify(original)) as NonNullable<VaultDocument["published_source"]> : null;
  return {
    entity_type: "document", entity_id: before.id, action,
    payload: JSON.parse(JSON.stringify({ ...next, ...(source ? { published_source_digest: source.digest } : {}) })),
    ...(source ? { replay_provider: getSyncProvider(), server_context: { server_base: source.server_context.server_base, account_id: source.server_context.account_id, session_scope: serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) } }
      : { last_error: "Original published server source is unavailable; retain this edit for explicit review before published replay." }),
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
        (typeof payload.description !== "string" ||
          existingSpace.description === payload.description) &&
        (typeof payload.owner_username !== "string" ||
          existingSpace.owner_username === payload.owner_username) &&
        (typeof payload.owner_display_name !== "string" ||
          existingSpace.owner_display_name === payload.owner_display_name) &&
        (typeof payload.created_by !== "string" ||
          existingSpace.created_by === payload.created_by) &&
        (typeof payload.created_at !== "string" ||
          existingSpace.created_at === payload.created_at) &&
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
      description:
        typeof payload.description === "string" ? payload.description : existingSpace?.description,
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
      if (payload?.space_id !== undefined && typeof payload.space_id !== "string") {
        return { status: "failed", message: "Remote folder delete space must be a string" };
      }

      const removed = await commitLocalFolderDeletion(
        payload?.space_id,
        operation.entity_id,
        false,
      );

      return {
        status: removed ? "applied" : "skipped",
        message: removed
          ? "Remote folder delete applied; pages preserved without folder assignments"
          : "Remote folder delete target is already absent locally",
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
        message: `Remote folder update payload space does not match local folder. ${CROSS_SPACE_MOVE_REVIEW_REQUIRED}`,
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

    await commitLocalFolderWrite({
      ...(existingFolder ?? { id: operation.entity_id }),
      space_id: payload.space_id, name, parent_id: parentId,
    }, existingFolder, false);

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
        JSON.stringify(existingTemplate.properties) ===
          JSON.stringify(importedTemplate.properties) &&
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

    const schemaFieldFailure = providerPropertySchemaFieldsFailure(payload);
    if (schemaFieldFailure) {
      return {
        status: "failed",
        message: `Remote property schema ${operation.action} payload ${schemaFieldFailure} is invalid`,
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
        message: "Remote property schema update payload space does not match local property schema",
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

    if (existingSchema)
      validatePropertySchemaChange(
        existingSchema,
        importedSchema,
        localPropertyValues
          .filter((value) => value.property_id === existingSchema.id)
          .map((value) => value.value),
      );
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

    if (existingDoc.archived_at || existingDoc.deleted_at)
      return { status: "failed", message: "Document is archived or deleted" };
    if (existingDoc.space_id !== payload.space_id) {
      return {
        status: "failed",
        message: "Remote document property values payload space does not match local document",
      };
    }

    requireLocalSpace(payload.space_id);
    const importedValues: VaultPropertyValue[] = [];
    const importedPropertyIds = new Set<string>();
    const importedValueIds = new Set<string>();
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
          message: "Remote document property values payload includes duplicate property values",
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

      validateRecordedPropertyIdentity(entry.id, entry);
      if (typeof entry.id === "string") {
        if (importedValueIds.has(entry.id))
          throw new Error("Duplicate property value import ids are not allowed");
        importedValueIds.add(entry.id);
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

    validateLocalPropertyAssignments(payload.space_id, importedValues);
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

  if (["trash", "restore", "archive", "unarchive"].includes(operation.action)) {
    const existing = localDocs.get(operation.entity_id);
    if (!existing) {
      return { status: "skipped", message: "Remote lifecycle target is absent locally" };
    }
    if (payload?.space_id && payload.space_id !== existing.space_id) {
      return {
        status: "failed",
        message: "Remote lifecycle payload space does not match local document",
      };
    }
    await commitLocalDocumentLifecycle(existing.space_id, operation.entity_id, operation.action as LocalLifecycleAction, payload ?? {});
    return {
      status: "applied",
      message: `Remote document ${operation.action} applied to the local vault`,
    };
  }

  if (operation.action === "update_maintenance") {
    try {
      const intent = parseDocumentMaintenanceIntent(payload);
      await commitLocalDocumentMaintenance(intent.space_id,intent.id,intent.maintenance,payload as Record<string,unknown>);
      return { status: "applied", message: "Remote maintenance assignment applied atomically without echo" };
    } catch (error) { return { status: "blocked", message: error instanceof Error ? error.message : "Remote maintenance requires review" }; }
  }
  if (operation.action === "update_identifier") {
    try {
      const intent = parseDocumentIdentifierIntent(payload);
      if (intent.id !== operation.entity_id) throw new Error("Identifier operation page ID differs");
      await commitLocalDocumentIdentifier(intent.space_id, intent.id, intent.human_identifier, payload as Record<string, unknown>);
      return { status: "applied", message: "Remote page identifier applied atomically without queuing an echo" };
    } catch (error) {
      return { status: "blocked", message: error instanceof Error ? error.message : "Remote identifier requires review" };
    }
  }

  if (operation.action === "update") {
    if (payload && "metadata_intent" in payload) {
      try {
        const raw = payload as Record<string, unknown>;
        const intent = parseDocumentMetadataIntent(raw);
        if (raw.id !== undefined && raw.id !== operation.entity_id) throw new Error("Remote metadata ID differs");
        const patch: LocalMetadataPatch = {};
        for (const field of intent.fields) patch[field] = raw[field] as string | null;
        const draft = intent.draft?.mode === "exact" ? intent.draft : undefined;
        await commitLocalDocumentMetadata(raw.space_id as string, operation.entity_id, patch, draft, raw);
        return { status: "applied", message: "Remote explicit metadata intent applied atomically without queuing an echo" };
      } catch (error) {
        return { status: "blocked", message: error instanceof Error ? error.message : "Remote metadata intent requires review" };
      }
    }
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
        message: `Remote document update payload space does not match local document. ${CROSS_SPACE_MOVE_REVIEW_REQUIRED}`,
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
      human_identifier: "human_identifier" in payload ? normalizeHumanIdentifier(payload.human_identifier) : existing.human_identifier,
      maintenance_user_id: "maintenance_user_id" in payload ? normalizeDocumentMaintenanceFields(payload).maintenance_user_id as string | null : existing.maintenance_user_id,
      maintenance_group_id: "maintenance_group_id" in payload ? normalizeDocumentMaintenanceFields(payload).maintenance_group_id as string | null : existing.maintenance_group_id,
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
      published_at:
        typeof payload.published_at === "string" ? payload.published_at : existing.published_at,
    };

    normalizeDocumentMaintenanceFields(updated);
    normalizeDocumentIdentifiers(Array.from(localDocs.values()).map((page) => page.id === updated.id ? updated : page));
    await localVault.saveDocument(updated);
    localDocs.set(operation.entity_id, updated);
    localSearchIndex = buildLocalSearchIndexEntries();

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
        message:
          error instanceof Error ? error.message : "Remote document create content is invalid",
      };
    }
    let folderId: string | undefined, parentId: string | undefined;
    try {
      folderId = normalizeLocalDocumentFolderId(payload.space_id, payload.folder_id);
      parentId = normalizeLocalDocumentParentId(payload.space_id, payload.parent_id, folderId);
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
      human_identifier: "human_identifier" in payload ? normalizeHumanIdentifier(payload.human_identifier) : undefined,
      maintenance_user_id: normalizeDocumentMaintenanceFields(payload).maintenance_user_id as string | null | undefined,
      maintenance_group_id: normalizeDocumentMaintenanceFields(payload).maintenance_group_id as string | null | undefined,
      id: operation.entity_id,
      space_id: payload.space_id,
      title: normalizeTitle(payload.title),
      content,
      icon: typeof payload.icon === "string" ? payload.icon : undefined,
      cover_image: typeof payload.cover_image === "string" ? payload.cover_image : undefined,
      folder_id: folderId,
      parent_id: parentId,
      created_by: typeof payload.created_by === "string" ? payload.created_by : LOCAL_SYSTEM_USER,
      last_updated_by:
        typeof payload.last_updated_by === "string"
          ? payload.last_updated_by
          : typeof payload.created_by === "string"
            ? payload.created_by
            : LOCAL_SYSTEM_USER,
      created_at: typeof payload.created_at === "string" ? payload.created_at : now,
      updated_at: typeof payload.updated_at === "string" ? payload.updated_at : now,
      published_at:
        typeof payload.published_at === "string"
          ? payload.published_at
          : typeof payload.updated_at === "string"
            ? payload.updated_at
            : now,
    };

    const existing = localDocs.get(operation.entity_id);
    if (existing) {
      const matchesExisting =
        existing.space_id === created.space_id &&
        existing.parent_id === created.parent_id &&
        existing.title === created.title &&
        existing.content === created.content &&
        (existing.human_identifier ?? null) === (created.human_identifier ?? null) &&
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

    const selection = captureDraftSaveSelection();
    await serializeLocalPersistence(async () => {
      verifyDraftSaveSelection(selection);
      const saved = await localVault.commitSnapshotMutation(snapshot => {
        verifyDraftSaveSelection(selection);
        if (!snapshot.spaces.some(space => space.id === created.space_id)) throw new Error("Space not found");
        if (snapshot.documents.some(page => page.id === created.id)) throw new Error("Remote document create target changed; reload before retrying");
        if (created.folder_id) getFolderAncestors(buildFolderIndex(snapshot.folders), created.space_id, created.folder_id);
        const index = buildPageIndex(snapshot.documents), budget = createPageTraversalBudget();
        budget.visit(created.id);
        validatePageDestination(index, created.space_id, created.folder_id, created.parent_id, undefined, budget);
        // Provider import preserves its authorized remote graph. Local authoring
        // Edit/lock admission does not apply to imported ancestor records.
        snapshot.documents = normalizeDocumentIdentifiers([...snapshot.documents, created]);
        snapshot.search_index = [...(snapshot.search_index ?? []), { document_id: created.id, space_id: created.space_id, title: created.title, content: created.content.replace(/\s+/g, " ").trim(), search_text: `${created.title}\n${created.content}`.toLowerCase(), updated_at: created.updated_at }];
        return { snapshot };
      });
      verifyDraftSaveSelection(selection);
      localDocs.set(created.id, { ...created });
      localSearchIndex = [...localSearchIndex.filter(row => row.document_id !== created.id), ...(saved.search_index ?? []).filter(row => row.document_id === created.id)];
    });

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
      message: `Remote document content update payload space does not match local document. ${CROSS_SPACE_MOVE_REVIEW_REQUIRED}`,
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
        error instanceof Error
          ? error.message
          : "Remote document content update payload is invalid",
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
    published_at:
      typeof payload.published_at === "string" ? payload.published_at : existing.published_at,
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

export interface LocalReviewedMoveSession {
  readonly preview: LocalMovePreview;
  readonly provider?: "none" | "google-drive";
  readonly provider_status?: "pending_provider";
  verifyCurrent(): void;
  /** Original-source readiness only; final canonical callback must reprove. */
  validate(decisions: LocalReviewedMoveDecisions): void;
  relationTargets(valueId: string): { id: string; title: string }[];
  /** One captured review; caller changes/cancel invalidate its own selection guard. */
  commit(input: { operation_id: string; moved_at: string; decisions: LocalReviewedMoveDecisions }): Promise<{ canonical_committed: true; record: LocalReviewedMoveRecord; newer_local_changes_retained: boolean; provider_status?: "pending_provider" }>;
}

function localMovePageReadable(snapshot: LocalVaultSnapshot, page: VaultDocument | undefined): page is VaultDocument {
  if (!page || page.archived_at || page.deleted_at) return false;
  const drafts = (snapshot.drafts ?? []).filter(row => row.document_id === page.id);
  return !!(page.published_at || drafts.some(row => row.space_id === page.space_id && row.owner_id === LOCAL_SYSTEM_USER_ID) || (!drafts.length && (!page.created_by || page.created_by === LOCAL_SYSTEM_USER_ID || page.created_by === LOCAL_SYSTEM_USER)));
}

/** Original standalone context only, retained in memory; never persist credential bytes. */
function captureStandaloneMoveContext(selectionCurrent: () => boolean): () => void {
  if (!inLocalMode() || getSyncProvider() !== "none" || localVault.provider !== "none") throw new Error("Reviewed standalone moves require the original no-provider local workspace");
  const base = getApiBase(), token = api.getToken(), refresh = api.getRefreshToken();
  const diskToken = localStorage.getItem("access_token"), diskRefresh = localStorage.getItem("refresh_token");
  const selection = JSON.stringify(getStorageSelection());
  const native = canUseTauriNativeVaultArchive();
  return () => {
    if (canUseTauriNativeVaultArchive() !== native || !inLocalMode() || getSyncProvider() !== "none" || JSON.stringify(getStorageSelection()) !== selection || getApiBase() !== base || api.getToken() !== token || api.getRefreshToken() !== refresh || localStorage.getItem("access_token") !== diskToken || localStorage.getItem("refresh_token") !== diskRefresh || !selectionCurrent()) throw new Error("Original move workspace or selection changed; keep this review for its original target");
  };
}

/** Finite move-cache merge: publish canonical relocation, preserving any later B fields. */
function publishLocalReviewedMoveSnapshot(saved: LocalVaultSnapshot, before: LocalVaultSnapshot): boolean {
  const current = JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
  const merged = JSON.parse(JSON.stringify(saved)) as LocalVaultSnapshot;
  let retained = false;
  const collections = ["spaces", "folders", "documents", "drafts", "assets", "templates", "workflow_templates", "document_workflows", "document_workflow_events", "property_schemas", "property_values", "tags", "doc_tags", "search_index"] as const;
  for (const collection of collections) {
    const key = (row: Record<string, unknown>) => collection === "doc_tags" ? JSON.stringify([row.document_id, row.tag_id]) : collection === "search_index" ? String(row.document_id) : String(row.id);
    const rows = (value: LocalVaultSnapshot) => (value[collection] ?? []) as unknown as Record<string, unknown>[];
    const old = new Map(rows(before).map(row => [key(row), row]));
    const now = new Map(rows(current).map(row => [key(row), row]));
    const canonical = new Map(rows(saved).map(row => [key(row), row]));
    for (const id of new Set([...old.keys(), ...now.keys()])) {
      const previous = old.get(id), local = now.get(id);
      if (nativeJsonValueEquals(previous, local)) continue;
      retained = true;
      if (!local) { canonical.delete(id); continue; }
      const committed = canonical.get(id);
      if (!previous || !committed) { canonical.set(id, local); continue; }
      const combined = { ...committed };
      for (const field of new Set([...Object.keys(previous), ...Object.keys(local)])) {
        if (nativeJsonValueEquals(previous[field], local[field])) continue;
        if (Object.prototype.hasOwnProperty.call(local, field)) combined[field] = local[field]; else delete combined[field];
      }
      canonical.set(id, combined);
    }
    (merged as unknown as Record<string, unknown>)[collection] = [...canonical.values()];
  }
  hydrateFromSnapshot(merged);
  return retained;
}

async function guardSelectedBrowserGooglePending(provider: SyncProviderKind, verifyCurrent: () => void): Promise<void> {
  verifyCurrent();
  if (provider !== "google-drive" || canUseTauriNativeVaultArchive() || localVault.provider === provider) return;
  // Legacy ordinary manual sync may use its incumbent application adapter.
  // Refuse any selected Google move before that path changes state or sends HTTP.
  const selected = await createLocalVaultAdapter("google-drive").loadSnapshot();
  verifyCurrent(); requireNoPendingGoogleReviewedMoves(selected.sync_state?.queued_operations ?? []);
}

async function providerStateWithoutReviewedMove(adapter: ReturnType<typeof createLocalVaultAdapter>, state: LocalVaultSyncState, verifyCurrent: () => void): Promise<void> {
  verifyCurrent();
  if (adapter.provider !== "google-drive") { await adapter.setSyncState(state); verifyCurrent(); return; }
  await adapter.commitSnapshotMutation(snapshot => {
    verifyCurrent(); requireNoPendingGoogleReviewedMoves(snapshot.sync_state?.queued_operations ?? []);
    snapshot.sync_state = { ...snapshot.sync_state, state, updated_at: new Date().toISOString(), queued_operations: snapshot.sync_state?.queued_operations ?? [] };
    return { snapshot };
  }); verifyCurrent();
}

async function reviewCapturedMove(intent: LocalMovePreviewIntent, selectionCurrent: () => boolean, google?: GoogleDriveReviewedMoveEnrollment): Promise<LocalReviewedMoveSession> {
    const requireOriginal = google ? google.verifyCurrent : captureStandaloneMoveContext(selectionCurrent);
    const provider = google ? "google-drive" as const : "none" as const;
    if (google && canUseTauriNativeVaultArchive()) throw new Error("Native Google reviewed moves are unavailable; use the browser Google workspace");
    if (google && typeof window.navigator?.locks?.request !== "function") throw new Error("Reviewed Google moves require browser Web Locks");
    requireOriginal();
    if (google && localVault.provider !== provider) throw new Error("Reload the selected Google workspace before reviewing a move");
    // Use the application's selected adapter so canonical commit and ordinary
    // asset readers observe the same admitted committed snapshot/cache.
    const adapter = localVault;
    requireOriginal();
    const originalIntent = JSON.parse(JSON.stringify(intent)) as LocalMovePreviewIntent;
    // Native capture must precede any ordinary init/read/repair of current selection.
    const access = await adapter.openReviewedMoveSession?.(requireOriginal); requireOriginal();
    if (!access) throw new Error("Reviewed moves require the original transactional workspace");
    if (!canUseTauriNativeVaultArchive()) { await ensureLocalData(); requireOriginal(); }
    if (hydratedLocalProvider !== null && hydratedLocalProvider !== provider) throw new Error("Reload the standalone workspace before reviewing a move");
    const snapshot = JSON.parse(JSON.stringify(await access.loadSnapshot())) as LocalVaultSnapshot; requireOriginal();
    if (!localInitialized) { hydrateFromSnapshot(snapshot); hydratedLocalProvider = provider; localInitialized = true; }
    const preview = buildLocalMovePreview(snapshot, originalIntent, LOCAL_SYSTEM_USER_ID);
    const accepted = JSON.parse(JSON.stringify(preview)) as LocalMovePreview;
    let validationIndex = 0;
    let validationId = "00000000-0000-0000-0000-000000000000";
    while (snapshot.reviewed_move_records?.some(row => row.id === validationId)) validationId = `00000000-0000-0000-0000-${(++validationIndex).toString(16).padStart(12, "0")}`;
    let dispatchedInput: { operation_id: string; moved_at: string; decisions: LocalReviewedMoveDecisions } | undefined;
    let dispatchedBeforeCache: LocalVaultSnapshot | undefined;
    let dispatchedExpectedRecord: LocalReviewedMoveRecord | undefined;
    let dispatchedQueueOperation: VaultQueuedSyncOperation | undefined;
    let lastPublishedCanonical: LocalVaultSnapshot | undefined;
    const publishAcknowledgedSnapshot = (canonical: LocalVaultSnapshot) => {
      const retained = publishLocalReviewedMoveSnapshot(canonical, lastPublishedCanonical ?? dispatchedBeforeCache!);
      // Compare future RAM changes with our own last canonical publication, not
      // pre-move placement and not the merged RAM containing genuine pending edits.
      lastPublishedCanonical = JSON.parse(JSON.stringify(canonical)) as LocalVaultSnapshot;
      return retained;
    };
    const matchesOriginalRecord = (record: LocalReviewedMoveRecord, input: NonNullable<typeof dispatchedInput>) =>
      record.schema_version === (google ? 2 : 1) && record.id === input.operation_id && record.provider === provider &&
      record.actor_id === LOCAL_SYSTEM_USER_ID && record.moved_at === input.moved_at &&
      nativeJsonValueEquals(record.intent, originalIntent) && nativeJsonValueEquals(record.decisions, input.decisions) &&
      !!dispatchedExpectedRecord && nativeJsonValueEquals(record, dispatchedExpectedRecord);
    const matchesQueue = (canonical: LocalVaultSnapshot) => !google || (!!dispatchedQueueOperation && (canonical.sync_state?.queued_operations ?? []).some(row => nativeJsonValueEquals(row, dispatchedQueueOperation)));
    const preparedMove = (canonical: LocalVaultSnapshot, input: Parameters<typeof prepareLocalReviewedMove>[2]) => google ? prepareGoogleReviewedMove(canonical, accepted, input, google.binding) : prepareLocalReviewedMove(canonical, accepted, input);
    return {
      provider, ...(google ? { provider_status: "pending_provider" as const } : {}),
      preview: JSON.parse(JSON.stringify(preview)) as LocalMovePreview,
      verifyCurrent: requireOriginal,
      validate: (decisions) => {
        requireOriginal();
        preparedMove(snapshot, { operation_id: validationId, actor_id: LOCAL_SYSTEM_USER_ID, moved_at: "2000-01-01T00:00:00Z", decisions });
      },
      relationTargets: (valueId) => {
        requireOriginal();
        const edge = accepted.inventory.external_relations.find(row => row.value.id === valueId);
        if (!edge) throw new Error("Relation review is unavailable");
        const moved = new Set(accepted.inventory.documents.map(row => row.id));
        const targetSpace = edge.direction === "outgoing" ? accepted.intent.destination.space_id : snapshot.documents.find(row => row.id === edge.value.document_id)?.space_id;
        return snapshot.documents.filter(row => localMovePageReadable(snapshot, row) && (moved.has(row.id) ? accepted.intent.destination.space_id : row.space_id) === targetSpace).map(row => ({ id: row.id, title: row.title }));
      },
      commit: async (input) => {
        requireOriginal();
        const copiedInput = JSON.parse(JSON.stringify(input)) as typeof input;
        if (dispatchedInput && !nativeJsonValueEquals(dispatchedInput, copiedInput)) throw new Error("Retain the original move request for review; retry cannot change its identity or decisions");
        const commitOriginal = () => serializeLocalPersistence(async () => {
          requireOriginal();
          if (dispatchedInput && !nativeJsonValueEquals(dispatchedInput, copiedInput)) throw new Error("Retain the original move request for review; retry cannot change its identity or decisions");
          const current = await access.loadSnapshot(); requireOriginal();
          assertReviewedMoveRecords(current.reviewed_move_records);
          const historical = current.reviewed_move_records?.find(row => row.id === copiedInput.operation_id);
          if (historical) {
            if (!dispatchedInput || !dispatchedBeforeCache || (!matchesOriginalRecord(historical, dispatchedInput) || !matchesQueue(current))) throw new Error("Move operation evidence does not match the original dispatched request");
            // Exact retained receipt, not a newly enrolled source or latest-body ACK.
            const newer = publishAcknowledgedSnapshot(current);
            return { canonical_committed: true as const, record: JSON.parse(JSON.stringify(historical)) as LocalReviewedMoveRecord, newer_local_changes_retained: newer, ...(google ? { provider_status: "pending_provider" as const } : {}) };
          }
          if (!dispatchedInput) {
            dispatchedInput = copiedInput;
            dispatchedBeforeCache = JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
          }
          const originalInput = dispatchedInput;
          const saved = await access.commitSnapshotMutation((committed) => {
            requireOriginal();
            const prepared = preparedMove(committed, { ...originalInput, actor_id: LOCAL_SYSTEM_USER_ID });
            // Pin all original evidence before the actual canonical write. No
            // later historical row may enroll a replacement receipt baseline.
            if (dispatchedExpectedRecord && !nativeJsonValueEquals(dispatchedExpectedRecord, prepared.record)) throw new Error("Move operation evidence does not match the original dispatched request");
            dispatchedExpectedRecord ??= JSON.parse(JSON.stringify(prepared.record)) as LocalReviewedMoveRecord;
            if (google && "envelope" in prepared) {
              const operation: VaultQueuedSyncOperation = { id: originalInput.operation_id, entity_type: "reviewed_move", entity_id: originalIntent.root.id, action: "commit", queued_at: originalInput.moved_at, attempts: 0, replay_provider: "google-drive", payload: prepared.envelope };
              if ((committed.sync_state?.queued_operations ?? []).some(row => row.id === operation.id)) throw new Error("Move operation collides with retained queue evidence");
              if (dispatchedQueueOperation && !nativeJsonValueEquals(dispatchedQueueOperation, operation)) throw new Error("Original Google move envelope changed; retain it for review");
              dispatchedQueueOperation ??= JSON.parse(JSON.stringify(operation)) as VaultQueuedSyncOperation;
              return { snapshot: prepared.snapshot, operation: dispatchedQueueOperation };
            }
            return { snapshot: prepared.snapshot }; // Standalone has no provider replay operation.
          });
          requireOriginal();
          const record = saved.reviewed_move_records?.find(row => row.id === originalInput.operation_id);
          if (!record || (!matchesOriginalRecord(record, originalInput) || !matchesQueue(saved))) throw new Error("Move acknowledgment is unavailable; inspect the retained original operation before retrying");
          const newer = publishAcknowledgedSnapshot(saved);
          return { canonical_committed: true as const, record: JSON.parse(JSON.stringify(record)) as LocalReviewedMoveRecord, newer_local_changes_retained: newer, ...(google ? { provider_status: "pending_provider" as const } : {}) };
        });
        return google ? withGoogleReviewedMovePublicationLock(requireOriginal, commitOriginal, true) : commitOriginal();
      },
    };
}

export const dataClient = {
  /** Actual dedicated enrollment for the planned connected move; availability remains gated. */
  async captureGoogleDriveMoveEnrollment(selectionCurrent: () => boolean): Promise<GoogleDriveReviewedMoveEnrollment> {
    if (!inLocalMode() || getSyncProvider() !== "google-drive") throw new Error("Google Drive move enrollment requires the original local Google workspace");
    const selection = JSON.stringify(getStorageSelection()), mode = getCurrentLaunchMode();
    const base = getApiBase(), token = api.getToken(), refresh = api.getRefreshToken();
    const diskToken = localStorage.getItem("access_token"), diskRefresh = localStorage.getItem("refresh_token");
    const native = canUseTauriNativeVaultArchive();
    const verifyOriginal = () => {
      if (!inLocalMode() || getCurrentLaunchMode() !== mode || JSON.stringify(getStorageSelection()) !== selection ||
        getApiBase() !== base || api.getToken() !== token || api.getRefreshToken() !== refresh ||
        localStorage.getItem("access_token") !== diskToken || localStorage.getItem("refresh_token") !== diskRefresh ||
        canUseTauriNativeVaultArchive() !== native || !selectionCurrent()) throw new Error("Original Google Drive move workspace or selection changed; retain it for review");
    };
    verifyOriginal();
    const enrolled = await captureGoogleDriveReviewedMoveEnrollment({ selectionCurrent: () => { verifyOriginal(); return true; } });
    verifyOriginal(); enrolled.verifyCurrent();
    return { binding: enrolled.binding, verifyCurrent: () => { verifyOriginal(); enrolled.verifyCurrent(); } };
  },
  /** Standalone and connected histories remain explicitly distinct. */
  async reviewLocalMove(intent: LocalMovePreviewIntent, selectionCurrent: () => boolean): Promise<LocalReviewedMoveSession> {
    return reviewCapturedMove(intent, selectionCurrent);
  },
  async reviewGoogleDriveMove(intent: LocalMovePreviewIntent, selectionCurrent: () => boolean): Promise<LocalReviewedMoveSession> {
    if (canUseTauriNativeVaultArchive()) throw new Error("Native Google reviewed moves are unavailable; use the browser Google workspace");
    const enrollment = await this.captureGoogleDriveMoveEnrollment(selectionCurrent);
    enrollment.verifyCurrent();
    return reviewCapturedMove(intent, selectionCurrent, enrollment);
  },

  /** Reloadable local evidence, current standalone page/participant eligibility, no restoration. */
  async inspectLocalReviewedMoveRecords(spaceId: string, documentId: string, selectionCurrent: () => boolean): Promise<LocalReviewedMoveRecord[]> {
    if (getSyncProvider() === "google-drive" && canUseTauriNativeVaultArchive()) throw new Error("Native Google move evidence inspection is unavailable");
    const google = getSyncProvider() === "google-drive" ? await this.captureGoogleDriveMoveEnrollment(selectionCurrent) : undefined;
    const requireOriginal = google ? google.verifyCurrent : captureStandaloneMoveContext(selectionCurrent); requireOriginal();
    if (google && canUseTauriNativeVaultArchive()) throw new Error("Native Google move evidence inspection is unavailable");
    const adapter = createLocalVaultAdapter(google ? "google-drive" : "none");
    const access = await adapter.openReviewedMoveSession?.(requireOriginal); requireOriginal();
    if (!access) throw new Error("Local move evidence inspection requires the original transactional standalone vault");
    const snapshot = await access.loadSnapshot(); requireOriginal();
    const selected = snapshot.documents.find(row => row.id === documentId && row.space_id === spaceId);
    if (!localMovePageReadable(snapshot, selected)) throw new Error("Local move evidence is unavailable");
    assertReviewedMoveRecords(snapshot.reviewed_move_records);
    const records = (snapshot.reviewed_move_records ?? []).filter(record => record.actor_id === LOCAL_SYSTEM_USER_ID && (google ? record.schema_version === 2 && record.provider === "google-drive" && nativeJsonValueEquals(record.origin, google.binding) : record.schema_version === 1 && record.provider === "none") && (record.before.documents ?? []).some(row => !!row && typeof row === "object" && (row as { id?: unknown }).id === documentId));
    for (const record of records) {
      if (record.schema_version === 2) {
        const operations = snapshot.sync_state?.queued_operations ?? [];
        const pending = operations.filter(row => row.id === record.id);
        const payload = pending[0]?.payload as { record?: unknown; origin?: unknown; kind?: unknown } | undefined;
        if (pending.length !== 1 || pending[0].entity_type !== "reviewed_move" || pending[0].action !== "commit" || pending[0].replay_provider !== "google-drive" || payload?.kind !== "reviewed_move" || !nativeJsonValueEquals(payload.record, record) || !nativeJsonValueEquals(payload.origin, record.origin)) throw new Error("Original Google move provider request is unavailable; retain its history for review");
      }

      const inventories: Partial<Record<string, unknown[]>>[] = [record.before, record.after,
        ...record.preserved.map(item => ({ [item.collection]: item.records })),
        ...record.removed.map(item => ({ [item.collection]: item.records }))];
      const schemaTypes = new Map<string, string>();
      for (const inventory of inventories) for (const raw of inventory.property_schemas ?? []) {
        if (!raw || typeof raw !== "object") throw new Error("Local move evidence is unavailable");
        const schema = raw as Record<string, unknown>;
        if (typeof schema.id !== "string" || typeof schema.property_type !== "string" ||
          (schemaTypes.has(schema.id) && schemaTypes.get(schema.id) !== schema.property_type)) throw new Error("Local move evidence is unavailable");
        schemaTypes.set(schema.id, schema.property_type);
      }
      for (const inventory of inventories) {
        for (const [collection, rows] of Object.entries(inventory)) for (const raw of rows ?? []) {
          if (!raw || typeof raw !== "object") throw new Error("Local move evidence is unavailable");
          const row = raw as Record<string, unknown>;
          const owner = collection === "documents" ? row.id : row.document_id;
          if (typeof owner === "string" && !localMovePageReadable(snapshot, snapshot.documents.find(page => page.id === owner))) throw new Error("Local move evidence is unavailable");
          if (collection === "drafts" && row.owner_id !== LOCAL_SYSTEM_USER_ID) throw new Error("Local move evidence is unavailable");
          if (collection === "property_values") {
            // Legacy evidence without its original definition is unavailable, never
            // repaired from a mutable current schema or silently treated as text.
            const type = typeof row.property_id === "string" ? schemaTypes.get(row.property_id) : undefined;
            if (!type) throw new Error("Local move evidence is unavailable");
            if (type === "relation" && row.value !== null && row.value !== "" &&
              (typeof row.value !== "string" || !localMovePageReadable(snapshot, snapshot.documents.find(page => page.id === row.value)))) throw new Error("Local move evidence is unavailable");
          }
        }
      }
    }
    requireOriginal();
    return JSON.parse(JSON.stringify(records)) as LocalReviewedMoveRecord[];
  },

  inspectDocumentAccess,
  inspectPageRestrictions,
  previewPageRestrictions,
  applyPageRestrictions,
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
    return [
      {
        user_id: LOCAL_SYSTEM_USER_ID,
        display_name: LOCAL_SYSTEM_USER,
        email: "local@starlane",
        role: "owner",
      },
    ];
  },

  async listSpaces(): Promise<SpaceRecord[]> {
    if (!inLocalMode()) {
      const loaded = await api.fetch<SpaceRecord[]>("/spaces");
      if (
        loaded.length > 0 ||
        typeof window === "undefined" ||
        !("__TAURI_INTERNALS__" in window)
      ) {
        return loaded;
      }

      // A transient server/auth hydration failure must not hide a durable
      // native workspace that was already opened on this device.
      const { loadServerVaultSnapshot } = await import("@/lib/serverVaultMirror");
      const snapshot = await loadServerVaultSnapshot();
      if (!snapshot || snapshot.spaces.length === 0) return loaded;
      return snapshot.spaces.map((space) => ({
        id: space.id,
        name: space.name,
        slug: space.slug || space.id,
        description: space.description,
        member_role: "owner" as const,
        owner_username: space.owner_username,
        owner_display_name: space.owner_display_name,
        created_by: space.created_by,
      }));
    }
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
        body: { name, visibility: "internal", ...importOptions },
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

  async updateSpace(
    spaceId: string,
    patch: { name?: string; description?: string },
  ): Promise<SpaceRecord> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}`, {
        method: "PUT",
        body: patch,
      });
      return this.getSpace(spaceId);
    }
    await ensureLocalData();
    const space = requireLocalSpace(spaceId);
    const normalizedName = patch.name === undefined ? space.name : patch.name.trim();
    if (!normalizedName) throw new Error("Space name is required");
    const normalizedDescription =
      patch.description === undefined ? space.description : patch.description.trim();
    if ((normalizedDescription?.length ?? 0) > 1000) {
      throw new Error("Space description must be at most 1000 characters");
    }
    const now = new Date().toISOString();
    const slug = normalizedName
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9-]/g, "");
    const updated = {
      ...space,
      name: normalizedName,
      slug: slug || space.slug || space.id,
      description: normalizedDescription,
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
      .map((f) => ({ id: f.id, name: f.name, icon: f.icon, parent_id: f.parent_id }));
  },

  async createFolder(
    spaceId: string,
    name: string,
    options: { parent_id?: string | null; icon?: string } & ImportIdentityOptions = {},
  ): Promise<FolderRecord> {
    if (!inLocalMode()) {
      const sourceId = options.source_id ?? makeId("folder");
      const created = await api.fetch<FolderRecord>(`/spaces/${spaceId}/folders`, {
        method: "POST",
        body: {
          name,
          parent_id: options.parent_id ?? null,
          ...(options.icon?.trim() ? { icon: options.icon.trim() } : {}),
          source_id: sourceId,
          created_at: options.created_at,
          updated_at: options.updated_at,
        },
      });
      return (
        created ?? {
          id: sourceId,
          name,
          icon: options.icon?.trim() || undefined,
          parent_id: options.parent_id ?? undefined,
        }
      );
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const normalizedName = name.trim();
    if (!normalizedName) throw new Error("Folder name is required");
    const parentId = options.parent_id || undefined;
    const id = options.source_id ?? makeId("folder");
    if (localFolders.has(id)) throw new Error("A folder with this import id already exists");
    await commitLocalFolderWrite({
      id, space_id: spaceId, name: normalizedName,
      icon: options.icon?.trim() || undefined, parent_id: parentId,
    }, undefined, true);
    return {
      id,
      name: normalizedName,
      icon: options.icon?.trim() || undefined,
      parent_id: parentId,
    };
  },

  async updateFolder(
    spaceId: string,
    folderId: string,
    patch: { name?: string; icon?: string; parent_id?: string | null },
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
      ? patch.parent_id || undefined
      : folder.parent_id;
    const nextIcon = Object.prototype.hasOwnProperty.call(patch, "icon")
      ? patch.icon?.trim() || undefined
      : folder.icon;
    await commitLocalFolderWrite({ ...folder, name: nextName, icon: nextIcon, parent_id: nextParentId }, folder, true);
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
    if (input.target_space_id !== spaceId) {
      throw new Error(CROSS_SPACE_MOVE_REVIEW_REQUIRED);
    }
    await commitLocalFolderWrite({ ...folder, parent_id: input.parent_id || undefined }, folder, true);
  },

  async deleteFolder(spaceId: string, folderId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/folders/${folderId}`, { method: "DELETE" });
      return;
    }
    await ensureLocalData();
    await commitLocalFolderDeletion(spaceId, folderId, true);
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
      .map((document) => ({
        document,
        draft:
          document.published_at === undefined
            ? localDrafts
                .filter((draft) => draft.document_id === document.id)
                .sort((left, right) => right.updated_at.localeCompare(left.updated_at))[0]
            : undefined,
      }))
      .filter(({ document, draft }) => Boolean(document.published_at) || Boolean(draft))
      .filter(({ document }) => !document.archived_at && !document.deleted_at)
      .filter(({ document }) => documentMatchesListOptions(document, options))
      .sort(({ document: left }, { document: right }) =>
        left.updated_at < right.updated_at ? 1 : -1,
      )
      .map(({ document, draft }) => ({
        id: document.id,
        human_identifier: document.human_identifier,
        maintenance_user_id: document.maintenance_user_id,
        maintenance_group_id: document.maintenance_group_id,
        title: draft?.title ?? document.title,
        icon: draft?.icon ?? document.icon,
        folder_id: document.folder_id,
        parent_id: document.parent_id,
        published_at: document.published_at,
        draft_id: draft?.id,
        ...workflowSummaryForDocument(document.id),
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
      human_identifier?: string | null;
  maintenance_user_id?: string | null;
  maintenance_group_id?: string | null;
      property_values?: { property_id: string; value: string }[];
      icon?: string;
      cover_image?: string;
    } & ImportIdentityOptions,
    // Local duplication also participates in its original source page. This
    // internal admission context is never serialized into a provider request.
    localCopySourceId?: string,
  ): Promise<{ id: string; draft_id?: string; draft_source_digest?: string }> {
    const identifier = input.human_identifier === undefined ? undefined : normalizeHumanIdentifier(input.human_identifier);
    const responsibility = normalizeDocumentMaintenanceFields(input);
    if (!inLocalMode()) {
      const normalizedTitle = normalizeTitle(input.title);
      const normalizedContent = input.content ?? DEFAULT_NEW_DOCUMENT_CONTENT;
      assertContentSize(normalizedContent);
      const sourceId = input.source_id ?? makeId("doc");
      let created: { id: string; draft_id?: string; draft_source_digest?: string };
      try {
        created = await api.fetch<{ id: string; draft_id?: string; draft_source_digest?: string }>(
          `/spaces/${spaceId}/documents`,
          {
            method: "POST",
            body: {
              title: normalizedTitle,
              folder_id: input.folder_id ?? null,
              parent_id: input.parent_id ?? null,
              icon: input.icon ?? null,
              cover_image: input.cover_image ?? null,
              content: normalizedContent,
              template_id: input.template_id,
              property_values: input.property_values,
              human_identifier: identifier,
              maintenance_user_id: responsibility.maintenance_user_id,
              maintenance_group_id: responsibility.maintenance_group_id,
              source_id: sourceId,
              created_at: input.created_at,
              updated_at: input.updated_at,
            },
          },
        );
      } catch (cause) {
        // A stable operation ID lets a timeout retry recover the committed page.
        // Never retry a POST with a new identity or overwrite the existing page.
        const existing = await this.getDocument(spaceId, sourceId).catch(() => null);
        if (!existing || existing.title !== normalizedTitle || identifier !== undefined && (existing.human_identifier ?? null) !== identifier || responsibility.maintenance_user_id !== undefined && (existing?.maintenance_user_id ?? null) !== responsibility.maintenance_user_id || responsibility.maintenance_group_id !== undefined && (existing?.maintenance_group_id ?? null) !== responsibility.maintenance_group_id) throw cause;
        return { id: existing.id, draft_id: existing.draft_id };
      }
      return created ?? { id: sourceId };
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    const normalizedTitle = normalizeTitle(input.title);
    const normalizedContent = input.content ?? DEFAULT_NEW_DOCUMENT_CONTENT;
    assertContentSize(normalizedContent);
    const folderId = normalizeLocalDocumentFolderId(spaceId, input.folder_id);
    const parentId = normalizeLocalDocumentParentId(spaceId, input.parent_id, folderId);
    const id = input.source_id ?? makeId("doc");
    assertValidEntityId("document", id);
    if (localDocs.has(id)) throw new Error("A document with this import id already exists");
    normalizeDocumentIdentifiers([...Array.from(localDocs.values()), { id, space_id: spaceId, human_identifier: identifier }]);
    validateLocalPropertyAssignments(spaceId, input.property_values ?? []);
    const now = new Date().toISOString();
    const template = input.template_id
      ? localTemplateCache.find(
          (item) => item.id === input.template_id && item.space_id === spaceId,
        )
      : undefined;
    if (input.template_id && !template) throw new Error("Template not found");
    let templateTagIds: string[] = [];
    let templatePropertyValues: VaultPropertyValue[] = [];
    if (template) {
      const tagNames = new Set((template.tags ?? []).map((tag) => tag.trim().toLowerCase()));
      templateTagIds = localTags
        .filter((tag) => tag.space_id === spaceId && tagNames.has(tag.name.trim().toLowerCase()))
        .map((tag) => tag.id);

      const values =
        template.properties &&
        typeof template.properties === "object" &&
        !Array.isArray(template.properties)
          ? Object.entries(template.properties as Record<string, unknown>)
          : [];
      for (const [name, value] of values) {
        if (name.startsWith("__starlane_") || value === null) continue;
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
    }
    for (const assignment of input.property_values ?? []) {
      templatePropertyValues = templatePropertyValues.filter(
        (value) => value.property_id !== assignment.property_id,
      );
      templatePropertyValues.push({
        id: makeId("propval"),
        document_id: id,
        property_id: assignment.property_id,
        value: assignment.value,
        created_at: now,
        updated_at: now,
      });
    }
    validateLocalPropertyAssignments(spaceId, templatePropertyValues);
    const page: LocalDocState = {
      id,
      human_identifier: identifier,
      maintenance_user_id: responsibility.maintenance_user_id,
      maintenance_group_id: responsibility.maintenance_group_id,
      space_id: spaceId,
      title: normalizedTitle,
      content: normalizedContent,
      icon: input.icon,
      cover_image: input.cover_image,
      folder_id: folderId,
      parent_id: parentId,
      created_by: LOCAL_SYSTEM_USER,
      last_updated_by: LOCAL_SYSTEM_USER,
      created_at: now,
      updated_at: now,
    };
    const draftId = makeId("draft");
    const draft: VaultDocumentDraft = {
      id: draftId,
      document_id: id,
      space_id: spaceId,
      owner_id: LOCAL_SYSTEM_USER_ID,
      title: normalizedTitle,
      content: normalizedContent,
      icon: input.icon,
      cover_image: input.cover_image,
      base_updated_at: now,
      created_at: now,
      updated_at: now,
    };
    const selection = captureDraftSaveSelection();
    const parentProof = parentId ? getPageAncestors(buildPageIndex(localDocs.values()), spaceId, parentId).map(parent => ({ id: parent.id, parent_id: parent.parent_id, folder_id: parent.folder_id })) : [];
    const queuedAt = Date.now();
    const operations: VaultQueuedSyncOperation[] = [{ id: makeId("syncop"), entity_type: "document", entity_id: id, action: "create", queued_at: new Date(queuedAt).toISOString(), attempts: 0, payload: page }];
    for (const [index, tagId] of templateTagIds.entries()) operations.push({ id: makeId("syncop"), entity_type: "document_tag", entity_id: `${id}:${tagId}`, action: "add", queued_at: new Date(queuedAt + index + 1).toISOString(), attempts: 0, payload: { document_id: id, tag_id: tagId, space_id: spaceId } });
    if (templatePropertyValues.length) operations.push({ id: makeId("syncop"), entity_type: "document_property_values", entity_id: id, action: "set", queued_at: new Date(queuedAt + templateTagIds.length + 1).toISOString(), attempts: 0, payload: { document_id: id, space_id: spaceId, values: templatePropertyValues } });
    await serializeLocalPersistence(async () => {
      verifyDraftSaveSelection(selection);
      if (!(await localVault.supportsAtomicSnapshotCommit())) throw new Error("Update this native shell before creating pages atomically");
      verifyDraftSaveSelection(selection);
      const saved = await localVault.commitSnapshotMutation(snapshot => {
        verifyDraftSaveSelection(selection);
        if (!snapshot.spaces.some(space => space.id === spaceId)) throw new Error("Space not found");
        if (snapshot.documents.some(row => row.id === id)) throw new Error("A document with this import id already exists");
        if (folderId) getFolderAncestors(buildFolderIndex(snapshot.folders), spaceId, folderId);
        const index = buildPageIndex(snapshot.documents), budget = createPageTraversalBudget();
        budget.visit(id);
        if (localCopySourceId) budget.visit(localCopySourceId);
        validatePageDestination(index, spaceId, folderId, parentId, undefined, budget);
        const actualParents = parentId ? getPageAncestors(index, spaceId, parentId, budget) : [];
        if (JSON.stringify(actualParents.map(parent => ({ id: parent.id, parent_id: parent.parent_id, folder_id: parent.folder_id }))) !== JSON.stringify(parentProof)) throw new Error("Parent hierarchy changed; reload before creating a child page");
        for (const parent of actualParents) requireLocalEditableHierarchyPage(snapshot, parent);
        if (template && JSON.stringify(snapshot.templates.find(row => row.id === template.id)) !== JSON.stringify(template)) throw new Error("Template changed; reload before creating a page");
        for (const tagId of templateTagIds) if (!(snapshot.tags ?? []).some(tag => tag.id === tagId && tag.space_id === spaceId)) throw new Error("Template tag changed; reload before creating a page");
        validateLocalPropertyAssignments(spaceId, templatePropertyValues, snapshot.property_schemas ?? [], snapshot.documents, snapshot.drafts ?? []);
        snapshot.documents = normalizeDocumentIdentifiers([...snapshot.documents, page]);
        snapshot.drafts = [...(snapshot.drafts ?? []), draft];
        snapshot.doc_tags = [...(snapshot.doc_tags ?? []), ...templateTagIds.map(tag_id => ({ document_id: id, tag_id }))];
        snapshot.property_values = [...(snapshot.property_values ?? []), ...templatePropertyValues];
        snapshot.search_index = [...(snapshot.search_index ?? []), { document_id: id, space_id: spaceId, title: page.title, content: page.content.replace(/\s+/g, " ").trim(), search_text: `${page.title}\n${page.content}`.toLowerCase(), updated_at: page.updated_at }];
        snapshot.sync_state = { ...snapshot.sync_state, state: snapshot.sync_state?.state ?? "idle", updated_at: now, queued_operations: [...(snapshot.sync_state?.queued_operations ?? []), ...operations] };
        return { snapshot };
      });
      verifyDraftSaveSelection(selection);
      const committed = saved.documents.find(row => row.id === id)!;
      localDocs.set(id, { ...committed, created_by: committed.created_by ?? LOCAL_SYSTEM_USER, last_updated_by: committed.last_updated_by ?? LOCAL_SYSTEM_USER, created_at: committed.created_at ?? committed.updated_at });
      localDrafts = [...localDrafts.filter(row => row.id !== draftId), saved.drafts!.find(row => row.id === draftId)!];
      localDocTags = [...localDocTags, ...(saved.doc_tags ?? []).filter(row => row.document_id === id)];
      localPropertyValues = [...localPropertyValues, ...(saved.property_values ?? []).filter(row => row.document_id === id)];
      committedPropertyValues = [...committedPropertyValues, ...(saved.property_values ?? []).filter(row => row.document_id === id)];
      localSearchIndex = [...localSearchIndex.filter(row => row.document_id !== id), ...(saved.search_index ?? []).filter(row => row.document_id === id)];
    });
    return { id, draft_id: draftId };
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

  async createTemplateDraft(spaceId: string, templateId: string): Promise<TemplateDraftRecord> {
    if (!inLocalMode()) {
      return api.fetch<TemplateDraftRecord>(`/spaces/${spaceId}/templates/${templateId}/drafts`, {
        method: "POST",
      });
    }
    const template = await this.getTemplate(spaceId, templateId);
    const now = new Date().toISOString();
    return {
      id: `local-template-draft-${templateId}`,
      template_id: templateId,
      space_id: spaceId,
      owner_id: LOCAL_SYSTEM_USER_ID,
      snapshot: {
        name: template.name,
        description: template.description,
        content: template.content,
        properties: template.properties,
        tags: template.tags,
      },
      created_at: now,
      updated_at: now,
      lock_owner_id: null,
      lock_expires_at: null,
    };
  },

  async getTemplateDraft(
    spaceId: string,
    templateId: string,
    draftId: string,
  ): Promise<TemplateDraftRecord> {
    if (inLocalMode()) return this.createTemplateDraft(spaceId, templateId);
    return api.fetch<TemplateDraftRecord>(
      `/spaces/${spaceId}/templates/${templateId}/drafts/${draftId}`,
    );
  },

  async updateTemplateDraft(
    spaceId: string,
    templateId: string,
    draftId: string,
    input: TemplateInput,
  ): Promise<TemplateDraftRecord> {
    if (!inLocalMode()) {
      return api.fetch<TemplateDraftRecord>(
        `/spaces/${spaceId}/templates/${templateId}/drafts/${draftId}`,
        {
          method: "PUT",
          body: normalizeTemplateInput(input),
        },
      );
    }
    const now = new Date().toISOString();
    return {
      id: draftId,
      template_id: templateId,
      space_id: spaceId,
      owner_id: LOCAL_SYSTEM_USER_ID,
      snapshot: normalizeTemplateInput(input),
      created_at: now,
      updated_at: now,
      last_autosaved_at: now,
      lock_owner_id: null,
      lock_expires_at: null,
    };
  },

  async publishTemplateDraft(spaceId: string, templateId: string, draftId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/templates/${templateId}/drafts/${draftId}/publish`, {
        method: "POST",
      });
      return;
    }
  },

  async listTemplateRevisions(
    spaceId: string,
    templateId: string,
  ): Promise<TemplateRevisionRecord[]> {
    if (inLocalMode()) return [];
    return api.fetch<TemplateRevisionRecord[]>(
      `/spaces/${spaceId}/templates/${templateId}/revisions`,
    );
  },

  async revertTemplateRevision(
    spaceId: string,
    templateId: string,
    revisionId: string,
  ): Promise<void> {
    if (inLocalMode()) return;
    await api.fetch(`/spaces/${spaceId}/templates/${templateId}/revisions/${revisionId}/revert`, {
      method: "POST",
    });
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
    if (input.source_id && localTemplateCache.some((item) => item.id === input.source_id)) {
      throw new Error("A template with this import id already exists");
    }
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
    let created: VaultPropertySchema | undefined;
    await commitLocalPropertyMutation((snapshot) => {
      if (!snapshot.spaces.some((space) => space.id === spaceId))
        throw new Error("Space not found");
      validatePropertyImportIdentity(input);
      const schemas = snapshot.property_schemas ?? [];
      if (
        input.source_id !== undefined &&
        schemas.some((schema) => schema.id === input.source_id!.trim())
      )
        throw new Error("A property with this import id already exists");
      created = {
        id: input.source_id?.trim() ?? makeId("prop"),
        space_id: spaceId,
        name: normalized.name,
        property_type: normalized.property_type,
        options: normalized.options,
        sort_order:
          Math.max(
            0,
            ...schemas
              .filter((schema) => schema.space_id === spaceId)
              .map((schema) => schema.sort_order),
          ) + 1,
      };
      snapshot.property_schemas = [...schemas, created];
      return {
        snapshot,
        operation: {
          entity_type: "property_schema",
          entity_id: created.id,
          action: "create",
          payload: created,
        },
      };
    });
    return toPropertySchemaRecord(created!);
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
    await commitLocalPropertyMutation((snapshot) => {
      const schemas = snapshot.property_schemas ?? [];
      const previous = schemas.find(
        (schema) => schema.id === propertyId && schema.space_id === spaceId,
      );
      if (!previous) throw new Error("Property not found in this space");
      validatePropertySchemaChange(
        previous,
        normalized,
        (snapshot.property_values ?? [])
          .filter((value) => value.property_id === propertyId)
          .map((value) => value.value),
      );
      const updated = {
        ...previous,
        name: normalized.name,
        property_type: normalized.property_type,
        options: normalized.options,
      };
      snapshot.property_schemas = schemas.map((schema) =>
        schema.id === propertyId ? updated : schema,
      );
      return {
        snapshot,
        operation: {
          entity_type: "property_schema",
          entity_id: propertyId,
          action: "update",
          payload: updated,
        },
      };
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
    await commitLocalPropertyMutation((snapshot) => {
      if (!snapshot.spaces.some((space) => space.id === spaceId))
        throw new Error("Space not found");
      const schemas = snapshot.property_schemas ?? [];
      const seen = new Set<string>();
      for (const id of input.property_ids) {
        if (
          seen.has(id) ||
          !schemas.some((schema) => schema.id === id && schema.space_id === spaceId)
        )
          throw new Error("Property reorder contains a duplicate or foreign property");
        seen.add(id);
      }
      const orderById = new Map(input.property_ids.map((id, index) => [id, index + 1]));
      snapshot.property_schemas = schemas.map((schema) =>
        schema.space_id === spaceId && orderById.has(schema.id)
          ? { ...schema, sort_order: orderById.get(schema.id)! }
          : schema,
      );
      return {
        snapshot,
        operation: {
          entity_type: "property_schema",
          entity_id: spaceId,
          action: "reorder",
          payload: {
            space_id: spaceId,
            property_ids: input.property_ids,
            schemas: snapshot.property_schemas.filter((schema) => schema.space_id === spaceId),
          },
        },
      };
    });
  },

  async deletePropertySchema(spaceId: string, propertyId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/properties/${propertyId}`, { method: "DELETE" });
      return;
    }
    await ensureLocalData();
    await commitLocalPropertyMutation((snapshot) => {
      const schemas = snapshot.property_schemas ?? [];
      const deleted = schemas.find(
        (schema) => schema.id === propertyId && schema.space_id === spaceId,
      );
      if (!deleted) throw new Error("Property not found in this space");
      snapshot.property_schemas = schemas.filter((schema) => schema.id !== propertyId);
      snapshot.property_values = (snapshot.property_values ?? []).filter(
        (value) => value.property_id !== propertyId,
      );
      return {
        snapshot,
        operation: {
          entity_type: "property_schema",
          entity_id: propertyId,
          action: "delete",
          payload: deleted,
        },
      };
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
    const spaceName = localSpaces.get(spaceId)?.name ?? "Current space";

    return Array.from(localDocs.values())
      .filter((document) => {
        if (document.id === docId || document.space_id !== spaceId) return false;
        return containsDocumentBacklink(document.content, target.title, docId);
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
    await commitLocalPropertyMutation((snapshot) => {
      const document = snapshot.documents.find(
        (doc) => doc.id === docId && doc.space_id === spaceId,
      );
      if (!document) throw new Error("Document not found in this space");
      if (document.archived_at || document.deleted_at)
        throw new Error("Document is archived or deleted");
      validateLocalPropertyAssignments(
        spaceId,
        values,
        snapshot.property_schemas ?? [],
        snapshot.documents,
        snapshot.drafts ?? [],
      );
      const propertyValues = snapshot.property_values ?? [];
      const sourceIds = new Set<string>();
      for (const entry of values) {
        validatePropertyImportIdentity(entry);
        if (entry.source_id !== undefined) {
          const id = entry.source_id.trim();
          if (sourceIds.has(id))
            throw new Error("Duplicate property value import ids are not allowed");
          sourceIds.add(id);
          const existing = propertyValues.find((value) => value.id === id);
          if (
            existing &&
            (existing.document_id !== docId || existing.property_id !== entry.property_id)
          )
            throw new Error("Property value import id conflicts with another value");
        }
      }
      const next = propertyValues.map((value) => ({ ...value }));
      const savedValues: VaultPropertyValue[] = [];
      for (const entry of values) {
        const existing = next.find(
          (value) => value.document_id === docId && value.property_id === entry.property_id,
        );
        if (existing) {
          existing.value = entry.value;
          if (entry.updated_at !== undefined) existing.updated_at = entry.updated_at;
          savedValues.push(existing);
        } else {
          const created = {
            id: entry.source_id?.trim() ?? makeId("propval"),
            document_id: docId,
            property_id: entry.property_id,
            value: entry.value,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
          };
          next.push(created);
          savedValues.push(created);
        }
      }
      snapshot.property_values = next;
      return {
        snapshot,
        operation: {
          entity_type: "document_property_values",
          entity_id: docId,
          action: "set",
          payload: {
            document_id: docId,
            space_id: spaceId,
            values: savedValues.map((entry) => ({
              id: entry.id,
              property_id: entry.property_id,
              value: entry.value,
              created_at: entry.created_at,
              updated_at: entry.updated_at,
            })),
          },
        },
      };
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
    const existing = localTags.find(
      (tag) => tag.space_id === spaceId && tag.name.trim().toLowerCase() === name.toLowerCase(),
    );
    if (existing) return toTagRecord(existing, 0);
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
    await commitLocalDocumentLifecycle(spaceId, docId, "trash");
  },

  async archiveDocument(spaceId: string, docId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/archive`, { method: "POST" });
      return;
    }
    await commitLocalDocumentLifecycle(spaceId, docId, "archive");
  },

  async restoreDocument(spaceId: string, docId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/trash`, { method: "POST" });
      return;
    }
    await commitLocalDocumentLifecycle(spaceId, docId, "restore");
  },

  async unarchiveDocument(spaceId: string, docId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/archive`, { method: "DELETE" });
      return;
    }
    await commitLocalDocumentLifecycle(spaceId, docId, "unarchive");
  },

  async listTrashedDocuments(spaceId: string): Promise<DocumentRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<DocumentRecord[]>(`/spaces/${spaceId}/documents/trash`);
    }
    if (canUseTauriNativeVaultArchive()) return listOriginalNativeTrashDocuments(spaceId);
    await ensureLocalData();
    return Array.from(localDocs.values())
      .filter((doc) => doc.space_id === spaceId && Boolean(doc.deleted_at))
      .map((doc) => ({
        id: doc.id,
        human_identifier: doc.human_identifier,
        maintenance_user_id: doc.maintenance_user_id,
        maintenance_group_id: doc.maintenance_group_id,
        space_id: doc.space_id,
        title: doc.title,
        icon: doc.icon,
        cover_image: doc.cover_image,
        folder_id: doc.folder_id,
        parent_id: doc.parent_id,
        created_by: doc.created_by,
        created_at: doc.created_at,
        updated_at: doc.updated_at,
        archived_at: doc.archived_at,
        deleted_at: doc.deleted_at,
      }));
  },

  async listArchivedDocuments(spaceId: string): Promise<DocumentRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<DocumentRecord[]>(`/spaces/${spaceId}/documents/archive`);
    }
    await ensureLocalData();
    return Array.from(localDocs.values())
      .filter((doc) => doc.space_id === spaceId && Boolean(doc.archived_at) && !doc.deleted_at)
      .map((doc) => ({
        id: doc.id,
        human_identifier: doc.human_identifier,
        maintenance_user_id: doc.maintenance_user_id,
        maintenance_group_id: doc.maintenance_group_id,
        space_id: doc.space_id,
        title: doc.title,
        icon: doc.icon,
        cover_image: doc.cover_image,
        folder_id: doc.folder_id,
        parent_id: doc.parent_id,
        created_by: doc.created_by,
        updated_at: doc.updated_at,
        archived_at: doc.archived_at,
        deleted_at: doc.deleted_at,
      }));
  },

  async permanentlyDeleteDocument(spaceId: string, docId: string, originalTarget?: { created_at?: string; deleted_at?: string | null }, originalNativeSelection?: object): Promise<void> {
    if (!inLocalMode()) {
      if (!originalTarget) throw new Error("Original page identity unavailable. Reload Trash before permanent deletion.");
      await permanentlyDeleteOriginalServerDocument(spaceId, docId, originalTarget);
      return;
    }
    if (!originalTarget) throw new Error("Original page identity unavailable. Reload Trash before permanent deletion.");
    const witness = requirePermanentDeleteTarget(originalTarget);
    if (canUseTauriNativeVaultArchive()) {
      const original = capturePublishedProducerContext(), adapter = localVault;
      const before = JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
      if (!adapter.admitArchiveCanonicalSnapshot) throw new Error("Update the desktop shell before bound native deletion");
      await deleteOriginalNativeDocument(spaceId, docId, witness, async (snapshot, verify) => {
        const requireCurrent = () => { original(); verify(); if (localVault !== adapter) throw new Error("Original native adapter changed"); };
        requireCurrent(); await adapter.admitArchiveCanonicalSnapshot!(snapshot, requireCurrent); requireCurrent();
        publishLocalReviewedMoveSnapshot(snapshot, before); hydratedLocalProvider = getSyncProvider(); localInitialized = true;
      }, originalNativeSelection ? originalNativeTrashBinding(originalNativeSelection, spaceId, docId, witness) : undefined);
      original(); return;
    }
    if (typeof indexedDB === "undefined" && !navigator.locks?.request) throw new Error("This browser cannot serialize canonical deletion. Use a supported browser and retain the original Trash page.");
    const requireOriginal = capturePublishedProducerContext();
    const adapter = localVault;
    await ensureLocalData(); requireOriginal();
    const operationId = crypto.randomUUID();
    await serializeLocalPersistence(async () => {
      requireOriginal();
      if (!await adapter.supportsAtomicSnapshotCommit()) throw new Error("Update this native shell before atomic permanent deletion is available.");
      requireOriginal();
      const saved = await adapter.commitSnapshotMutation(committed => {
        requireOriginal();
        const page = committed.documents.find(row => row.id === docId && row.space_id === spaceId);
        if (!page || page.created_at !== witness.created_at || page.deleted_at !== witness.deleted_at) throw new Error("Original Trash page changed; reload and review without retrying against a replacement.");
        committed.documents = committed.documents.filter(row => row.id !== docId);
        committed.drafts = (committed.drafts ?? []).filter(row => row.document_id !== docId);
        committed.doc_tags = (committed.doc_tags ?? []).filter(row => row.document_id !== docId);
        committed.property_values = (committed.property_values ?? []).filter(row => row.document_id !== docId);
        committed.document_workflows = (committed.document_workflows ?? []).filter(row => row.document_id !== docId);
        committed.document_workflow_events = (committed.document_workflow_events ?? []).filter(row => row.document_id !== docId);
        committed.search_index = (committed.search_index ?? []).filter(row => row.document_id !== docId);
        const originalOperation = originalPublishedContentOperation(page as LocalDocState, page as LocalDocState, "delete");
        return { snapshot: committed, operation: { ...originalOperation, id: operationId, payload: { id: docId, space_id: spaceId, original_target: witness } } };
      });
      requireOriginal();
      const cached = localDocs.get(docId);
      if (cached?.space_id === spaceId && cached.created_at === witness.created_at && cached.deleted_at === witness.deleted_at) {
        localDocs.delete(docId); removeLocalDocumentMetadata(new Set([docId])); localSearchIndex = buildLocalSearchIndexEntries();
      }
      publishCommittedProperties(saved);
    });
  },

  async discoverPendingUploadCleanups(spaceId:string,documentId:string,verifyCurrent:()=>void) {
    if(inLocalMode())throw new Error("Reserved upload review requires its original server context");
    return discoverPendingUploads(spaceId,documentId,verifyCurrent);
  },
  retainedPendingUploadCleanups(spaceId:string,documentId:string,verifyCurrent:()=>void) {
    if(inLocalMode())throw new Error("Reserved upload review requires its original server context");
    return retainedPendingUploadCleanups(spaceId,documentId,verifyCurrent);
  },
  async cleanupOriginalPendingUpload(selection:PendingUploadSelection,verifyCurrent:()=>void) {
    if(inLocalMode())throw new Error("Reserved upload cleanup requires its original server context");
    return cleanupPendingUpload(selection,verifyCurrent);
  },

  async listPendingDocumentDeletions():Promise<PendingDocumentDeletion[]> { return listPendingDocumentDeletions(); },
  async reconcilePendingDocumentDeletion(selection:PendingDocumentDeletion):Promise<CleanupResult|void> {
    const row=JSON.parse(JSON.stringify(selection)) as PendingDocumentDeletion;
    if(!row.canReconcile)throw new Error("Original deletion authority unavailable; retain evidence for review");
    const original=capturePublishedProducerContext();
    if(row.kind==="cleanup") {
      if(inLocalMode())throw new Error("Original attachment cleanup requires its original server context");
      const retained=await listPendingDocumentDeletions();original();
      if(!retained.some(r=>r.kind===row.kind&&r.id===row.id&&r.canReconcile&&JSON.stringify(r.original)===JSON.stringify(row.original)))throw new Error("Original cleanup selection changed; retain evidence");
      const result=await reconcileAcknowledgedCleanup(row.original as CleanupAttempt);original();return result;
    }
    if(row.kind==="server") {
      if(inLocalMode())throw new Error("Original server deletion requires its original server context");
      await reconcileRetainedServerDocumentDelete(row.original as import("./offlineServerCache").QueuedServerMutation);original();return;
    }
    if(!inLocalMode() || !canUseTauriNativeVaultArchive())throw new Error("Original native deletion requires its original local workspace");
    const adapter=localVault,before=JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
    if(!adapter.admitArchiveCanonicalSnapshot)throw new Error("Updated bound native deletion adapter required");
    const retained=await listPendingDocumentDeletions();original();
    if(!retained.some(r=>r.kind===row.kind&&r.id===row.id&&JSON.stringify(r.original)===JSON.stringify(row.original)))throw new Error("Original named native deletion evidence changed; reload review without creating another operation");
    await reconcileOriginalNativeDocumentDelete(row.operation,async(snapshot,verify)=>{
      const requireCurrent=()=>{original();verify();if(localVault!==adapter)throw new Error("Original native adapter changed");};
      requireCurrent();await adapter.admitArchiveCanonicalSnapshot!(snapshot,requireCurrent);requireCurrent();publishLocalReviewedMoveSnapshot(snapshot,before);hydratedLocalProvider=getSyncProvider();localInitialized=true;
    });original();
  },

  async duplicateDocument(spaceId: string, docId: string, title?: string): Promise<DocumentRecord> {
    if (!inLocalMode()) {
      return api.fetch<DocumentRecord>(`/spaces/${spaceId}/documents/${docId}/duplicate`, {
        method: "POST",
        body: title ? { title } : {},
      });
    }
    await ensureLocalData();
    const existing = requireLocalDoc(spaceId, docId);
    const created = await this.createDocument(spaceId, {
      title: title?.trim() || `${existing.title} (Copy)`,
      folder_id: existing.folder_id,
      parent_id: existing.parent_id,
      content: existing.content,
    }, docId);
    return (await this.getDocument(spaceId, created.id)) as DocumentRecord;
  },

  async listDocumentVersions(spaceId: string, docId: string): Promise<DocumentVersionRecord[]> {
    if (!inLocalMode()) {
      return api.fetch<DocumentVersionRecord[]>(`/spaces/${spaceId}/documents/${docId}/versions`);
    }
    await ensureLocalData();
    requireLocalDoc(spaceId, docId);
    return [];
  },

  listDocumentPublicationSnapshots,
  getDocumentPublicationSnapshot,
  readDocumentPublicationSnapshotRaw,

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

  supportsNativeLocalCover(): boolean { return inLocalMode() && canUseTauriNativeVaultArchive(); },

  async saveNativeLocalDocumentCover(input:{file:File;space_id:string;document_id:string;draft_id:string},verifyCurrent:()=>void):Promise<{operationId:string;asset:VaultAsset}>{
    if(!inLocalMode()||!canUseTauriNativeVaultArchive())throw new Error("Current native local cover creator required");
    const original=capturePublishedProducerContext(),adapter=localVault;
    const verify=()=>{original();verifyCurrent();if(localVault!==adapter)throw new Error("Original native local cover adapter changed");};
    verify();await ensureLocalData();verify();requireLocalDoc(input.space_id,input.document_id);
    const before=JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
    if(!adapter.admitArchiveCanonicalSnapshot)throw new Error("Updated bound native cover adapter required");
    const {createOriginalNativeLocalCover}=await import("./nativeLocalCover");verify();
    return createOriginalNativeLocalCover(input.file,{space_id:input.space_id,document_id:input.document_id,draft_id:input.draft_id},verify,async(snapshot,check)=>{
      const current=()=>{verify();check();};current();await adapter.admitArchiveCanonicalSnapshot!(snapshot,current);current();
      publishLocalReviewedMoveSnapshot(snapshot,before);hydratedLocalProvider=getSyncProvider();localInitialized=true;
    });
  },

  async reconcileNativeLocalDocumentCover(operationId:string,selected:{space_id:string;document_id:string;draft_id:string},verifyCurrent:()=>void):Promise<{operationId:string;asset:VaultAsset}>{
    if(!inLocalMode()||!canUseTauriNativeVaultArchive())throw new Error("Current native local cover creator required");
    const original=capturePublishedProducerContext(),adapter=localVault;
    const verify=()=>{original();verifyCurrent();if(localVault!==adapter)throw new Error("Original native local cover adapter changed");};
    verify();await ensureLocalData();verify();requireLocalDoc(selected.space_id,selected.document_id);
    const before=JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
    if(!adapter.admitArchiveCanonicalSnapshot)throw new Error("Updated bound native cover adapter required");
    const {reconcileOriginalNativeLocalCover}=await import("./nativeLocalCover");verify();
    return reconcileOriginalNativeLocalCover(operationId,selected,verify,async(snapshot,check)=>{
      const current=()=>{verify();check();};current();await adapter.admitArchiveCanonicalSnapshot!(snapshot,current);current();
      publishLocalReviewedMoveSnapshot(snapshot,before);hydratedLocalProvider=getSyncProvider();localInitialized=true;
    });
  },

  async resumeNativeLocalDocumentCover(operationId:string,selected:{space_id:string;document_id:string;draft_id:string},verifyCurrent:()=>void):Promise<{operationId:string;asset:VaultAsset}>{
    if(!inLocalMode()||!canUseTauriNativeVaultArchive())throw new Error("Current native local cover creator required");
    const original=capturePublishedProducerContext(),adapter=localVault;
    const verify=()=>{original();verifyCurrent();if(localVault!==adapter)throw new Error("Original native local cover adapter changed");};
    verify();await ensureLocalData();verify();requireLocalDoc(selected.space_id,selected.document_id);
    const before=JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
    if(!adapter.admitArchiveCanonicalSnapshot)throw new Error("Updated bound native cover adapter required");
    const {resumeOriginalNativeLocalCover}=await import("./nativeLocalCover");verify();
    return resumeOriginalNativeLocalCover(operationId,selected,verify,async(snapshot,check)=>{
      const current=()=>{verify();check();};current();await adapter.admitArchiveCanonicalSnapshot!(snapshot,current);current();
      publishLocalReviewedMoveSnapshot(snapshot,before);hydratedLocalProvider=getSyncProvider();localInitialized=true;
    });
  },

  async discardNativeLocalDocumentCover(operationId:string,selected:{space_id:string;document_id:string;draft_id:string},verify:()=>void,inspectOnly=false):Promise<{discardedBytes:number}>{
    if(!this.supportsNativeLocalCover())throw new Error("Original native local discard is unavailable");
    const {discardOriginalNativeLocalCover}=await import("./nativeLocalCover");verify();return discardOriginalNativeLocalCover(operationId,selected,verify,inspectOnly);
  },
  async listRetainedNativeLocalDocumentCovers(selected:{space_id:string;document_id:string;draft_id:string},currentPin:string|null|undefined,verifyCurrent:()=>void){
    if(!inLocalMode()||!canUseTauriNativeVaultArchive())throw new Error("Current native local cover creator required");
    const original=capturePublishedProducerContext(),adapter=localVault;
    const verify=()=>{original();verifyCurrent();if(localVault!==adapter)throw new Error("Original native local cover adapter changed");};
    verify();await ensureLocalData();verify();requireLocalDoc(selected.space_id,selected.document_id);
    const {retainedOriginalNativeLocalCovers}=await import("./nativeLocalCover");verify();
    return retainedOriginalNativeLocalCovers(selected,currentPin,verify);
  },

  /** Original File structured clone and multipart; no data URL/ArrayBuffer copy. */
  async saveDocumentFileAsset(input: {name:string; bytes:Blob; space_id:string; document_id:string;draft_id?:string}): Promise<AttachmentRecord> {
    if (inLocalMode()) throw new Error("Original File upload requires connected server");
    const context = captureOfflineServerContext();
    const original = {...input};
    if (!(original.bytes instanceof Blob) || !original.name.trim()) throw new Error("Original file is unavailable");
    verifyOfflineReplayContext(context,getApiBase(),api.getToken());
    const asset:VaultAsset={id:makeId("asset"),name:original.name,mime_type:original.bytes.type||"application/octet-stream",size:original.bytes.size,url:"",source:"browser",space_id:original.space_id,document_id:original.document_id,created_at:new Date().toISOString()};
    return this.uploadDocumentAttachment(original.space_id,original.document_id,asset,context,original.bytes,original.draft_id);
  },

  async uploadDocumentAttachment(
    spaceId: string,
    docId: string,
    asset: VaultAsset,
    originalContext?: OfflineReplayContext | null,
    originalBlob?: Blob,
    draftId?:string,
  ): Promise<AttachmentRecord> {
    asset = { ...asset };
    const originalDraftId=draftId;
    if (inLocalMode()) {
      throw new Error("Server attachment upload is only available while connected to a server");
    }
    if (originalContext === undefined) {
      try { originalContext = captureOfflineServerContext(); } catch { originalContext = null; /* Ordinary opaque-token online compatibility. */ }
    }
    const base = getApiBase(), bearer = api.getToken(), refresh = api.getRefreshToken();
    const mode = getCurrentLaunchMode(), selection = getStorageSelection();
    const verifyUpload = () => {
      if (originalContext) verifyOfflineReplayContext(originalContext, getApiBase(), api.getToken());
      else if (getApiBase() !== base || api.getToken() !== bearer || localStorage.getItem("access_token") !== bearer ||
        api.getRefreshToken() !== refresh || getCurrentLaunchMode() !== mode ||
        JSON.stringify(getStorageSelection()) !== JSON.stringify(selection)) throw new Error("Original attachment context changed");
    };
    verifyUpload();
    if(originalDraftId!==undefined&&!originalContext)throw new Error("Original draft upload requires durable authenticated provenance");
    const blob = originalBlob ?? await vaultAssetToBlob(asset);
    verifyUpload();
    if (!blob) {
      throw new Error("Asset binary data is unavailable");
    }
    const formData = new FormData();
    formData.append("file", blob, asset.name);
    if (originalContext) {
      const operation = await prepareUploadOperation(originalContext.origin, spaceId, docId, asset.name, blob, originalDraftId);
      try {
      if(originalBlob){await retainDirectUploadReview(operation);verifyUpload();await markDirectUploadUnknown(operation);}
      verifyUpload();
      // Persist unknown before dispatch: a process exit during fetch never looks undispatched.
      await setUploadOperationState(operation, "unknown"); verifyUpload();
      const result = await api.fetchOriginalContext<AttachmentRecord>(originalContext,
        `/spaces/${spaceId}/documents/${docId}/attachments`, { method: "POST", body: formData,
          headers: { "X-Starlane-Upload-Operation": operation.operationId, ...uploadDraftHeaders(operation), ...(originalBlob ? {"X-Starlane-Upload-Length":String(originalBlob.size)} : {}) } });
      verifyUpload(); validateUploadAcknowledgement(operation, result);
      if(originalBlob){await acknowledgeDirectUpload(operation,result);verifyUpload();rememberDirectUploadAcknowledgement(result,operation.operationId);}
      await setUploadOperationState({ ...operation, state: "unknown" }, "acknowledged"); verifyUpload();
      return result;
      } catch(error) {throw attachRetainedUploadOperation(error,operation.operationId);}
    }
    return api.fetch<AttachmentRecord>(`/spaces/${spaceId}/documents/${docId}/attachments`, {
      method: "POST",
      body: formData,
    });
  },

  async retryRetainedDocumentAttachment(operationId: string, context: OfflineReplayContext): Promise<AttachmentRecord> {
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    const operation = await readUploadOperation(operationId, context.origin);
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    if (operation.state === "acknowledged") throw new Error("Upload already acknowledged; inspect its existing attachment");
    if (operation.state === "prepared") await setUploadOperationState(operation, "unknown");
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    const formData = new FormData(); formData.append("file", operation.bytes, operation.originalName);
    const result = await api.fetchOriginalContext<AttachmentRecord>(context,
      `/spaces/${operation.spaceId}/documents/${operation.documentId}/attachments`, { method: "POST", body: formData,
        headers: { "X-Starlane-Upload-Operation": operation.operationId, ...uploadDraftHeaders(operation) } });
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    validateUploadAcknowledgement(operation, result);
    await setUploadOperationState({ ...operation, state: "unknown" }, "acknowledged");
    return result;
  },

  async listDocumentAttachments(spaceId: string, docId: string): Promise<AttachmentRecord[]> {
    if (inLocalMode()) return [];
    return api.fetch<AttachmentRecord[]>(`/spaces/${spaceId}/documents/${docId}/attachments`);
  },

  async listSpaceAttachments(spaceId: string): Promise<AttachmentRecord[]> {
    if (inLocalMode()) return [];
    return api.fetch<AttachmentRecord[]>(`/spaces/${spaceId}/attachments`);
  },

  async listReusableAssets(spaceId?: string): Promise<Array<LocalAssetRecord | AttachmentRecord>> {
    if (!spaceId) return [];
    if (!inLocalMode()) return this.listSpaceAttachments(spaceId);
    await ensureLocalData();
    const assets = await localVault.listAssets();
    snapshotAssetCache = assets;
    return assets.filter((asset) => asset.space_id === spaceId);
  },

  async previewNativeLocalCoverReference(reference:string,spaceId:string,docId:string,draftId?:string):Promise<string>{
    if(!inLocalMode()||!canUseTauriNativeVaultArchive())throw new Error("Current native local cover preview required");
    const session=captureNativeArchiveLocalSession(),adapter=localVault;await ensureLocalData();session.verify();
    const assets=await adapter.listAssets();session.verify();if(localVault!==adapter)throw new Error("Original native cover adapter changed");
    const asset=assets.find(a=>a.url===reference&&a.source==="tauri");if(!asset)throw new Error("Current canonical native cover reference unavailable");
    if(asset.native_local_version_provenance===undefined&&!asset.id.startsWith("native-cover-"))return reference; // Existing canonical legacy preview, no receipt-backed ownership grant.
    if(!asset.native_local_version_provenance)throw new Error("Original local cover provenance unavailable; retain it for review");
    if(asset.space_id!==spaceId||asset.document_id!==docId)throw new Error("Exact imported local cover association differs");
    const {previewImportedNativeLocalCover}=await import("./nativeLocalCover");session.verify();const result=await previewImportedNativeLocalCover({space_id:spaceId,document_id:docId,asset_id:asset.id,draft_id:draftId??null,cover_reference:reference});session.verify();return result;
  },

  async previewImportedDocumentAssetText(spaceId:string,docId:string,assetId:string) {
    if(!inLocalMode()||!isTauri())throw new Error("Current native local imported asset required");
    const session=captureNativeArchiveLocalSession();await ensureLocalData();session.verify();
    const page=await localVault.getDocument(docId);session.verify();
    if(!page||page.space_id!==spaceId)throw new Error("Original imported page association changed");
    const assets=await localVault.listAssets();session.verify();
    const asset=assets.find(asset=>asset.id===assetId&&asset.space_id===spaceId&&asset.document_id===docId&&asset.source==="tauri");
    if(!asset)throw new Error("Current imported asset association missing");
    const result=await previewCurrentImportedNativeAttachmentText({space_id:spaceId,document_id:docId,asset_id:assetId},asset.size);session.verify();return result;
  },

  /** Caller supplies exact current page and immutable record; history never selects by name. */
  async listAttachmentVersionHistory(spaceId:string,docId:string,logicalId:string,after=0) {
    if(inLocalMode())throw new Error("Server attachment history requires connected server");
    return fetchNativeAttachmentHistory({space_id:spaceId,document_id:docId},logicalId,after);
  },

  async selectExactAttachmentVersion(spaceId:string,docId:string,attachment:AttachmentRecord):Promise<SelectedAttachmentVersion> {
    return fetchExactAttachmentVersion({space_id:spaceId,document_id:docId},attachment.id);
  },

  async savePinnedDocumentAttachmentVersion(spaceId:string,docId:string,versionId:string) {
    const context=captureOfflineServerContext();
    const selection=await fetchExactAttachmentVersion({space_id:spaceId,document_id:docId},versionId);
    verifyOfflineReplayContext(context,getApiBase(),api.getToken());
    return saveSelectedAttachment({space_id:spaceId,document_id:docId},selection);
  },

  async saveDocumentAttachmentVersion(spaceId:string,docId:string,attachment:AttachmentRecord) {
    const context=captureOfflineServerContext();
    const selection=await this.selectExactAttachmentVersion(spaceId,docId,attachment);
    verifyOfflineReplayContext(context,getApiBase(),api.getToken());
    return saveSelectedAttachment({space_id:spaceId,document_id:docId},selection);
  },

  async previewPinnedDocumentAttachmentVersionImage(spaceId:string,docId:string,versionId:string) {
    const context=captureOfflineServerContext();
    const selection=await fetchExactAttachmentVersion({space_id:spaceId,document_id:docId},versionId);
    verifyOfflineReplayContext(context,getApiBase(),api.getToken());
    return isTauri() ? imageSelectedAttachment({space_id:spaceId,document_id:docId},selection) : previewBrowserAttachmentImage(selection);
  },

  async previewDocumentAttachmentVersionImage(spaceId:string,docId:string,attachment:AttachmentRecord) {
    const context=captureOfflineServerContext();
    const selection=await this.selectExactAttachmentVersion(spaceId,docId,attachment);
    verifyOfflineReplayContext(context,getApiBase(),api.getToken());
    return isTauri() ? imageSelectedAttachment({space_id:spaceId,document_id:docId},selection) : previewBrowserAttachmentImage(selection);
  },

  async saveSelectedAttachmentVersion(spaceId:string,docId:string,selection:SelectedAttachmentVersion) {
    return saveSelectedAttachment({space_id:spaceId,document_id:docId},selection);
  },

  async previewSelectedAttachmentVersionText(spaceId:string,docId:string,selection:SelectedAttachmentVersion) {
    return previewSelectedAttachmentText({space_id:spaceId,document_id:docId},selection);
  },

  async downloadDocumentAttachmentToVault(
    spaceId: string,
    docId: string,
    attachment: AttachmentRecord,
  ): Promise<string> {
    if (inLocalMode()) {
      throw new Error("Server attachment download requires a connected server");
    }
    if(isTauri())throw new Error("Use original-version native preview or Save file; whole-file data URL delivery unavailable");
    const token = api.getToken();
    const response = await fetch(resolveServerFileUrl(attachment.url), {
      headers: token ? { Authorization: `Bearer ${token}` } : undefined,
    });
    if (!response.ok) throw new Error("Document asset could not be downloaded");
    const dataUrl = await blobToDataUrl(await response.blob());
    const { mirrorServerAttachmentToVault } = await import("@/lib/serverVaultMirror");
    await mirrorServerAttachmentToVault(spaceId, docId, attachment, dataUrl);
    return dataUrl;
  },

  async saveDocumentDataAsset(
    input: DocumentDataAssetInput,
  ): Promise<LocalAssetRecord | AttachmentRecord> {
    input = { ...input }; // Capture the producer fields before any upload wait.
    const localSelection = inLocalMode() ? captureDraftSaveSelection() : undefined;
    const originalLocalSource = localInitialized && inLocalMode() ? localDraftAssetSource(input) : undefined;
    const originalLocalToken = api.getToken();
    const blob = dataUrlToBlob(input.data_url, input.mime_type);
    if (!blob) throw new Error("Document asset data URL is invalid");
    const size = blob.size;

    if (!inLocalMode()) {
      if (!input.space_id || !input.document_id) {
        throw new Error("Server document asset save requires a document context");
      }
      const asset: VaultAsset = {
        id: makeId("asset"),
        name: input.name,
        mime_type: input.mime_type,
        size,
        url: input.data_url,
        source: "browser",
        space_id: input.space_id,
        document_id: input.document_id,
        created_at: new Date().toISOString(),
      };
      let uploadContext: OfflineReplayContext | null = null;
      try { uploadContext = captureOfflineServerContext(); } catch { /* Online upload authority is unchanged. */ }
      const deliberatelyUnsent = typeof navigator !== "undefined" && navigator.onLine === false;
      try {
        if (deliberatelyUnsent) throw new Error("Attachment upload deliberately not dispatched while offline");
        return await this.uploadDocumentAttachment(input.space_id, input.document_id, asset, uploadContext, undefined, input.draft_id);
      } catch (error) {
        // Preserve bytes locally through transient outages. Authentication and validation errors
        // must still surface instead of being mislabeled as an offline save.
        if (isApiError(error) && error.status < 500) throw error;
        if (!uploadContext) throw new Error("Offline attachment lacks original authenticated provenance");
        if (deliberatelyUnsent) verifyOfflineReplayContext(uploadContext, getApiBase(), api.getToken());
        queueOfflineServerAttachment({
          ...(!deliberatelyUnsent && retainedUploadOperationId(error) ? {uploadOperationId:retainedUploadOperationId(error)!} : {}),
          ...(input.draft_id===undefined?{}:{scopeVersion:2 as const,draftId:input.draft_id}),
          origin: uploadContext.origin,
          ...(deliberatelyUnsent ? {} : { replayState: "unknown" as const, lastError: "Server outcome unknown; explicit review required" }),
          serverBase: uploadContext.origin.serverBase,
          path: `/spaces/${input.space_id}/documents/${input.document_id}/attachments`,
          spaceId: input.space_id,
          documentId: input.document_id,
          name: input.name,
          mimeType: input.mime_type,
          dataUrl: input.data_url,
        });
        if (!deliberatelyUnsent) verifyOfflineReplayContext(uploadContext, getApiBase(), api.getToken());
        // The journal durably owns the data URL. Generic native asset persistence
        // selects the current vault after awaits; defer that derived mirror here.
        return asset;
      }
    }

    await ensureLocalData();
    verifyLocalAssetSource(localSelection!, originalLocalSource, originalLocalToken);
    // Draft uploads are independent objects, even when their display names match.
    // The legacy page-owned update path below remains available only without scope.
    if (input.draft_id !== undefined) return this.saveBrowserAsset({ ...input, size, url: input.data_url }, { originalLocalSource: originalLocalSource ?? localDraftAssetSource(input) });
    if (!input.document_id) {
      return this.saveBrowserAsset({
        name: input.name,
        mime_type: input.mime_type,
        size,
        url: input.data_url,
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
        mime_type: input.mime_type,
        size,
        url: input.data_url,
      };
      await localVault.enqueueSyncOperation({
        entity_type: "asset",
        entity_id: updated.id,
        action: "update",
        payload: {
          ...updated,
          data_url: input.data_url,
        },
      });
      await localVault.saveAsset(updated);
      snapshotAssetCache = await localVault.listAssets();
      return updated;
    }

    return this.saveBrowserAsset({
      name: input.name,
      mime_type: input.mime_type,
      size,
      url: input.data_url,
      space_id: input.space_id,
      document_id: input.document_id,
    });
  },

  async saveDocumentTextAsset(
    input: DocumentTextAssetInput,
  ): Promise<LocalAssetRecord | AttachmentRecord> {
    const mimeType = input.mime_type ?? "text/plain";
    return this.saveDocumentDataAsset({
      name: input.name,
      mime_type: mimeType,
      data_url: textToDataUrl(input.text, mimeType),
      space_id: input.space_id,
      document_id: input.document_id,
      draft_id: input.draft_id,
    });
  },

  async resolveDocumentAssetUrl(
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
      return this.downloadDocumentAttachmentToVault(
        context.space_id,
        context.document_id,
        attachment,
      );
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
    return asset?.url ?? null;
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
        const dataUrl = await this.downloadDocumentAttachmentToVault(
          context.space_id,
          context.document_id,
          attachment,
        );
        return dataUrlToBlob(dataUrl)?.text() ?? null;
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

  async saveBrowserAsset(
    input: {
      name: string;
      mime_type?: string;
      size: number;
      url: string;
      space_id?: string;
      document_id?: string;
      draft_id?:string;
    },
    options: { persistInServerMode?: boolean; originalLocalSource?: LocalDraftAssetSource } = {},
  ): Promise<LocalAssetRecord> {
    input = { ...input };
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
    if (!inLocalMode() && !options.persistInServerMode) return asset;

    const selection = captureDraftSaveSelection(), originalToken = api.getToken();
    const originalAdapter = localVault;
    const originalSource = options.originalLocalSource ?? (localInitialized && inLocalMode() ? localDraftAssetSource(input) : undefined);
    await ensureLocalData();
    verifyDraftSaveSelection(selection);
    const source = originalSource ?? (inLocalMode() ? localDraftAssetSource(input) : undefined);
    verifyLocalAssetSource(selection, source, originalToken);
    if (source && canUseTauriNativeAssetImport() && input.url.startsWith("data:")) {
      if (!originalAdapter.admitArchiveCanonicalSnapshot) throw new Error("Updated bound native import adapter required");
      const blob = dataUrlToBlob(input.url, input.mime_type);
      if (!blob || blob.size !== input.size) throw new Error("Original browser asset bytes differ");
      const before = JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
      const { createOriginalNativeLocalImportFromBlob, NativeLocalImportAdoptionError } = await import("./nativeLocalImport");
      try {
        const committed = await createOriginalNativeLocalImportFromBlob(blob, input.name, {
          space_id: source.space_id, document_id: source.document_id,
          draft_id: source.draft_id, draft_created_at: source.draft_created_at,
        }, () => {
          verifyLocalAssetSource(selection, source, originalToken);
          if (localVault !== originalAdapter) throw new Error("Original native import adapter changed");
        }, async (snapshot, check) => {
          const current = () => {
            check();
            verifyLocalAssetSource(selection, source, originalToken);
            if (localVault !== originalAdapter) throw new Error("Original native import adapter changed");
          };
          current();
          await originalAdapter.admitArchiveCanonicalSnapshot!(snapshot, current);
          current();
          publishLocalReviewedMoveSnapshot(snapshot, before);
          hydratedLocalProvider = getSyncProvider();
          localInitialized = true;
        });
        return committed.asset;
      } catch (error) {
        if (error instanceof NativeLocalImportAdoptionError) throw new LocalAssetImportHeldError(error.asset, { selection, source });
        throw error;
      }
    }
    if (canUseTauriNativeAssetImport() && input.url.startsWith("data:")) {
      asset = await invokeTauri<VaultAsset>("import_vault_asset_from_data_url", {
        provider: getSyncProvider(), name: input.name, mimeType: input.mime_type,
        dataUrl: input.url, spaceId: input.space_id, documentId: input.document_id,
      });
      asset = { ...asset, space_id: input.space_id, document_id: input.document_id };
      try { verifyLocalAssetSource(selection, source, originalToken); }
      catch { throw new LocalAssetImportHeldError(asset, { selection, source }); }
    }
    verifyLocalAssetSource(selection, source, originalToken);
    if (source) return persistOriginalScopedLocalAsset(originalAdapter, asset, source, selection, originalToken, input.url.startsWith("data:") ? input.url : undefined);
    await originalAdapter.enqueueSyncOperation({
      entity_type: "asset",
      entity_id: asset.id,
      action: "create",
      payload: {
        ...asset,
        space_id: input.space_id,
        document_id: input.document_id,
        ...(input.draft_id===undefined?{}:{draft_id:input.draft_id}),
        ...(source ? { local_draft_source: source } : {}),
        data_url: input.url.startsWith("data:") ? input.url : undefined,
      },
    });
    verifyLocalAssetSource(selection, source, originalToken);
    await originalAdapter.saveAsset(asset);
    verifyLocalAssetSource(selection, source, originalToken);
    const savedAssets = await originalAdapter.listAssets();
    verifyLocalAssetSource(selection, source, originalToken);
    snapshotAssetCache = savedAssets;
    return asset;
  },

  async importAssetFromPath(
    sourcePath: string,
    context: LocalAssetContext = {},
  ): Promise<LocalAssetRecord> {
    context={...context};
    if (!inLocalMode()) {
      throw new Error("Native vault asset import is only available in local mode");
    }
    const selection = captureDraftSaveSelection(), originalToken = api.getToken();
    const originalAdapter = localVault;
    const originalSource = localInitialized ? localDraftAssetSource(context) : undefined;
    await ensureLocalData();
    verifyDraftSaveSelection(selection);
    const source = originalSource ?? localDraftAssetSource(context);
    verifyLocalAssetSource(selection, source, originalToken);
    if (!canUseTauriNativeAssetImport()) {
      throw new Error("Native vault asset import is unavailable");
    }

    if (source) {
      if (!originalAdapter.admitArchiveCanonicalSnapshot) throw new Error("Updated bound native import adapter required");
      const before = JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
      const { createOriginalNativeLocalImportFromPath, NativeLocalImportAdoptionError } = await import("./nativeLocalImport");
      try {
        const committed = await createOriginalNativeLocalImportFromPath(sourcePath, {
          space_id: source.space_id, document_id: source.document_id,
          draft_id: source.draft_id, draft_created_at: source.draft_created_at,
        }, () => {
          verifyLocalAssetSource(selection, source, originalToken);
          if (localVault !== originalAdapter) throw new Error("Original native import adapter changed");
        }, async (snapshot, check) => {
          const current = () => {
            check();
            verifyLocalAssetSource(selection, source, originalToken);
            if (localVault !== originalAdapter) throw new Error("Original native import adapter changed");
          };
          current();
          await originalAdapter.admitArchiveCanonicalSnapshot!(snapshot, current);
          current();
          publishLocalReviewedMoveSnapshot(snapshot, before);
          hydratedLocalProvider = getSyncProvider();
          localInitialized = true;
        });
        return committed.asset;
      } catch (error) {
        if (error instanceof NativeLocalImportAdoptionError) throw new LocalAssetImportHeldError(error.asset, { selection, source });
        throw error;
      }
    }

    const asset = await invokeTauri<VaultAsset>("import_vault_asset_from_path", {
      provider: getSyncProvider(), sourcePath, spaceId: context.space_id, documentId: context.document_id,
    });
    const ownedAsset = { ...asset, space_id: context.space_id, document_id: context.document_id };
    try { verifyLocalAssetSource(selection, source, originalToken); }
    catch { throw new LocalAssetImportHeldError(ownedAsset, { selection, source }); }
    if (source) return persistOriginalScopedLocalAsset(originalAdapter, ownedAsset, source, selection, originalToken);
    await originalAdapter.enqueueSyncOperation({
      entity_type: "asset",
      entity_id: ownedAsset.id,
      action: "create",
      payload: {
        ...ownedAsset,
        space_id: context.space_id,
        document_id: context.document_id,
        ...(context.draft_id===undefined?{}:{draft_id:context.draft_id}),
        ...(source ? { local_draft_source: source } : {}),
      },
    });
    verifyLocalAssetSource(selection, source, originalToken);
    await originalAdapter.saveAsset(ownedAsset);
    verifyLocalAssetSource(selection, source, originalToken);
    const savedAssets = await originalAdapter.listAssets();
    verifyLocalAssetSource(selection, source, originalToken);
    snapshotAssetCache = savedAssets;
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
    return createLocalVaultAdapter(hydratedLocalProvider ?? localVault.provider).loadSnapshot();
  },

  async listPortableReviewedDraftTransfers(): Promise<ReviewedDraftTransferRecord[]> {
    if (!inLocalMode()) throw new Error("Reviewed draft transfer evidence is only available in local mode");
    await ensureLocalData();
    const snapshot = await createLocalVaultAdapter(hydratedLocalProvider ?? localVault.provider).loadSnapshot();
    assertReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
    return JSON.parse(JSON.stringify(snapshot.reviewed_draft_transfer_records ?? [])) as ReviewedDraftTransferRecord[];
  },

  async savePortableReviewedDraftTransfer(record: ReviewedDraftTransferRecord): Promise<ReviewedDraftTransferRecord> {
    if (!inLocalMode()) throw new Error("Reviewed draft transfer evidence is only available in local mode");
    assertReviewedDraftTransferRecords([record]);
    await ensureLocalData();
    const adapter = createLocalVaultAdapter(hydratedLocalProvider ?? localVault.provider);
    const saved = await adapter.commitSnapshotMutation(snapshot => {
      assertReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
      const existing = snapshot.reviewed_draft_transfer_records?.find(row => row.operation_id === record.operation_id);
      if (existing && (JSON.stringify(existing.request) !== JSON.stringify(record.request) || JSON.stringify(existing.exact_manifest) !== JSON.stringify(record.exact_manifest)))
        throw new Error("Reviewed draft transfer evidence conflicts with its original request; retain both vault copies for review");
      snapshot.reviewed_draft_transfer_records = [
        ...(snapshot.reviewed_draft_transfer_records ?? []).filter(row => row.operation_id !== record.operation_id),
        JSON.parse(JSON.stringify(record)) as ReviewedDraftTransferRecord,
      ];
      return { snapshot };
    });
    assertReviewedDraftTransferRecords(saved.reviewed_draft_transfer_records);
    localReviewedDraftTransferRecords = saved.reviewed_draft_transfer_records === undefined ? undefined : JSON.parse(JSON.stringify(saved.reviewed_draft_transfer_records)) as ReviewedDraftTransferRecord[];
    const retained = localReviewedDraftTransferRecords?.find(row => row.operation_id === record.operation_id);
    if (!retained || JSON.stringify(retained) !== JSON.stringify(record)) throw new Error("Portable reviewed draft transfer readback differs; no request is permitted");
    return JSON.parse(JSON.stringify(retained)) as ReviewedDraftTransferRecord;
  },

  async exportQueuedSyncReplayManifest(
    operationIds?: string[],
    draftBindings?: LocalSyncReplayDraftBindingSidecarRecord,
  ): Promise<LocalSyncReplayManifestRecord> {
    if (!inLocalMode()) {
      throw new Error("Queued sync replay export is only available in local mode");
    }
    await ensureLocalData();
    const snapshot = await createLocalVaultAdapter(hydratedLocalProvider ?? localVault.provider).loadSnapshot();
    requireNoPendingGoogleReviewedMoves(snapshot.sync_state?.queued_operations ?? []);
    const requestedIds = operationIds && operationIds.length > 0 ? new Set(operationIds) : null;
    const operations = (snapshot.sync_state?.queued_operations ?? []).filter(
      (operation) => !requestedIds || requestedIds.has(operation.id),
    );
    for (const operation of operations) {
      if (operation.replay_provider !== undefined && operation.replay_provider !== getSyncProvider())
        throw new Error("This identifier edit belongs to a different sync provider. Restore that provider before replaying its original operation.");
      if (operation.server_context) {
        const account = await api.fetch<{ id: string }>("/auth/me");
        if (identifierCredentialAccount(api.getToken()) !== account.id || operation.server_context.server_base !== getApiBase() || operation.server_context.account_id !== account.id || operation.server_context.session_scope !== serverCacheScopeForSession(api.getToken(), api.getRefreshToken()))
          throw new Error("This identifier edit belongs to a different server or account. Sign in to its original account before recovery.");
      }
    }
    const recoveryCheckpoints = readSharedSpaceDownloadCheckpoints();
    const replayContract = buildNativeLocalDatabaseReplayContract();
    const replayOperations = await Promise.all(
      sortReplayOperationsForContract(operations).map(async (operation) => {
        const payload = await hydrateReplayOperationPayload(
          operation,
          findQueuedOperationPayload(snapshot, operation),
        );
        if (operation.entity_type === "document" && operation.action === "delete" && (!canonicalDeleteOperation(operation.id) || !originalDeleteTarget(payload) || !operation.server_context)) throw new Error("Legacy queued deletion lacks original incarnation evidence; retain it for explicit review.");
        const payloadAvailable = isQueuedReplayPayloadAvailable(operation, payload);
        return {
          ...operation,
          payload_status: payloadAvailable ? "available" : "missing",
          payload,
        } satisfies LocalSyncReplayOperationRecord;
      }),
    );
    if (draftBindings) {
      const selected = new Map(replayOperations.map((operation) => [operation.id, operation]));
      if (
        draftBindings.schema_version !== 1 ||
        draftBindings.binding_count !== draftBindings.bindings.length ||
        new Set(draftBindings.bindings.map((binding) => binding.operation_id)).size !== draftBindings.bindings.length
      ) throw new Error("Reviewed draft replay bindings are malformed");
      for (const binding of draftBindings.bindings) {
        const operation = selected.get(binding.operation_id);
        if (!operation || operation.entity_type !== "asset" || operation.action !== "create")
          throw new Error("Reviewed draft replay binding does not match one selected asset create operation");
        if (await publishedReplayFingerprint(operation.payload) !== binding.operation_fingerprint)
          throw new Error("Reviewed draft replay binding differs from its exact original payload");
      }
    }
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
      ...(draftBindings ? { draft_bindings: JSON.parse(JSON.stringify(draftBindings)) as LocalSyncReplayDraftBindingSidecarRecord } : {}),
      operations: replayOperations,
    };
  },

  async preflightQueuedSyncReplayManifest(
    operationIds?: string[],
    draftBindings?: LocalSyncReplayDraftBindingSidecarRecord,
  ): Promise<LocalSyncReplayServerPreflightRecord> {
    if (!inLocalMode()) {
      throw new Error("Queued sync replay preflight is only available in local mode");
    }
    const manifest = await this.exportQueuedSyncReplayManifest(operationIds, draftBindings);
    return api.fetch<LocalSyncReplayServerPreflightRecord>("/sync/replay/preflight", {
      method: "POST",
      body: manifest,
    });
  },

  async runQueuedSyncReplay(
    operationIds?: string[],
    draftBindings?: LocalSyncReplayDraftBindingSidecarRecord,
  ): Promise<LocalSyncReplayServerResultRecord> {
    if (!inLocalMode()) {
      throw new Error("Queued sync replay is only available in local mode");
    }
    const originalProvider = getSyncProvider(), originalBase = getApiBase(), originalToken = api.getToken();
    const originalMode = getCurrentLaunchMode(), originalSelection = JSON.stringify(getStorageSelection());
    const originalScope = serverCacheScopeForSession(originalToken, api.getRefreshToken());
    const originalDiskToken = localStorage.getItem("access_token");
    const originalAdapter = createLocalVaultAdapter(originalProvider);
    const verifyResultContext = () => {
      if (localStorage.getItem("access_token") !== originalDiskToken || getSyncProvider() !== originalProvider || getApiBase() !== originalBase || api.getToken() !== originalToken || getCurrentLaunchMode() !== originalMode || JSON.stringify(getStorageSelection()) !== originalSelection || serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== originalScope) throw new Error("Replay context changed; retain original operations and review their server outcomes.");
    };
    const manifest: LocalSyncReplayManifestRecord = JSON.parse(JSON.stringify(await this.exportQueuedSyncReplayManifest(operationIds, draftBindings)));
    verifyResultContext();
    const fingerprints = new Map<string, string>();
    for (const op of manifest.operations) if (op.payload && typeof op.payload === "object" && Object.prototype.hasOwnProperty.call(op.payload, "published_source_digest")) fingerprints.set(op.id, await publishedReplayFingerprint(op.payload));
    verifyResultContext();
    const result = await api.fetch<LocalSyncReplayServerResultRecord>("/sync/replay", {
      method: "POST",
      body: manifest,
    });
    verifyResultContext();
    // An exact receipt result may advance only the unchanged local confirmed A.
    // Existing B operation/payload is never rewritten or silently acknowledged.
    if (result.results.some((entry) => ["applied", "skipped"].includes(entry.status) && typeof entry.published_source?.published_source_digest === "string" && /^[0-9a-f]{64}$/.test(entry.published_source.published_source_digest)) && await originalAdapter.supportsAtomicSnapshotCommit()) {
      verifyResultContext();
      await serializeLocalPersistence(async () => {
        verifyResultContext();
        const saved = await originalAdapter.commitSnapshotMutation((committed) => {
          verifyResultContext();
          for (const outcome of result.results) {
            const proof = outcome.published_source;
            if (!["applied", "skipped"].includes(outcome.status) || !proof || typeof proof.published_source_digest !== "string" || !/^[0-9a-f]{64}$/.test(proof.published_source_digest)) continue;
            if (result.results.filter((entry) => entry.operation_id === outcome.operation_id).length !== 1) continue;
            const matches = manifest.operations.filter((op) => op.id === outcome.operation_id && op.entity_type === outcome.entity_type && op.action === outcome.action);
            if (matches.length !== 1) continue;
            const operation = matches[0];
            const queued = committed.sync_state?.queued_operations.find((op) => op.id === operation.id);
            const expectedOperation: Partial<LocalSyncReplayOperationRecord> = { ...operation }; delete expectedOperation.payload_status;
            // Compare every raw queue field, including original origin and payload.
            if (!queued || !publishedReplayEqual(queued, expectedOperation)) continue;
            const payload = operation.payload && typeof operation.payload === "object" && !Array.isArray(operation.payload) ? operation.payload as Record<string, unknown> : null;
            const doc = committed.documents.find((page) => page.id === operation.entity_id);
            if (!payload || typeof payload.published_source_digest !== "string" || operation.entity_type !== "document" || !(operation.action === "update_content" || operation.action.startsWith("resolve_conflict:")) || operation.replay_provider !== originalProvider || operation.server_context?.server_base !== originalBase || operation.server_context?.session_scope !== originalScope || operation.server_context?.account_id !== identifierCredentialAccount(originalToken)) continue;
            if (proof.payload_fingerprint !== fingerprints.get(operation.id) || proof.document_id !== operation.entity_id || proof.space_id !== payload.space_id || proof.original_source_digest !== payload.published_source_digest) continue;
            // Exact receipt ACK removes A only; dependent/concurrent B stays raw.
            if (committed.sync_state) committed.sync_state.queued_operations = committed.sync_state.queued_operations.filter((op) => op.id !== operation.id);
            if (!doc?.published_source || doc.published_source.digest !== payload.published_source_digest) continue;
            const expectedDocument = { ...payload }; delete expectedDocument.published_source_digest;
            if (!publishedReplayEqual(doc, expectedDocument)) continue;
            doc.published_source = { ...doc.published_source, digest: proof.published_source_digest };
          }
          return { snapshot: committed };
        });
        verifyResultContext();
        for (const persisted of saved.documents) {
          const current = localDocs.get(persisted.id);
          if (!current || !persisted.published_source || !current.published_source) continue;
          const unchanged = { ...persisted, published_source: current.published_source };
          if (publishedReplayEqual(current, unchanged)) localDocs.set(current.id, { ...current, published_source: persisted.published_source });
        }
      });
    }
    verifyResultContext();
    const failures = result.results.filter((entry) => entry.status === "failed" || entry.status === "unsupported");
    if (failures.length > 0 && await originalAdapter.supportsAtomicSnapshotCommit()) {
      verifyResultContext();
      await serializeLocalPersistence(async () => {
        verifyResultContext();
        await originalAdapter.commitSnapshotMutation((committed) => {
          verifyResultContext();
          let changed = false;
          for (const outcome of failures) {
            if (result.results.filter((entry) => entry.operation_id === outcome.operation_id).length !== 1) continue;
            const originals = manifest.operations.filter((op) => op.id === outcome.operation_id && op.entity_type === outcome.entity_type && op.action === outcome.action);
            if (originals.length !== 1) continue;
            const expected: Partial<LocalSyncReplayOperationRecord> = { ...originals[0] }; delete expected.payload_status;
            const current = committed.sync_state?.queued_operations.find((op) => op.id === outcome.operation_id);
            if (!current || !publishedReplayEqual(current, expected)) continue;
            current.attempts = (current.attempts ?? 0) + 1;
            current.last_error = outcome.message;
            changed = true;
          }
          if (changed && committed.sync_state) committed.sync_state.state = "error";
          return { snapshot: committed };
        });
        verifyResultContext();
      });
    }
    verifyResultContext();
    return result;
  },

  async createReviewedDraftTransfer(
    serverSpaceId: string,
    serverDocumentId: string,
    request: ReviewedDraftTransferRequestRecord,
  ): Promise<ReviewedDraftTransferReceiptRecord> {
    if (!inLocalMode()) throw new Error("Reviewed local draft transfer is only available from local mode");
    const receipt = await api.fetch<ReviewedDraftTransferReceiptRecord>(
      `/spaces/${encodeURIComponent(serverSpaceId)}/documents/${encodeURIComponent(serverDocumentId)}/draft-transfers`,
      { method: "POST", body: request },
    );
    if (
      receipt.schema_version !== 1 || receipt.operation_id !== request.operation_id ||
      receipt.server_space_id !== serverSpaceId || receipt.server_document_id !== serverDocumentId ||
      !nativeJsonValueEquals(receipt.local_source, request.local_source) ||
      !nativeJsonValueEquals(receipt.selected_operation_fingerprints, request.selected_operation_fingerprints) ||
      !/^[0-9a-f]{64}$/.test(receipt.request_fingerprint) || !/^[0-9a-f]{64}$/.test(receipt.target_source_digest) ||
      !receipt.target_draft_id || !receipt.target_draft_created_at || !receipt.server_actor_id || !receipt.created_at
    ) throw new Error("Reviewed draft transfer acknowledgement differs from the exact original request");
    return receipt;
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
    const verifyCurrent = capturePublishedProducerContext();
    const capturedProvider = getSyncProvider();
    const capturedAdapter = localVault;
    const runOriginal = async () => {
      await guardSelectedBrowserGooglePending(capturedProvider, verifyCurrent);
      await ensureLocalData(); verifyCurrent();
      const pendingMoves = (await capturedAdapter.loadSnapshot()).sync_state?.queued_operations ?? [];
      requireNoPendingGoogleReviewedMoves(pendingMoves);
      assertNoReviewedMoveProviderManifest(manifest);
      await providerStateWithoutReviewedMove(capturedAdapter, "syncing", verifyCurrent);
      try {
        const localQueuedOperations = (await capturedAdapter.loadSnapshot()).sync_state?.queued_operations ?? [];
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
            applyOperation: async operation => {
              verifyCurrent(); const latest = await capturedAdapter.loadSnapshot(); verifyCurrent();
              requireNoPendingGoogleReviewedMoves(latest.sync_state?.queued_operations ?? []);
              const result = await applyProviderRemoteImportOperationToLocalVault(operation); verifyCurrent(); return result;
            },
          },
        });
        if (result.status === "completed") {
          await providerStateWithoutReviewedMove(capturedAdapter, "idle", verifyCurrent);
        } else if (result.results.some((operation) => operation.status === "blocked")) {
          await providerStateWithoutReviewedMove(capturedAdapter, "conflict", verifyCurrent);
        } else {
          await providerStateWithoutReviewedMove(capturedAdapter, "error", verifyCurrent);
        }
        return result;
      } catch (error) {
        await providerStateWithoutReviewedMove(capturedAdapter, "error", verifyCurrent);
        throw error;
      }
    };
    return getSyncProvider() === "google-drive" ? withGoogleReviewedMovePublicationLock(verifyCurrent, runOriginal) : runOriginal();
  },

  previewVaultMigration(snapshot: unknown): VaultMigrationSummaryRecord {
    assertLocalVaultSnapshot(snapshot);
    return summarizeVaultSnapshot(normalizeLocalVaultSnapshot(snapshot));
  },

  async importLocalVaultSnapshot(snapshot: unknown): Promise<void> {
    if (!inLocalMode()) throw new Error("Vault import is only available in local mode");
    const requireOriginalContext = capturePublishedProducerContext();
    const provider = getSyncProvider();
    const capturedImportAdapter = createLocalVaultAdapter(provider);
    const importOriginal = async () => {
      const nextSnapshot = prepareImportedSnapshot(snapshot);
      const carriesPublishedSource = nextSnapshot.documents.some((doc) => doc.published_source !== undefined);
      const carriesReviewedMoveEvidence = nextSnapshot.reviewed_move_records !== undefined;
      await requireNativeReplayQueueCapability(nextSnapshot, provider);
      requireOriginalContext();
      if ((provider === "google-drive" && !canUseTauriNativeVaultArchive()) || carriesPublishedSource || carriesReviewedMoveEvidence) {
        const adapter = capturedImportAdapter;
        const atomic = await adapter.supportsAtomicSnapshotCommit();
        requireOriginalContext();
        if (!atomic) throw new Error("Update the desktop shell before importing original published source evidence");
        await adapter.commitSnapshotMutation(() => {
          requireOriginalContext();
          return { snapshot: nextSnapshot };
        });
      } else {
        // Exact incumbent source-free native/legacy path: no added describe/init
        // or read commands; imported raw queue provenance remains unchanged.
        await localVault.saveSnapshot(nextSnapshot);
      }
      requireOriginalContext();
      hydrateFromSnapshot(nextSnapshot);
      hydratedLocalProvider = (provider === "google-drive" && !canUseTauriNativeVaultArchive()) || carriesPublishedSource || carriesReviewedMoveEvidence ? provider : localVault.provider;
      localInitialized = true;
    };
    return provider === "google-drive" ? withGoogleReviewedMovePublicationLock(requireOriginalContext, importOriginal) : importOriginal();
  },

  async importVaultSnapshotToServer(
    snapshot: unknown,
    options: {
      maintenance_identity_map?: {
        users: Record<string, string>;
        groups: Record<string, string>;
      };
    } = {},
  ): Promise<VaultMigrationSummaryRecord> {
    if (inLocalMode()) {
      throw new Error("Server migration is only available while connected to a server");
    }
    assertLocalVaultSnapshot(snapshot);
    assertRawImportedProperties(snapshot);
    const normalized = normalizeLocalVaultSnapshot(snapshot);
    let requireOriginalContext = () => {};
    const assigned = normalized.documents.filter(
      (doc) => doc.maintenance_user_id || doc.maintenance_group_id,
    );
    if (assigned.length) {
      const token = api.getToken(),
        base = getApiBase();
      const scope = serverCacheScopeForSession(token, api.getRefreshToken());
      const account = await api.fetch<{ id: string }>("/auth/me");
      if (
        identifierCredentialAccount(token) !== account.id ||
        token !== api.getToken() ||
        base !== getApiBase()
      )
        throw new Error("Server account changed before maintenance import mapping");
      requireOriginalContext = () => {
        if (
          token !== api.getToken() ||
          base !== getApiBase() ||
          serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== scope ||
          identifierCredentialAccount(api.getToken()) !== account.id
        )
          throw new Error(
            "Server account changed during maintenance import; retain remaining source records for the original account",
          );
      };
      requireOriginalContext();
      const catalogs = normalizeDocumentMaintenanceCatalogs(
        normalized.maintenance_target_catalogs,
      );
      normalized.documents = normalized.documents.map((doc) => {
        if (!doc.maintenance_user_id && !doc.maintenance_group_id) return doc;
        const same = catalogs.some(
          (record) =>
            record.space_id === doc.space_id &&
            record.server_base === base &&
            record.account_id === account.id,
        );
        if (same) return doc;
        const user = doc.maintenance_user_id
          ? options.maintenance_identity_map?.users[doc.maintenance_user_id]
          : null;
        const group = doc.maintenance_group_id
          ? options.maintenance_identity_map?.groups[doc.maintenance_group_id]
          : null;
        if ((doc.maintenance_user_id && !user) || (doc.maintenance_group_id && !group))
          throw new Error(
            "Map every imported maintenance person/team to exact destination IDs before importing any server entities",
          );
        return normalizeDocumentMaintenanceFields({
          ...doc,
          maintenance_user_id: user,
          maintenance_group_id: group,
        });
      });
    }

    const scopedClient = new Proxy(this, {
      get(target, property, receiver) {
        const value = Reflect.get(target, property, receiver);
        if (typeof value !== "function") return value;
        return async (...args: unknown[]) => {
          requireOriginalContext();
          const result = await value.apply(target, args);
          requireOriginalContext();
          return result;
        };
      },
    });
    // Preflight wire types and snapshot scope before creating remote entities.
    // The server resolves account activity and effective relation access.
    validateImportedSnapshotProperties(normalized, false);
    const userPropertyIds = new Set(
      (normalized.property_schemas ?? [])
        .filter((schema) => schema.property_type === "user")
        .map((schema) => schema.id),
    );
    if (
      (normalized.property_values ?? []).some(
        (value) =>
          userPropertyIds.has(value.property_id) &&
          value.value !== null &&
          value.value !== "",
      )
    )
      throw new Error(
        "Choose an active server account mapping for user property values before migration; the local vault has not changed",
      );
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
      const createdSpace = await scopedClient.createSpace(sourceSpace.name, {
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

          const createdFolder = await scopedClient.createFolder(
            createdSpace.id,
            folder.name,
            {
              parent_id: parentId ?? null,
              source_id: folder.id,
            },
          );
          folderMap.set(folder.id, createdFolder.id);
          pendingFolders.splice(index, 1);
          madeProgress = true;
        }
      }
      for (const folder of pendingFolders) {
        const createdFolder = await scopedClient.createFolder(
          createdSpace.id,
          folder.name,
          {
            source_id: folder.id,
          },
        );
        folderMap.set(folder.id, createdFolder.id);
      }

      for (const doc of normalized.documents.filter(
        (item) => item.space_id === sourceSpace.id,
      )) {
        const createdDoc = await scopedClient.createDocument(createdSpace.id, {
          title: doc.title,
          content: doc.content,
          folder_id: doc.folder_id ? (folderMap.get(doc.folder_id) ?? null) : null,
          human_identifier: doc.human_identifier,
          maintenance_user_id: doc.maintenance_user_id,
          maintenance_group_id: doc.maintenance_group_id,
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
        await scopedClient.createTemplate(createdSpace.id, {
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
        const createdProperty = await scopedClient.createPropertySchema(createdSpace.id, {
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
        await scopedClient.reorderPropertySchemas(createdSpace.id, {
          property_ids: propertyOrder,
        });
      }

      for (const tag of (normalized.tags ?? []).filter(
        (item) => item.space_id === sourceSpace.id,
      )) {
        const createdTag = await scopedClient.createTag(createdSpace.id, {
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
        const createdWorkflowTemplate = await scopedClient.createWorkflowTemplate(
          createdSpace.id,
          {
            name: workflowTemplate.name,
            description: workflowTemplate.description,
            stages: workflowTemplate.stages,
            approvers: workflowTemplate.approvers,
            source_id: workflowTemplate.id,
            created_at: workflowTemplate.created_at,
            updated_at: workflowTemplate.updated_at,
          },
        );
        workflowTemplateMap.set(workflowTemplate.id, createdWorkflowTemplate.id);
      }
    }

    const skippedAssets: string[] = [];
    for (const asset of normalized.assets) {
      const sourceDocumentId = extractDocumentIdFromAssetPath(
        asset,
        normalized.documents,
      );
      const migratedDocument = sourceDocumentId
        ? migratedDocumentContent.get(sourceDocumentId)
        : undefined;
      if (!sourceDocumentId || !migratedDocument) {
        skippedAssets.push(asset.name);
        continue;
      }

      try {
        const uploaded = await scopedClient.uploadDocumentAttachment(
          migratedDocument.space_id,
          migratedDocument.document_id,
          asset,
        );
        if (migratedDocument.content.includes(asset.url)) {
          migratedDocument.content = migratedDocument.content
            .split(asset.url)
            .join(uploaded.url);
          migratedDocumentContent.set(sourceDocumentId, migratedDocument);
        }
      } catch {
        skippedAssets.push(asset.name);
      }
    }

    for (const migratedDocument of migratedDocumentContent.values()) {
      if (migratedDocument.original_content === migratedDocument.content) continue;
      await scopedClient.updateDocumentContent(
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
      const sourceDoc = normalized.documents.find(
        (doc) => doc.id === propertyValue.document_id,
      );
      const mappedSpaceId = sourceDoc?.space_id
        ? spaceMap.get(sourceDoc.space_id)
        : undefined;
      if (!mappedSpaceId) continue;
      await scopedClient.setDocumentPropertyValues(mappedSpaceId, mappedDocumentId, [
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
      const mappedSpaceId = sourceDoc?.space_id
        ? spaceMap.get(sourceDoc.space_id)
        : undefined;
      if (!mappedSpaceId) continue;
      await scopedClient.addDocumentTag(mappedSpaceId, mappedDocumentId, mappedTagId);
    }

    for (const workflow of normalized.document_workflows ?? []) {
      const mappedDocumentId = documentMap.get(workflow.document_id);
      const mappedWorkflowTemplateId = workflowTemplateMap.get(
        workflow.workflow_template_id,
      );
      if (!mappedDocumentId || !mappedWorkflowTemplateId) continue;
      const sourceDoc = normalized.documents.find(
        (doc) => doc.id === workflow.document_id,
      );
      const mappedSpaceId = sourceDoc?.space_id
        ? spaceMap.get(sourceDoc.space_id)
        : undefined;
      if (!mappedSpaceId) continue;
      const workflowEvents = (normalized.document_workflow_events ?? []).filter(
        (item) =>
          item.document_workflow_id === workflow.id &&
          item.document_id === workflow.document_id,
      );
      const enableEvent = workflowEvents.find((event) => event.action === "enable");
      await scopedClient.enableDocumentWorkflow(mappedSpaceId, mappedDocumentId, {
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
        await scopedClient.recordDocumentWorkflowEvent(mappedSpaceId, mappedDocumentId, {
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

    requireOriginalContext();
    return summary;
  },

  async exportServerVaultSnapshot(): Promise<LocalVaultSnapshot> {
    if (inLocalMode()) {
      throw new Error("Server export is only available while connected to a server");
    }
    const token = api.getToken(),
      base = getApiBase();
    const scope = serverCacheScopeForSession(token, api.getRefreshToken());
    const subject = identifierCredentialAccount(token);
    const exportedProvider = getSyncProvider(), exportedSelection = JSON.stringify(getStorageSelection());
    const account = await api.fetch<{ id: string }>("/auth/me");
    const requireOriginalContext = () => {
      if (
        inLocalMode() || getSyncProvider() !== exportedProvider || JSON.stringify(getStorageSelection()) !== exportedSelection ||
        account.id !== subject ||
        api.getToken() !== token ||
        getApiBase() !== base ||
        serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== scope
      )
        throw new Error(
          "Server account changed during maintenance identity export; no mixed-account snapshot can be returned",
        );
    };
    requireOriginalContext();
    const scopedClient = new Proxy(this, {
      get(target, property, receiver) {
        const value = Reflect.get(target, property, receiver);
        if (typeof value !== "function") return value;
        return async (...args: unknown[]) => {
          requireOriginalContext();
          const result = await value.apply(target, args);
          requireOriginalContext();
          return result;
        };
      },
    });
    const now = new Date().toISOString();
    const spaces = await scopedClient.listSpaces();
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

    const exportedCatalogs: DocumentMaintenanceCatalog[] = [];
    for (const space of spaces) {
      const [folders, documents, templates, workflowTemplates, propertySchemas, tags] =
        await Promise.all([
          scopedClient.listFolders(space.id),
          scopedClient.listDocuments(space.id, { include_nested: true }),
          scopedClient.listTemplates(space.id),
          scopedClient.listWorkflowTemplates(space.id),
          scopedClient.listPropertySchemas(space.id),
          scopedClient.listTags(space.id),
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

      const assignedPage = documents.find(
        (doc) => doc.maintenance_user_id || doc.maintenance_group_id,
      );
      if (assignedPage) {
        requireOriginalContext();
        const targets = normalizeDocumentMaintenanceTargets(
          await api.fetch(
            `/spaces/${space.id}/documents/${assignedPage.id}/maintenance/targets`,
          ),
        );
        requireOriginalContext();
        exportedCatalogs.push({
          server_base: base,
          account_id: account.id,
          space_id: space.id,
          targets,
        });
      }
      for (const doc of documents) {
        const [content, workflow, workflowEvents, properties, docTags] =
          await Promise.all([
            scopedClient.getDocumentContent(space.id, doc.id),
            scopedClient.getDocumentWorkflow(space.id, doc.id).catch(() => null),
            scopedClient.listDocumentWorkflowEvents(space.id, doc.id).catch(() => []),
            scopedClient.listDocumentProperties(space.id, doc.id).catch(() => []),
            scopedClient.listDocumentTags(space.id, doc.id).catch(() => []),
          ]);
        requireOriginalContext();
        if (content.published_source_digest !== undefined && (content.id !== doc.id || typeof content.published_source_digest !== "string" || !/^[0-9a-f]{64}$/.test(content.published_source_digest))) throw new Error("Published source response is unproven; no mixed source snapshot can be exported");
        snapshot.documents.push({
          id: doc.id,
          human_identifier: doc.human_identifier,
          maintenance_user_id: doc.maintenance_user_id,
          maintenance_group_id: doc.maintenance_group_id,
          space_id: space.id,
          title: content.title,
          content: content.content,
          ...(typeof content.published_source_digest === "string" && /^[0-9a-f]{64}$/.test(content.published_source_digest) ? {
            published_source: { schema_version: 1 as const, digest: content.published_source_digest, space_id: space.id, document_id: doc.id,
              server_context: { server_base: base, account_id: account.id, session_scope: scope }, replay_provider: exportedProvider },
          } : {}),
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
        snapshot.doc_tags?.push(
          ...docTags.map((tag) => ({ document_id: doc.id, tag_id: tag.id })),
        );
      }
    }

    // Never report an attachment-complete export from a metadata-only snapshot.
    if (snapshot.documents.some(document => portableAttachmentReferences(document.content,base).size > 0)) {
      throw new Error("Server export requires bounded exact attachment bytes; no archive was prepared. Download the selected versions or retry after supported archive export is available");
    }
    snapshot.maintenance_target_catalogs = exportedCatalogs;
    requireOriginalContext();
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
    if (!inLocalMode() || !canUseTauriNativeVaultArchive()) throw new Error("Vault archive import is only available in the desktop local workspace");
    const requireOriginal = capturePublishedProducerContext();
    const provider = getSyncProvider();
    const verifyCurrent = () => { requireOriginal(); if (!canUseTauriNativeVaultArchive() || localVault.provider !== provider) throw new Error("Original archive import context changed"); };
    if (!localVault.admitArchiveCanonicalSnapshot) throw new Error("Update the desktop shell before joint archive import");
    // Noncreating inventory is consumed BEFORE preparing another import. An old
    // pending intent is evidence for review, never permission to retry new bytes.
    const history = await this.listLocalVaultArchiveImports(); verifyCurrent();
    if (history.operations.some(row => row.canonical_committed !== true && !row.retained_abandoned)) throw new Error("A prior archive import needs original database evidence inspection before another import");
    const beforeCache = JSON.parse(JSON.stringify(buildSnapshot())) as LocalVaultSnapshot;
    const prepared = await invokeTauri<{ handle: string; snapshot_json: string }>("prepare_vault_archive_import", { provider, archivePath });
    verifyCurrent();
    if (!prepared || typeof prepared.handle !== "string" || !prepared.handle || typeof prepared.snapshot_json !== "string") throw new Error("Native shell did not capture a safe original archive");
    try {
      const original: unknown = JSON.parse(prepared.snapshot_json);
      // Shared raw/full typed admission is check-only: its lossy legacy defaults
      // must not replace the original JSON sealed in the native request.
      const validatedOriginal = prepareImportedSnapshot(original);
      await requireNativeReplayQueueCapability(validatedOriginal, provider);verifyCurrent();
      const plan = await invokeTauri<NativeArchivePlan>("plan_native_vault_archive_import", { provider, handle: prepared.handle, originalSnapshotJson: prepared.snapshot_json });
      verifyCurrent();
      assertNativeArchivePlan(plan, prepared, provider);
      assertLocalVaultSnapshot(original);
      const projected = prepareArchiveAssetReferences(original, plan.versions);
      const candidate = projected.snapshot;
      const destinationQueue = plan.destination_snapshot?.sync_state?.queued_operations ?? [];
      const queue = [...destinationQueue, ...(original.sync_state?.queued_operations ?? [])];
      const seen = new Map<string, VaultQueuedSyncOperation>();
      const merged: VaultQueuedSyncOperation[] = [];
      const affected = new Set(projected.changes.map(change => original[change.collection]?.[change.index]?.id));
      for (const operation of queue) {
        const payload = operation.payload;
        const hasBody = !!payload && typeof payload === "object" && !Array.isArray(payload) && ["content", "body", "cover_image"].some(field => Object.prototype.hasOwnProperty.call(payload, field));
        if (affected.has(operation.entity_id) && (["update_content", "resolve_conflict"].includes(operation.action) || hasBody)) throw new Error("Queued body or source depends on remapped archive references; retain the original queue and archive");
        const prior = seen.get(operation.id);
        if (prior) { if (!nativeJsonValueEquals(prior, operation)) throw new Error("Archive and destination queue identities conflict; retain both original queues"); continue; }
        seen.set(operation.id, operation); merged.push(operation);
      }
      if (merged.length || original.sync_state !== undefined) {
        // Only the complete queue union is changed; raw import state/provenance
        // and operation payloads stay authored. No current account is adopted.
        candidate.sync_state = { ...original.sync_state, queued_operations: merged } as LocalVaultSnapshot["sync_state"];
      }
      prepareImportedSnapshot(candidate);
      verifyCurrent();
      const request = JSON.parse(JSON.stringify({ plan, candidate, reference_changes: projected.changes }));
      let result: NativeArchiveReceipt;
      try {
        result = await invokeTauri<NativeArchiveReceipt>("commit_native_vault_archive_import", { provider, handle: prepared.handle, planDigest: plan.plan_digest, candidate: request.candidate, referenceChanges: request.reference_changes });
        verifyCurrent();
        assertNativeArchiveReceipt(result, plan, request);
      } catch {
        // Only the ORIGINAL database and full captured request can acknowledge
        // this operation. No dispatch/reinstallation or latest snapshot rebasing.
        verifyCurrent();
        let cursor: string | null = null;
        let matched: NativeArchiveInventoryRow | undefined;
        do {
          const page: NativeArchiveInventory = await invokeTauri<NativeArchiveInventory>("list_native_vault_archive_imports", { provider, binding: plan.binding, cursor, limit: 20 });
          verifyCurrent(); assertNativeArchiveInventory(page, plan.binding);
          matched = page.operations.find(row => row.operation_id === plan.operation_id && nativeJsonValueEquals(row.intent, request));
          cursor = page.next_cursor;
        } while (!matched && cursor);
        if (!matched || !matched.canonical_committed) throw new Error("Archive outcome is unknown; retain its original operation and database intent for inspection");
        const inspected = await invokeTauri<NativeArchiveInspection>("inspect_native_vault_archive_import", { provider, binding: plan.binding, operationId: plan.operation_id, requestFingerprint: matched.request_fingerprint });
        verifyCurrent();
        if (!nativeJsonValueEquals(inspected.intent, request) || !inspected.receipt) throw new Error("Original archive request evidence is unavailable");
        result = { ...inspected.receipt, current_snapshot: inspected.current_snapshot };
        assertNativeArchiveReceipt(result, plan, request);
      }
      // Admit the separately bound CURRENT original DB snapshot, including later
      // B. The historical A receipt never becomes a current editing baseline.
      const current = result.current_snapshot;
      assertLocalVaultSnapshot(current); prepareImportedSnapshot(current); verifyCurrent();
      await localVault.admitArchiveCanonicalSnapshot(current, verifyCurrent);
      verifyCurrent();
      publishLocalReviewedMoveSnapshot(current, beforeCache);
      hydratedLocalProvider = provider; localInitialized = true;
    } finally {
      await invokeTauri<void>("cancel_vault_archive_import", { provider, handle: prepared.handle }).catch(() => {});
    }
  },

  async listLocalVaultArchiveImports(): Promise<NativeArchiveInventory> {
    if (!inLocalMode() || !canUseTauriNativeVaultArchive()) throw new Error("Archive evidence inspection requires the desktop local workspace");
    const verify = capturePublishedProducerContext(), provider = getSyncProvider();
    const first = await invokeTauri<NativeArchiveInventory>("list_native_vault_archive_imports", { provider, binding: null, cursor: null, limit: 20 });
    verify(); assertNativeArchiveInventory(first);
    const result = { ...first, operations: [...first.operations] };
    while (result.next_cursor) {
      const page: NativeArchiveInventory = await invokeTauri<NativeArchiveInventory>("list_native_vault_archive_imports", { provider, binding: first.binding, cursor: result.next_cursor, limit: 20 });
      verify(); assertNativeArchiveInventory(page, first.binding);
      result.operations.push(...page.operations); result.next_cursor = page.next_cursor;
    }
    return result;
  },

  async inspectLocalVaultArchiveImport(binding: NativeArchiveBinding, operationId: string, requestFingerprint: string): Promise<NativeArchiveInspection> {
    if (!inLocalMode() || !canUseTauriNativeVaultArchive()) throw new Error("Archive evidence inspection requires the desktop local workspace");
    const verify = capturePublishedProducerContext(), provider = getSyncProvider();
    if (binding.provider !== provider) throw new Error("Archive evidence belongs to its original provider");
    const result = await invokeTauri<NativeArchiveInspection>("inspect_native_vault_archive_import", { provider, binding, operationId, requestFingerprint });
    verify();
    if (!result || !nativeJsonValueEquals(result.binding, binding) || result.operation_id !== operationId || result.request_fingerprint !== requestFingerprint || !result.intent) throw new Error("Original archive evidence is unavailable");
    // Inspection is read-only; it does not replace the editor/cache or retry SQL.
    return result;
  },

  async abandonLocalVaultArchiveImport(binding: NativeArchiveBinding, operationId: string, requestFingerprint: string): Promise<void> {
    if (!inLocalMode() || !canUseTauriNativeVaultArchive()) throw new Error("Archive recovery requires its original desktop workspace");
    const verify = capturePublishedProducerContext(), provider = getSyncProvider();
    if (binding.provider !== provider) throw new Error("Archive evidence belongs to its original provider");
    const result = await invokeTauri<{ binding: NativeArchiveBinding; operation_id: string; request_fingerprint: string; retained_abandoned: boolean; canonical_committed: boolean; evidence_retained: boolean }>("abandon_native_vault_archive_import", { provider, binding, operationId, requestFingerprint });
    verify();
    if (!result || !nativeJsonValueEquals(result.binding, binding) || result.operation_id !== operationId || result.request_fingerprint !== requestFingerprint || result.retained_abandoned !== true || result.canonical_committed !== false || result.evidence_retained !== true) throw new Error("Archive abandonment acknowledgement unknown; inspect the original operation again");
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

  /** Optional live published View context; never an editing/source prerequisite. */
  async getCurrentDocumentPublishedView(spaceId:string,docId:string,selection:DraftSaveSelection):Promise<{metadata:DocumentRecord;content:DocumentContentRecord}> {
    if(selection.mode!=="server"||!selection.origin)throw new Error("Current published View is unavailable.");
    verifyDraftSaveSelection(selection);const context=captureOfflineServerContext();
    if(!sameOfflineReplayOrigin(context.origin,selection.origin))throw new Error("Published View context changed.");
    const verify=()=>{verifyDraftSaveSelection(selection);verifyOfflineReplayContext(context,getApiBase(),api.getToken());};
    const read=async(path:string)=>{verify();const response=await fetch(context.origin.serverBase+path,{cache:"no-store",headers:{Authorization:`Bearer ${context.bearer}`}});verify();if(!response.ok)throw new ApiError("Current published View is unavailable.",response.status);const value=await response.json();verify();return value;};
    const path=`/spaces/${encodeURIComponent(spaceId)}/documents/${encodeURIComponent(docId)}`;
    const [metadata,content]=await Promise.all([read(path),read(path+"/content")]);verify();
    if(!metadata||metadata.id!==docId||metadata.space_id!==spaceId||typeof metadata.title!=="string"||typeof metadata.locked!=="boolean"||metadata.page_edit_allowed!==undefined&&typeof metadata.page_edit_allowed!=="boolean"||metadata.locked_by!==undefined&&metadata.locked_by!==null&&typeof metadata.locked_by!=="string"||!content||content.id!==docId||typeof content.title!=="string"||typeof content.content!=="string")throw new Error("Current published View response is unverified.");
    return {metadata,content};
  },

  /** Exact editable selected source; does not require or reveal published View. */
  async getCurrentDocumentDraftEditSource(spaceId:string,docId:string,draftId:string,selection:DraftSaveSelection,expected?:DraftRecord):Promise<DocumentDraftEditSource> {
    if (selection.mode !== "server" || !selection.origin) throw new Error("Current selected draft source is unavailable in local mode.");
    verifyDraftSaveSelection(selection); const context=captureOfflineServerContext();
    if (!sameOfflineReplayOrigin(context.origin,selection.origin)) throw new Error("Selected draft source context changed.");
    const verify=()=>{verifyDraftSaveSelection(selection);verifyOfflineReplayContext(context,getApiBase(),api.getToken());}; verify();
    const response=await fetch(context.origin.serverBase+`/spaces/${encodeURIComponent(spaceId)}/documents/${encodeURIComponent(docId)}/drafts/${encodeURIComponent(draftId)}/save-source`,{cache:"no-store",headers:{Authorization:`Bearer ${context.bearer}`}});
    verify();if(!response.ok)throw new ApiError("Current selected draft Edit source is unavailable. Keep your edits and review access.",response.status);
    const source=await response.json() as DocumentDraftEditSource;verify();
    const draft=source?.selected_draft,fields=source?.draft;
    if(!source||source.schema_version!==1||source.space_id!==spaceId||source.document_id!==docId||source.draft_id!==draftId||!validDraftSourceDigest(source.draft_source_digest)||typeof source.locked!=="boolean"||source.locked_by!==null&&typeof source.locked_by!=="string"||source.page_edit_allowed!==true||typeof source.page_published!=="boolean"||typeof source.page_updated_at!=="string"||!source.page_updated_at||!draft||draft.id!==draftId||draft.space_id!==spaceId||draft.document_id!==docId||draft.source_digest!==source.draft_source_digest||!["owner","editor"].includes(draft.access_role)||typeof draft.owner_id!=="string"||typeof draft.owner_name!=="string"||typeof draft.can_publish!=="boolean"||draft.can_publish&&draft.access_role!=="owner"||draft.access_role==="owner"&&draft.owner_id!==context.origin.accountId||typeof draft.created_at!=="string"||typeof draft.updated_at!=="string"||typeof draft.base_updated_at!=="string"||!fields||typeof fields.title!=="string"||typeof fields.content!=="string"||fields.icon!==null&&typeof fields.icon!=="string"||fields.cover_image!==null&&typeof fields.cover_image!=="string"||draft.title!==fields.title||draft.content!==fields.content||(draft.icon??null)!==fields.icon||(draft.cover_image??null)!==fields.cover_image||draft.document_title!==fields.title)throw new Error("Selected draft Edit source is unverified. Retain the original draft and review access.");
    if(expected&&(expected.id!==draft.id||expected.source_digest!==draft.source_digest||expected.title!==draft.title||expected.content!==draft.content||(expected.icon??null)!==(draft.icon??null)||(expected.cover_image??null)!==(draft.cover_image??null)))throw new DraftSaveHeldError("source_conflict","Selected draft changed before current Edit review. Keep your edits; no source was replaced.");
    return source;
  },

  /** Uncached current ordinary Edit projection, used only to admit the locked-owner exception. */
  async getCurrentDocumentEditProjection(spaceId:string, docId:string, selection:DraftSaveSelection):Promise<DocumentRecord> {
    if (selection.mode !== "server" || !selection.origin) throw new Error("Current server Edit projection is unavailable in local mode.");
    verifyDraftSaveSelection(selection);
    const context = captureOfflineServerContext();
    if (!sameOfflineReplayOrigin(context.origin,selection.origin)) throw new Error("Page Edit selection changed.");
    const verify = () => { verifyDraftSaveSelection(selection); verifyOfflineReplayContext(context,getApiBase(),api.getToken()); };
    verify();
    const response = await fetch(context.origin.serverBase + `/spaces/${encodeURIComponent(spaceId)}/documents/${encodeURIComponent(docId)}`, {cache:"no-store",headers:{Authorization:`Bearer ${context.bearer}`}});
    verify();
    if (!response.ok) throw new ApiError("Current Edit access could not be verified. Keep your edits and refresh this page.",response.status);
    const page = await response.json() as DocumentRecord;
    verify();
    if (!page || page.id !== docId || page.space_id !== spaceId || typeof page.page_edit_allowed !== "boolean" || typeof page.locked !== "boolean" || page.locked_by !== undefined && page.locked_by !== null && typeof page.locked_by !== "string") throw new Error("Current page Edit projection is unverified. Keep your edits and refresh this page.");
    return page;
  },

  async getDocument(spaceId: string, docId: string): Promise<DocumentRecord> {
    if (!inLocalMode()) {
      return api.fetch<DocumentRecord>(`/spaces/${spaceId}/documents/${docId}`);
    }
    await ensureLocalData();
    const doc = requireLocalDoc(spaceId, docId);
    if (doc.deleted_at) throw new Error("Document not found");
    return {
      id: doc.id,
      human_identifier: doc.human_identifier,
      maintenance_user_id: doc.maintenance_user_id,
      maintenance_group_id: doc.maintenance_group_id,
      identifier_edit_allowed: !doc.locked && !doc.archived_at && !doc.deleted_at,
      maintenance_edit_allowed: !doc.locked && !doc.archived_at && !doc.deleted_at,
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
      published_at: doc.published_at,
      space_id: doc.space_id,
    };
  },

  async listDocumentDrafts(spaceId: string, documentId?: string): Promise<DraftSummaryRecord[]> {
    if (!inLocalMode()) {
      const params = documentId
        ? `?${new URLSearchParams({ document_id: documentId }).toString()}`
        : "";
      return api.fetch<DraftSummaryRecord[]>(`/spaces/${spaceId}/drafts${params}`);
    }
    await ensureLocalData();
    if (purgeExpiredLocalDrafts()) await persistLocalSnapshot();
    return localDrafts
      .filter((draft) => draft.space_id === spaceId)
      .filter((draft) => !documentId || draft.document_id === documentId)
      .sort((left, right) => right.updated_at.localeCompare(left.updated_at))
      .map((draft) => ({
        id: draft.id,
        document_id: draft.document_id,
        space_id: draft.space_id,
        owner_id: draft.owner_id,
        owner_name: LOCAL_SYSTEM_USER,
        document_title: localDocs.get(draft.document_id)?.title ?? draft.title,
        title: draft.title,
        access_role: "owner",
        can_publish: true,
        created_at: draft.created_at,
        updated_at: draft.updated_at,
      }));
  },

  async purgeDocumentDrafts(spaceId: string): Promise<number> {
    if (!inLocalMode()) {
      return api.requestDraftPurge(spaceId);
    }
    await ensureLocalData();
    const before = localDrafts.length;
    localDrafts = localDrafts.filter((draft) => draft.space_id !== spaceId);
    await persistLocalSnapshot();
    return before - localDrafts.length;
  },

  inspectDocumentDraftBirth(spaceId: string, documentId: string, operationId: string) {
    if (inLocalMode()) return null;
    return inspectServerDraftBirth(spaceId, documentId, operationId);
  },

  async retryDocumentDraftBirth(spaceId: string, documentId: string, operationId: string): Promise<DraftRecord> {
    if (inLocalMode()) throw new Error("Review the original server draft request in its server session");
    return createServerDraftBirth(spaceId, documentId, {}, operationId);
  },

  async createDocumentDraft(
    spaceId: string,
    documentId: string,
    options: DraftBirthOptions = {},
    onPreparedOperationId?: (operationId: string) => void,
  ): Promise<DraftRecord> {
    if (!inLocalMode()) return createServerDraftBirth(spaceId, documentId, options, undefined, onPreparedOperationId);
    await ensureLocalData();
    const document = requireLocalDoc(spaceId, documentId);
    const existing = !options.force_new
      ? localDrafts.find(
          (draft) =>
            draft.space_id === spaceId &&
            draft.document_id === documentId &&
            draft.owner_id === LOCAL_SYSTEM_USER_ID,
        )
      : undefined;
    const draft =
      existing ??
      ({
        id: makeId("draft"),
        document_id: documentId,
        space_id: spaceId,
        owner_id: LOCAL_SYSTEM_USER_ID,
        title: document.title,
        content: document.content,
        icon: document.icon,
        cover_image: document.cover_image,
        base_updated_at: document.updated_at,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        previous_folder_id: document.folder_id,
        previous_parent_id: document.parent_id,
      } satisfies VaultDocumentDraft);
    if (!existing) localDrafts.push(draft);
    await persistLocalSnapshot();
    return {
      ...draft,
      owner_name: LOCAL_SYSTEM_USER,
      document_title: document.title,
      access_role: "owner",
      can_publish: true,
    };
  },

  async getDocumentDraft(
    spaceId: string,
    documentId: string,
    draftId: string,
  ): Promise<DraftRecord> {
    if (!inLocalMode()) {
      return api.fetch<DraftRecord>(`/spaces/${spaceId}/documents/${documentId}/drafts/${draftId}`);
    }
    await ensureLocalData();
    const draft = localDrafts.find(
      (item) => item.id === draftId && item.space_id === spaceId && item.document_id === documentId,
    );
    if (!draft) throw new Error("Draft not found");
    return {
      ...draft,
      owner_name: LOCAL_SYSTEM_USER,
      document_title: localDocs.get(documentId)?.title ?? draft.title,
      access_role: "owner",
      can_publish: true,
    };
  },

  captureDraftSaveSelection,
  verifyDraftSaveSelection,
  get requiresDraftSourceCas(): boolean { return !inLocalMode(); },

  async updateDocumentDraft(
    spaceId: string,
    documentId: string,
    draftId: string,
    patch: { title?: string; icon?: string; cover_image?: string; content?: string },
    sourceDigest?: string,
  ): Promise<DraftRecord> {
    if (!inLocalMode()) {
      if (!validDraftSourceDigest(sourceDigest)) throw new DraftSaveHeldError("source_required", "This client needs an update before saving this draft. Your edits remain on this device.");
      const body = { ...patch, source_digest: sourceDigest };
      let originalContext: OfflineReplayContext | null = null;
      try { originalContext = captureOfflineServerContext(); } catch { /* Opaque online identity cannot create new replay authority. */ }
      const updated = await api.fetch<DraftRecord>(`/spaces/${spaceId}/documents/${documentId}/drafts/${draftId}`, { method: "PUT", body, expectedStatuses: [200] }).catch((error: unknown) => {
        if (isApiError(error) && error.status === 409) throw new DraftSaveHeldError(error.message.startsWith("source_required") ? "source_required" : "source_conflict", error.message.startsWith("source_required") ? "This client needs an update before saving this draft. Your edits remain on this device." : "The draft changed. Compare current and saved edits before saving again.");
        // Missing drafts never trigger creation/retry under a different identity.
        throw new DraftSaveHeldError("outcome_unknown", isApiError(error) && error.status === 404 ? "This draft is unavailable. Keep your edits and review the page before saving again." : "Could not confirm this draft save. Keep your edits and review its current state before saving again.");
      });
      if (!updated) throw new DraftSaveHeldError("queued", "Draft edit retained on this device. Reconnect and review its outcome before another save.");
      if (updated.id !== draftId || updated.document_id !== documentId || updated.space_id !== spaceId || !validDraftSourceDigest(updated.source_digest)) {
        if (originalContext) queueOfflineServerMutation(originalContext.origin.serverBase, `/spaces/${spaceId}/documents/${documentId}/drafts/${draftId}`, "PUT", body, originalContext.origin.cacheScope, originalContext.origin, "unknown");
        throw new DraftSaveHeldError("outcome_unknown", "Draft save acknowledgement is unproven; retain this edit and review before saving again.");
      }
      return updated;
    }
    await ensureLocalData();
    const index = localDrafts.findIndex((draft) => draft.id === draftId);
    if (index < 0) throw new Error("Draft not found");
    if (patch.content !== undefined) assertContentSize(patch.content);
    localDrafts[index] = {
      ...localDrafts[index],
      ...patch,
      title: patch.title ? normalizeTitle(patch.title) : localDrafts[index].title,
      updated_at: new Date().toISOString(),
    };
    await persistLocalSnapshot();
    return this.getDocumentDraft(spaceId, documentId, draftId);
  },

  async preparePublicationSnapshotRecovery(spaceId: string, documentId: string, selected: PublicationSnapshotDetail, selection: DraftSaveSelection) {
    if (inLocalMode() || !selection.origin) throw new Error("Snapshot recovery requires an authenticated server page.");
    verifyDraftSaveSelection(selection);
    const intent = await preparePublicationSnapshotRecovery(spaceId, documentId, selected, selection.origin);
    verifyDraftSaveSelection(selection);
    return intent;
  },
  recoverPreparedPublicationSnapshot(intent: SnapshotRecoveryIntent) { return recoverPreparedPublicationSnapshot(intent); },
  retryPublicationSnapshotRecovery(operationId: string) { return retryPublicationSnapshotRecovery(operationId); },
  inspectPublicationSnapshotRecoveries(spaceId: string, documentId: string) { return inspectPublicationSnapshotRecoveries(spaceId, documentId); },
  cancelPreparedPublicationSnapshotRecovery(operationId: string) { return cancelPreparedPublicationSnapshotRecovery(operationId); },

  async prepareDocumentDraftPublication(spaceId: string, documentId: string, saved: DraftRecord, selection: DraftSaveSelection) {
    if (inLocalMode() || !selection.origin) throw new Error("Source-bound publication requires a server draft.");
    verifyDraftSaveSelection(selection);
    const intent = await prepareDocumentDraftPublication(spaceId, documentId, saved, selection.origin);
    verifyDraftSaveSelection(selection);
    return intent;
  },
  publishPreparedDocumentDraft(intent: DraftPublicationIntent) { return publishPreparedDocumentDraft(intent); },
  retryDocumentDraftPublication(operationId: string) { return retryDocumentDraftPublication(operationId); },
  inspectDocumentDraftPublications(spaceId: string, documentId: string) { return inspectDocumentDraftPublications(spaceId, documentId); },
  cancelPreparedDocumentDraftPublication(operationId: string) { return cancelPreparedDocumentDraftPublication(operationId); },

  async publishDocumentDraft(
    spaceId: string,
    documentId: string,
    draftId: string,
  ): Promise<DocumentRecord> {
    if (!inLocalMode()) {
      throw new Error("Review and dispatch an original source-bound publication intent before publishing a server draft.");
    }
    await ensureLocalData();
    const draft = await this.getDocumentDraft(spaceId, documentId, draftId);
    const document = requireLocalDoc(spaceId, documentId);
    const publishedAt = new Date().toISOString();
    localDocs.set(documentId, {
      ...document,
      title: draft.title,
      content: stripTemplateGuidance(draft.content),
      icon: draft.icon,
      cover_image: draft.cover_image,
      last_updated_by: LOCAL_SYSTEM_USER,
      updated_at: publishedAt,
      published_at: publishedAt,
    });
    localDrafts = localDrafts.filter((item) => item.id !== draftId);
    await persistLocalSnapshot();
    return this.getDocument(spaceId, documentId);
  },

  async deleteDocumentDraft(spaceId: string, documentId: string, draftId: string): Promise<void> {
    if (!inLocalMode()) {
      await api.requestDraftOwner(`/spaces/${spaceId}/documents/${documentId}/drafts/${draftId}`, "DELETE");
      return;
    }
    await ensureLocalData();
    localDrafts = localDrafts.filter((draft) => draft.id !== draftId);
    await persistLocalSnapshot();
  },

  async listDocumentDraftAccess(
    spaceId: string,
    documentId: string,
    draftId: string,
  ): Promise<DraftAccessRecord[]> {
    if (inLocalMode()) return [];
    return api.requestDraftOwner<DraftAccessRecord[]>(
      `/spaces/${spaceId}/documents/${documentId}/drafts/${draftId}/access`, "GET",
    );
  },

  async listDocumentDraftAccessCandidates(
    spaceId: string,
    documentId: string,
    draftId: string,
  ): Promise<DraftAccessCandidateRecord[]> {
    if (inLocalMode()) return [];
    return api.requestDraftOwner<DraftAccessCandidateRecord[]>(
      `/spaces/${spaceId}/documents/${documentId}/drafts/${draftId}/access-candidates`, "GET",
    );
  },

  async grantDocumentDraftAccess(
    spaceId: string,
    documentId: string,
    draftId: string,
    userId: string,
    role: "viewer" | "editor",
  ): Promise<void> {
    if (inLocalMode()) throw new Error("Draft sharing requires a server workspace");
    await api.requestDraftOwner(
      `/spaces/${spaceId}/documents/${documentId}/drafts/${draftId}/access/${userId}`, "PUT", { role },
    );
  },

  async revokeDocumentDraftAccess(
    spaceId: string,
    documentId: string,
    draftId: string,
    userId: string,
  ): Promise<void> {
    if (inLocalMode()) return;
    await api.requestDraftOwner(
      `/spaces/${spaceId}/documents/${documentId}/drafts/${draftId}/access/${userId}`, "DELETE",
    );
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
    patch: {
      title?: string;
      icon?: string;
      cover_image?: string | null;
      folder_id?: string | null;
      parent_id?: string | null;
    },
    draftContext?: LocalMetadataDraftContext,
  ): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}`, { method: "PUT", body: patch });
      return;
    }
    await ensureLocalData();
    const intended: LocalMetadataPatch = {};
    for (const field of Object.keys(patch) as LocalMetadataField[]) {
      const value = patch[field];
      intended[field] = field === "title" ? normalizeTitle(value || "Untitled") : field === "icon" ? value?.trim() || null : value ?? null;
    }
    await commitLocalDocumentMetadata(spaceId, docId, intended, draftContext);
  },

  async moveDocument(spaceId: string, docId: string, input: MoveDocumentInput): Promise<void> {
    if (!inLocalMode()) {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/move`, { method: "POST", body: input });
      return;
    }
    await ensureLocalData();
    requireLocalSpace(spaceId);
    requireLocalSpace(input.target_space_id);
    if (input.target_space_id !== spaceId) throw new Error(CROSS_SPACE_MOVE_REVIEW_REQUIRED);
    await commitLocalDocumentMetadata(spaceId, docId, { folder_id: input.folder_id ?? null, parent_id: input.parent_id ?? null });
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
    const requireOriginalContext = capturePublishedProducerContext();
    const originalAdapter = createLocalVaultAdapter(getSyncProvider());
    await ensureLocalData();
    requireOriginalContext();
    const doc = requireLocalDoc(spaceId, docId);
    const carriesPublishedSource = doc.published_source !== undefined;
    if (carriesPublishedSource && hydratedLocalProvider !== getSyncProvider()) throw new Error("Import the selected provider before editing its published source");
    assertContentSize(content);
    const updated = {
      ...doc,
      content,
      last_updated_by: LOCAL_SYSTEM_USER,
      updated_at: new Date().toISOString(),
    };
    const operation = originalPublishedContentOperation(doc, updated, "update_content");
    if (operation.server_context) await requireNativeReplayQueueCapability(buildSnapshot(), getSyncProvider(), true);
    requireOriginalContext();
    if (localDocs.get(docId) !== doc) throw new Error("Page changed while edit waited; retain this input for review.");
    localDocs.set(docId, updated);
    await persistLocalSnapshot(operation, requireOriginalContext, carriesPublishedSource ? originalAdapter : localVault);
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

  async getPendingDocumentIdentifier(spaceId: string, docId: string): Promise<{
    operation_id: string;
    human_identifier: string | null;
    source: DocumentIdentifierSource;
    replay_provider: SyncProviderKind;
    pending_error?: string;
  } | null> {
    const serverMode = !inLocalMode();
    if (!serverMode && getSyncProvider() === "none") return null;
    let context: VaultQueuedSyncOperation["server_context"];
    if (serverMode) {
      const token = api.getToken();
      if (!token) return null;
      const credentialAccount = identifierCredentialAccount(token);
      const base = getApiBase();
      const scope = serverCacheScopeForSession(token, api.getRefreshToken());
      const account = await api.fetch<{ id: string }>("/auth/me");
      if (!account.id || account.id !== credentialAccount || getApiBase() !== base || api.getToken() !== token || serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== scope)
        throw new Error("Server account changed while loading pending identifier recovery");
      context = { server_base: base, account_id: account.id, session_scope: scope };
    }
    const snapshot = await createLocalVaultAdapter(localVault.provider).loadSnapshot();
    const pending = (snapshot.sync_state?.queued_operations ?? []).filter((operation) => operation.entity_type === "document" && operation.entity_id === docId && operation.action === "update_identifier" && (serverMode ? identifierContextsEqual(operation.server_context, context) : !operation.server_context));
    if (pending.length > 1) throw new Error("Multiple identifier recovery operations require review before editing");
    const operation = pending[0];
    if (!operation) return null;
    const intent = parseDocumentIdentifierIntent(operation.payload);
    if (intent.id !== docId || intent.space_id !== spaceId || !operation.replay_provider)
      throw new Error("Identifier recovery binding is invalid; retain the operation for review");
    if (context && (getApiBase() !== context.server_base || identifierCredentialAccount(api.getToken()) !== context.account_id || serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== context.session_scope))
      throw new Error("Server account changed before pending identifier recovery could be shown");
    const providerMismatch = operation.replay_provider !== getSyncProvider();
    return {
      operation_id: operation.id, human_identifier: intent.human_identifier, source: intent.source,
      replay_provider: operation.replay_provider,
      ...(providerMismatch ? { pending_error: "Restore the original sync provider before retrying this identifier operation" } : operation.last_error ? { pending_error: operation.last_error } : {}),
    };
  },

  async setDocumentIdentifier(spaceId: string, docId: string, humanIdentifier: string | null, source?: DocumentIdentifierSource): Promise<{ operation_id: string; disposition: "queued" | "applied" | "historical"; pending_error?: string }> {
    if (!inLocalMode()) return commitServerDocumentIdentifier(spaceId, docId, humanIdentifier, source);
    await ensureLocalData();
    const result = await commitLocalDocumentIdentifier(spaceId, docId, humanIdentifier, undefined, source);
    return { operation_id: result.operation_id, disposition: getSyncProvider() === "none" ? "applied" : "queued" };
  },

  async getDocumentMaintenanceTargets(
    spaceId: string,
    docId: string,
  ): Promise<DocumentMaintenanceTargets> {
    if (inLocalMode()) {
      await ensureLocalData();
      requireLocalDoc(spaceId, docId);
      const snapshot = await createLocalVaultAdapter(localVault.provider).loadSnapshot();
      const catalogs = normalizeDocumentMaintenanceCatalogs(
        snapshot.maintenance_target_catalogs,
      ).filter((record) => record.space_id === spaceId);
      if (catalogs.length > 1)
        throw new Error("Maintenance target catalog provenance is ambiguous");
      return (
        catalogs[0]?.targets ?? {
          users: [{ id: LOCAL_SYSTEM_USER_ID, name: LOCAL_SYSTEM_USER, active: true }],
          groups: [],
          groups_available: false,
        }
      );
    }
    const token = api.getToken(),
      base = getApiBase(),
      scope = serverCacheScopeForSession(token, api.getRefreshToken());
    const subject = identifierCredentialAccount(token);
    const account = await api.fetch<{ id: string }>("/auth/me");
    if (
      account.id !== subject ||
      api.getToken() !== token ||
      getApiBase() !== base ||
      serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== scope
    )
      throw new Error("Server account changed while loading maintenance targets");
    const targets = normalizeDocumentMaintenanceTargets(
      await api.fetch<unknown>(
        `/spaces/${spaceId}/documents/${docId}/maintenance/targets`,
      ),
    );
    if (
      api.getToken() !== token ||
      getApiBase() !== base ||
      serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== scope
    )
      throw new Error(
        "Server account changed before maintenance targets could be retained",
      );
    const requireOriginalAccount = () => {
      if (
        api.getToken() !== token ||
        getApiBase() !== base ||
        serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== scope ||
        identifierCredentialAccount(api.getToken()) !== account.id
      )
        throw new Error("Server account changed while retaining maintenance targets");
    };
    const adapter = createLocalVaultAdapter(localVault.provider);
    await adapter.commitSnapshotMutation((snapshot) => {
      requireOriginalAccount();
      const catalogs = normalizeDocumentMaintenanceCatalogs(
        snapshot.maintenance_target_catalogs,
      );
      snapshot.maintenance_target_catalogs = [
        ...catalogs.filter(
          (record) =>
            !(
              record.server_base === base &&
              record.account_id === account.id &&
              record.space_id === spaceId
            ),
        ),
        { server_base: base, account_id: account.id, space_id: spaceId, targets },
      ];
      return { snapshot };
    });
    requireOriginalAccount();
    return targets;
  },
  async getPendingDocumentMaintenance(
    spaceId: string,
    docId: string,
  ): Promise<{
    operation_id: string;
    maintenance: DocumentMaintenance;
    source: DocumentMaintenanceSource;
    replay_provider: SyncProviderKind;
    pending_error?: string;
  } | null> {
    const serverMode = !inLocalMode();
    if (!serverMode && getSyncProvider() === "none") return null;
    let context: VaultQueuedSyncOperation["server_context"];
    if (serverMode) {
      const token = api.getToken();
      if (!token) return null;
      const credentialAccount = identifierCredentialAccount(token);
      const base = getApiBase();
      const scope = serverCacheScopeForSession(token, api.getRefreshToken());
      const account = await api.fetch<{ id: string }>("/auth/me");
      if (
        !account.id ||
        account.id !== credentialAccount ||
        getApiBase() !== base ||
        api.getToken() !== token ||
        serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !== scope
      )
        throw new Error(
          "Server account changed while loading pending maintenance recovery",
        );
      context = { server_base: base, account_id: account.id, session_scope: scope };
    }
    const snapshot = await createLocalVaultAdapter(localVault.provider).loadSnapshot();
    const pending = (snapshot.sync_state?.queued_operations ?? []).filter(
      (operation) =>
        operation.entity_type === "document" &&
        operation.entity_id === docId &&
        operation.action === "update_maintenance" &&
        (serverMode
          ? identifierContextsEqual(operation.server_context, context)
          : !operation.server_context),
    );
    if (pending.length > 1)
      throw new Error(
        "Multiple maintenance recovery operations require review before editing",
      );
    const operation = pending[0];
    if (!operation) return null;
    const intent = parseDocumentMaintenanceIntent(operation.payload);
    if (intent.id !== docId || intent.space_id !== spaceId || !operation.replay_provider)
      throw new Error(
        "Maintenance recovery binding is invalid; retain the operation for review",
      );
    if (
      context &&
      (getApiBase() !== context.server_base ||
        identifierCredentialAccount(api.getToken()) !== context.account_id ||
        serverCacheScopeForSession(api.getToken(), api.getRefreshToken()) !==
          context.session_scope)
    )
      throw new Error(
        "Server account changed before pending maintenance recovery could be shown",
      );
    const providerMismatch = operation.replay_provider !== getSyncProvider();
    return {
      operation_id: operation.id,
      maintenance: intent.maintenance,
      source: intent.source,
      replay_provider: operation.replay_provider,
      ...(providerMismatch
        ? {
            pending_error:
              "Restore the original sync provider before retrying this maintenance operation",
          }
        : operation.last_error
          ? { pending_error: operation.last_error }
          : {}),
    };
  },

  async setDocumentMaintenance(
    spaceId: string,
    docId: string,
    maintenance: DocumentMaintenance,
    source: DocumentMaintenanceSource,
  ): Promise<{
    operation_id: string;
    disposition: "queued" | "applied" | "historical";
    pending_error?: string;
  }> {
    if (!inLocalMode())
      return commitServerDocumentMaintenance(spaceId, docId, maintenance, source);
    await ensureLocalData();
    const result = await commitLocalDocumentMaintenance(
      spaceId,
      docId,
      maintenance,
      undefined,
      source,
    );
    return {
      operation_id: result.operation_id,
      disposition: getSyncProvider() === "none" ? "applied" : "queued",
    };
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
    if (normalizedFilters.is_draft === "true") {
      return localDrafts
        .filter(
          (draft) => !normalizedFilters.space_id || draft.space_id === normalizedFilters.space_id,
        )
        .filter((draft) => localDocMatchesSearchFilters(draft.document_id, normalizedFilters))
        .filter((draft) => {
          if (!needle) return true;
          return `${draft.title}\n${draft.content}`.toLowerCase().includes(needle);
        })
        .sort((left, right) => right.updated_at.localeCompare(left.updated_at))
        .slice(0, limit)
        .map((draft) => ({
          document_id: draft.document_id,
          human_identifier: localDocs.get(draft.document_id)?.human_identifier,
          maintenance_user_id: localDocs.get(draft.document_id)?.maintenance_user_id,
          maintenance_group_id: localDocs.get(draft.document_id)?.maintenance_group_id,
          draft_id: draft.id,
          is_draft: true,
          title: draft.title,
          space_id: draft.space_id,
          space_name: spaceById.get(draft.space_id) ?? PERSONAL_SPACE_NAME,
          excerpt: draft.content.replace(/\s+/g, " ").trim().slice(0, 180),
          updated_at: draft.updated_at,
          author: LOCAL_SYSTEM_USER,
          author_id: LOCAL_SYSTEM_USER_ID,
        }));
    }
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
        human_identifier: localDocs.get(entry.document_id)?.human_identifier,
          maintenance_user_id: localDocs.get(entry.document_id)?.maintenance_user_id,
          maintenance_group_id: localDocs.get(entry.document_id)?.maintenance_group_id,
        title: entry.title,
        space_id: entry.space_id,
        space_name: spaceById.get(entry.space_id) ?? PERSONAL_SPACE_NAME,
        excerpt: entry.content.slice(0, 180).trim(),
        updated_at: entry.updated_at,
        author: "Local User",
        author_id: LOCAL_SYSTEM_USER_ID,
        folder_id: localDocs.get(entry.document_id)?.folder_id,
        folder_name: localFolders.get(localDocs.get(entry.document_id)?.folder_id ?? "")?.name,
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
        human_identifier: doc.human_identifier,
        maintenance_user_id: doc.maintenance_user_id,
        maintenance_group_id: doc.maintenance_group_id,
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
      human_identifier: doc.human_identifier,
      maintenance_user_id: doc.maintenance_user_id,
      maintenance_group_id: doc.maintenance_group_id,
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
    if (!inLocalMode())
      return api.fetch<TotpSetupResponse>("/auth/2fa/totp/setup", { method: "POST" });
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
    if (Array.from(localDocs.values()).some((doc) => doc.published_source !== undefined)) {
      const requireOriginalContext = capturePublishedProducerContext();
      const state = await createLocalVaultAdapter(hydratedLocalProvider ?? localVault.provider).getSyncState();
      requireOriginalContext();
      return state;
    }
    return localVault.getSyncState();
  },

  async listQueuedSyncOperations(): Promise<VaultQueuedSyncOperation[]> {
    if (!inLocalMode()) return [];
    await ensureLocalData();
    return (await createLocalVaultAdapter(hydratedLocalProvider ?? localVault.provider).loadSnapshot()).sync_state?.queued_operations ?? [];
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

    const verifyCurrent = capturePublishedProducerContext();
    const capturedProvider = getSyncProvider();
    const capturedVault = localVault;
    const runOriginal = async () => {
      await guardSelectedBrowserGooglePending(capturedProvider, verifyCurrent);
      await ensureLocalData(); verifyCurrent();
      const provider = getSyncProvider();
      const adapter = createProviderAdapter(provider);
      const readiness = await this.getProviderSyncReadiness();
      if (!readiness.ready) {
        throw new Error(`Provider sync is not ready: ${readiness.blockers.join("; ")}`);
      }
      requireNoPendingGoogleReviewedMoves((await capturedVault.loadSnapshot()).sync_state?.queued_operations ?? []); verifyCurrent();
      await providerStateWithoutReviewedMove(capturedVault, "syncing", verifyCurrent);

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
                apiTransport: { fetch: async (url, options) => {
                  verifyCurrent(); const latest = await capturedVault.loadSnapshot(); verifyCurrent();
                  requireNoPendingGoogleReviewedMoves(latest.sync_state?.queued_operations ?? []);
                  const response = await createGoogleDriveApiFetchTransport().fetch(url, options); verifyCurrent(); return response;
                } },
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
          await providerStateWithoutReviewedMove(capturedVault, "error", verifyCurrent);
        } else if (reviewedResult.conflicts > 0) {
          await providerStateWithoutReviewedMove(capturedVault, "conflict", verifyCurrent);
        } else {
          await providerStateWithoutReviewedMove(capturedVault, "idle", verifyCurrent);
        }
        return reviewedResult;
      } catch (error) {
        await providerStateWithoutReviewedMove(capturedVault, "error", verifyCurrent);
        throw error;
      }
    };
    return getSyncProvider() === "google-drive" ? withGoogleReviewedMovePublicationLock(verifyCurrent, runOriginal) : runOriginal();
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

    const requireOriginalContext = capturePublishedProducerContext();
    const originalAdapter = createLocalVaultAdapter(getSyncProvider());
    await ensureLocalData();
    requireOriginalContext();
    const conflict = getConflict(conflictId);
    if (!conflict) {
      throw new Error("Conflict not found");
    }
    if (conflict.status !== "pending") {
      throw new Error("Conflict already resolved");
    }

    const doc = requireLocalDoc(conflict.space_id, conflict.document_id);
    const carriesPublishedSource = doc.published_source !== undefined;
    if (carriesPublishedSource && hydratedLocalProvider !== getSyncProvider()) throw new Error("Import the selected provider before editing its published source");
    const nextDoc = applyConflictResolutionToDocument(doc, conflict, strategy);
    const operation = originalPublishedContentOperation(doc, nextDoc, `resolve_conflict:${strategy}`);
    if (operation.server_context) await requireNativeReplayQueueCapability(buildSnapshot(), getSyncProvider(), true);
    requireOriginalContext();
    if (localDocs.get(doc.id) !== doc) throw new Error("Page changed while conflict resolution waited; retain this input for review.");
    localDocs.set(nextDoc.id, nextDoc);
    await persistLocalSnapshot(operation, requireOriginalContext, carriesPublishedSource ? originalAdapter : localVault);

    requireOriginalContext();
    const resolved = resolveConflictRecord(conflictId, strategy);
    if (!carriesPublishedSource) {
      await refreshSyncStateFromConflicts();
      requireOriginalContext();
      return resolved;
    }
    const pending = await hasPendingConflicts();
    requireOriginalContext();
    if (await originalAdapter.supportsAtomicSnapshotCommit()) {
      requireOriginalContext();
      await originalAdapter.commitSnapshotMutation((snapshot) => {
        requireOriginalContext();
        if (snapshot.sync_state && (pending || snapshot.sync_state.state === "conflict")) snapshot.sync_state.state = pending ? "conflict" : "idle";
        return { snapshot };
      });
      requireOriginalContext();
    }
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
  if (filters.maintenance_user && localDocs.get(documentId)?.maintenance_user_id !== filters.maintenance_user) return false;
  if (filters.maintenance_group && localDocs.get(documentId)?.maintenance_group_id !== filters.maintenance_group) return false;
  if (filters.identifier && humanIdentifierKey(localDocs.get(documentId)?.human_identifier ?? null) !== humanIdentifierKey(filters.identifier)) return false;
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

  if (filters.user) {
    const userFilter = filters.user.toLowerCase();
    const matchesLocalUser =
      LOCAL_SYSTEM_USER_ID.toLowerCase() === userFilter ||
      LOCAL_SYSTEM_USER.toLowerCase() === userFilter ||
      "local@starlane.notes" === userFilter;
    if (!matchesLocalUser) return false;
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
      (asset) =>
        extractDocumentIdFromAssetPath(asset, Array.from(localDocs.values())) === documentId,
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
  localReviewedMoveRecords = undefined;
  localReviewedDraftTransferRecords = undefined;
  localInitialized = false;
  hydratedLocalProvider = null;
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
  localReviewedMoveRecords = undefined;
  localReviewedDraftTransferRecords = undefined;
  localInitialized = false;
}

export async function __resetLocalDataForTests(): Promise<void> {
  await resetLocalAppData();
}
