import type { LocalReviewedMoveRecord } from "./localReviewedMove";
import type { ReviewedDraftTransferRecord } from "./reviewedDraftTransfer";
import { normalizeDocumentMaintenanceFields, normalizeDocumentMaintenanceCatalogs, type DocumentMaintenanceCatalog } from "./documentMaintenance";
import { normalizeDocumentIdentifiers } from "@/lib/humanIdentifier";
import type { SyncProviderKind } from "@/lib/storageProviders";
import {
  NATIVE_LOCAL_DATABASE_COMMANDS,
  captureReviewedMoveDestination,
  nativeJsonValueEquals,
  type NativeLocalDatabaseDescriptor,
  type NativeLocalDatabaseRecordCollection,
  type NativeLocalDatabaseCommitReceipt,
} from "@/lib/nativeLocalDatabase";

export type LocalVaultSyncState = "idle" | "syncing" | "conflict" | "error";
export const LOCAL_VAULT_SCHEMA_VERSION = 1;
const LOCAL_VAULT_DB_NAME = "starlane-local-vault";
const LOCAL_VAULT_DB_STORE = "snapshots";

export function assertLocalVaultSnapshot(value: unknown): asserts value is LocalVaultSnapshot {
  if (!value || typeof value !== "object") {
    throw new Error("Invalid vault file");
  }

  const snapshot = value as Partial<LocalVaultSnapshot>;
  assertReviewedMoveRecords(snapshot.reviewed_move_records);
  assertReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
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

export interface VaultSpace {
  id: string;
  name: string;
  slug?: string;
  description?: string;
  owner_username?: string;
  owner_display_name?: string;
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface VaultFolder {
  id: string;
  space_id: string;
  name: string;
  icon?: string;
  parent_id?: string;
}

/** Original remote published read evidence; never a post-edit/latest baseline. */
export interface VaultPublishedSource {
  schema_version: 1;
  digest: string;
  space_id: string;
  document_id: string;
  server_context: { server_base: string; account_id: string; session_scope: string };
  replay_provider: SyncProviderKind;
}

export interface VaultDocument {
  published_source?: VaultPublishedSource;
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
  created_by?: string;
  last_updated_by?: string;
  created_at?: string;
  updated_at: string;
  published_at?: string;
  archived_at?: string;
  deleted_at?: string;
}

export interface VaultDocumentDraft {
  id: string;
  document_id: string;
  space_id: string;
  owner_id: string;
  title: string;
  content: string;
  icon?: string;
  cover_image?: string;
  base_updated_at: string;
  created_at: string;
  updated_at: string;
  previous_folder_id?: string;
  previous_parent_id?: string;
}

export interface VaultAsset {
  id: string;
  name: string;
  mime_type?: string;
  size: number;
  url: string;
  vault_path?: string;
  source: "browser" | "tauri";
  /** Optional ownership added after the initial vault schema; absent on legacy assets. */
  space_id?: string;
  document_id?: string;
  /** Exact readonly server version provenance; never an authorization grant. */
  server_version_provenance?: Record<string, unknown>;
  /** Native creator history only; raw fields never grant file or archive access. */
  native_local_version_provenance?: Record<string, unknown>;
  created_at: string;
}

export interface VaultTemplate {
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

export interface VaultWorkflowTemplateStage {
  id: string;
  name: string;
  sort_order: number;
}

export interface VaultWorkflowTemplateApprover {
  id: string;
  approver_type: "user" | "role";
  approver_id: string;
  stage_id?: string;
}

export interface VaultWorkflowTemplate {
  id: string;
  space_id: string;
  name: string;
  description?: string;
  stages: VaultWorkflowTemplateStage[];
  approvers: VaultWorkflowTemplateApprover[];
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface VaultDocumentWorkflow {
  id: string;
  document_id: string;
  workflow_template_id: string;
  current_stage_id: string;
  status: "active" | "completed" | "cancelled";
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface VaultDocumentWorkflowEvent {
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

export interface VaultPropertySchema {
  id: string;
  space_id: string;
  name: string;
  property_type: string;
  options?: unknown;
  sort_order: number;
}

export interface VaultPropertyValue {
  id: string;
  document_id: string;
  property_id: string;
  value: string | null;
  created_at?: string;
  updated_at?: string;
}

export interface VaultTag {
  id: string;
  space_id: string;
  name: string;
  color: string;
  created_at?: string;
}

export interface VaultDocTag {
  document_id: string;
  tag_id: string;
}

export interface VaultSearchIndexEntry {
  document_id: string;
  space_id: string;
  title: string;
  content: string;
  search_text: string;
  updated_at: string;
}

export interface VaultQueuedSyncOperation {
  replay_provider?: SyncProviderKind;
  server_context?: { server_base: string; account_id: string; session_scope: string };
  id: string;
  entity_type: string;
  entity_id: string;
  action: string;
  queued_at: string;
  payload?: unknown;
  attempts?: number;
  last_error?: string;
}

export interface VaultQueuedSyncOperationUpdate {
  id: string;
  attempts?: number;
  last_error?: string;
}

export interface VaultSyncMetadata {
  state: LocalVaultSyncState;
  updated_at: string;
  queued_operations: VaultQueuedSyncOperation[];
}

export class ReviewedMoveEvidenceError extends Error {}
/** Validate the portable record envelope without projecting/repairing its raw evidence. */
export function assertReviewedMoveRecords(records: unknown): asserts records is LocalReviewedMoveRecord[] | undefined {
  if (records === undefined) return;
  const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
  if (!Array.isArray(records)) throw new ReviewedMoveEvidenceError("Reviewed move recovery records are malformed; retain this vault for review");
  const ids = new Set<string>();
  for (const row of records) {
    if (!object(row) || typeof row.id !== "string" || !row.id.trim() || ids.has(row.id) || !((row.schema_version === 1 && row.provider === "none") || (row.schema_version === 2 && row.provider === "google-drive" && object(row.origin) && row.origin.schema_version === 1 && row.origin.provider === "google-drive" && typeof row.origin.account_permission_id === "string" && !!row.origin.account_permission_id.trim() && typeof row.origin.root_id === "string" && !!row.origin.root_id.trim())) || typeof row.actor_id !== "string" || !row.actor_id.trim() || typeof row.moved_at !== "string" || !object(row.intent) || !object(row.decisions) || !object(row.before) || !object(row.after) || !Array.isArray(row.preserved) || !Array.isArray(row.removed)) throw new ReviewedMoveEvidenceError("Reviewed move recovery record is unavailable; retain its original bytes");
    ids.add(row.id);
    for (const inventory of [row.before, row.after]) {
      if (Object.values(inventory).some(value => !Array.isArray(value))) throw new ReviewedMoveEvidenceError("Reviewed move recovery inventory is malformed");
    }
    for (const evidence of [...row.preserved, ...row.removed]) {
      if (!object(evidence) || typeof evidence.collection !== "string" || !Array.isArray(evidence.records)) throw new ReviewedMoveEvidenceError("Reviewed move recovery metadata is malformed");
    }
  }
}

export class ReviewedDraftTransferEvidenceError extends Error {}
/** Validate portable transfer evidence without repairing or selecting a newer copy. */
export function assertReviewedDraftTransferRecords(records: unknown): asserts records is ReviewedDraftTransferRecord[] | undefined {
  if (records === undefined) return;
  const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
  if (!Array.isArray(records)) throw new ReviewedDraftTransferEvidenceError("Reviewed draft transfer records are malformed; retain this vault for review");
  const ids = new Set<string>();
  for (const row of records) {
    if (!object(row) || row.schema_version !== 1 || typeof row.operation_id !== "string" || !row.operation_id.trim() || ids.has(row.operation_id) || !["pending", "unknown", "confirmed"].includes(String(row.state)) ||
        ["server_base", "server_account_id", "server_space_id", "server_space_name", "server_document_id", "server_document_title", "prepared_at", "updated_at"].some(field => typeof row[field] !== "string" || !(row[field] as string).trim()) ||
        !object(row.request) || row.request.schema_version !== 1 || row.request.operation_id !== row.operation_id || !object(row.request.local_source) || !object(row.request.server_source) || !object(row.request.reviewed_draft) || !Array.isArray(row.request.selected_operation_fingerprints) ||
        !object(row.exact_manifest) || !Array.isArray(row.exact_manifest.operations) || row.exact_manifest.operation_count !== row.exact_manifest.operations.length || row.request.selected_operation_fingerprints.length !== row.exact_manifest.operations.length ||
        (row.state === "confirmed" && (!object(row.receipt) || !object(row.binding))))
      throw new ReviewedDraftTransferEvidenceError("Reviewed draft transfer evidence is unavailable; retain its original bytes");
    ids.add(row.operation_id);
  }
}

export interface LocalVaultSnapshot {
  reviewed_move_records?: LocalReviewedMoveRecord[];
  reviewed_draft_transfer_records?: ReviewedDraftTransferRecord[];
  maintenance_target_catalogs?: DocumentMaintenanceCatalog[];
  canonical_revision?: string | null;
  mirror_pending?: boolean;
  schema_version: number;
  spaces: VaultSpace[];
  folders: VaultFolder[];
  documents: VaultDocument[];
  drafts?: VaultDocumentDraft[];
  assets: VaultAsset[];
  templates: VaultTemplate[];
  workflow_templates?: VaultWorkflowTemplate[];
  document_workflows?: VaultDocumentWorkflow[];
  document_workflow_events?: VaultDocumentWorkflowEvent[];
  property_schemas?: VaultPropertySchema[];
  property_values?: VaultPropertyValue[];
  tags?: VaultTag[];
  doc_tags?: VaultDocTag[];
  search_index?: VaultSearchIndexEntry[];
  sync_state?: VaultSyncMetadata;
}

export type VaultQueuedOperationInput = Omit<VaultQueuedSyncOperation, "id" | "queued_at"> &
  Partial<Pick<VaultQueuedSyncOperation, "id" | "queued_at">>;

export interface VaultSnapshotMutation {
  snapshot: LocalVaultSnapshot;
  operation?: VaultQueuedOperationInput;
}

export type VaultSnapshotMutationBuilder = (committed: LocalVaultSnapshot) => VaultSnapshotMutation;

function cloneSnapshotRecords(records: LocalReviewedMoveRecord[]): LocalReviewedMoveRecord[] {
  assertReviewedMoveRecords(records);
  return JSON.parse(JSON.stringify(records)) as LocalReviewedMoveRecord[];
}

function cloneReviewedDraftTransferRecords(records: ReviewedDraftTransferRecord[]): ReviewedDraftTransferRecord[] {
  assertReviewedDraftTransferRecords(records);
  return JSON.parse(JSON.stringify(records)) as ReviewedDraftTransferRecord[];
}

function cloneSnapshot(snapshot: LocalVaultSnapshot): LocalVaultSnapshot {
  return JSON.parse(JSON.stringify(snapshot)) as LocalVaultSnapshot;
}

function snapshotCommitFingerprint(snapshot: LocalVaultSnapshot): string {
  const normalized = cloneSnapshot(normalizeSnapshot(snapshot));
  delete normalized.canonical_revision;
  delete normalized.mirror_pending;
  const reviewedRecords = normalized.reviewed_move_records;
  delete normalized.reviewed_move_records;
  const reviewedDraftTransfers = normalized.reviewed_draft_transfer_records;
  delete normalized.reviewed_draft_transfer_records;
  const orderedEvidence = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(orderedEvidence);
    if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([key, item]) => [key, orderedEvidence(item)]));
    return value;
  };
  const sortValue = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(sortValue);
    if (value && typeof value === "object") {
      const record = value as Record<string, unknown>;
      return Object.fromEntries(
        Object.keys(record)
          .sort()
          .map((key) => [key, sortValue(record[key])]),
      );
    }
    return value;
  };
  for (const [key, value] of Object.entries(normalized)) {
    if (Array.isArray(value)) {
      (normalized as unknown as Record<string, unknown>)[key] = value.sort((a, b) => {
        const left = a as { id?: string; document_id?: string; tag_id?: string };
        const right = b as { id?: string; document_id?: string; tag_id?: string };
        return (left.id ?? `${left.document_id}:${left.tag_id ?? ""}`).localeCompare(
          right.id ?? `${right.document_id}:${right.tag_id ?? ""}`,
        );
      });
    }
  }
  return reviewedRecords === undefined && reviewedDraftTransfers === undefined
    ? JSON.stringify(sortValue(normalized))
    : JSON.stringify({
        snapshot: sortValue(normalized),
        ...(reviewedRecords === undefined ? {} : { reviewed_move_records: orderedEvidence(reviewedRecords) }),
        ...(reviewedDraftTransfers === undefined ? {} : { reviewed_draft_transfer_records: orderedEvidence(reviewedDraftTransfers) }),
      });
}

/** Separate from canonical adapter locks; acquire this BEFORE persistence serialization. */
export async function withGoogleReviewedMovePublicationLock<T>(verifyCurrent: () => void, run: () => Promise<T>, requireLock = false): Promise<T> {
  verifyCurrent();
  const locks = typeof window === "undefined" ? undefined : window.navigator?.locks;
  if (!locks?.request) {
    if (requireLock) throw new Error("Reviewed Google moves require browser Web Locks; retain this review without dispatch");
    return run(); // Ordinary legacy sync remains available; it cannot create reviewed moves.
  }
  return locks.request("starlane-google-move-publication", async () => { verifyCurrent(); return run(); });
}

/** Dedicated moves are indivisible pending evidence, never ordinary provider CRUD. */
export function isPendingGoogleReviewedMove(operation: Pick<VaultQueuedSyncOperation, "entity_type" | "action" | "payload">): boolean {
  const payload = operation.payload;
  return operation.entity_type === "reviewed_move" || (!!payload && typeof payload === "object" && (payload as { kind?: unknown }).kind === "reviewed_move");
}
export function requireNoPendingGoogleReviewedMoves(operations: Pick<VaultQueuedSyncOperation, "entity_type" | "action" | "payload">[]): void {
  if (operations.some(isPendingGoogleReviewedMove)) throw new Error("Reviewed Google move is committed locally and pending provider. Remote publication/application is unavailable; retain the complete move for review.");
}
function preservePendingGoogleReviewedMoves(before: LocalVaultSnapshot, after: LocalVaultSnapshot): void {
  const pending = (before.sync_state?.queued_operations ?? []).filter(isPendingGoogleReviewedMove);
  const queue = after.sync_state?.queued_operations ?? [];
  let position = -1;
  for (const operation of pending) {
    const found = queue.findIndex((row, index) => index > position && nativeJsonValueEquals(row, operation));
    if (found < 0) throw new ReviewedMoveEvidenceError("Pending Google move cannot be cleared, replaced or imported over; retain its exact original envelope for review");
    position = found;
    const original = before.reviewed_move_records?.find(row => row.id === operation.id);
    const retained = after.reviewed_move_records?.find(row => row.id === operation.id);
    if (!original || !nativeJsonValueEquals(original, retained)) throw new ReviewedMoveEvidenceError("Pending Google move history cannot be removed or replaced");
  }
}

function prepareSnapshotMutation(
  committed: LocalVaultSnapshot,
  mutate: VaultSnapshotMutationBuilder,
): LocalVaultSnapshot {
  const { snapshot, operation } = mutate(cloneSnapshot(committed));
  const candidate = cloneSnapshot(normalizeSnapshot(snapshot));
  if (operation) {
    const queued: VaultQueuedSyncOperation = {
      ...operation,
      id: operation.id ?? makeSyncOperationId(),
      queued_at: operation.queued_at ?? new Date().toISOString(),
      attempts: operation.attempts ?? 0,
    };
    candidate.sync_state = {
      ...candidate.sync_state,
      state: candidate.sync_state?.state ?? "idle",
      updated_at: new Date().toISOString(),
      queued_operations: [...(committed.sync_state?.queued_operations ?? []), queued],
    };
  }
  preservePendingGoogleReviewedMoves(committed, candidate);
  return candidate;
}

type SnapshotRecordCollection =
  | "documents"
  | "assets"
  | "templates"
  | "workflow_templates"
  | "document_workflows";

function upsertSnapshotRecord(
  snapshot: LocalVaultSnapshot,
  collection: SnapshotRecordCollection,
  record: { id?: string; document_id?: string },
  identity: "id" | "document_id" = "id",
): void {
  const records = (snapshot[collection] ?? []) as { id?: string; document_id?: string }[];
  (snapshot as unknown as Record<string, unknown>)[collection] = [
    ...records.filter((item) => item[identity] !== record[identity]),
    record,
  ];
}

function deleteSnapshotRecord(
  snapshot: LocalVaultSnapshot,
  collection: SnapshotRecordCollection,
  id: string,
  identity: "id" | "document_id" = "id",
): void {
  const records = (snapshot[collection] ?? []) as { id?: string; document_id?: string }[];
  (snapshot as unknown as Record<string, unknown>)[collection] = records.filter(
    (item) => item[identity] !== id,
  );
}

function snapshotSyncState(snapshot: LocalVaultSnapshot): VaultSyncMetadata {
  snapshot.sync_state ??= {
    state: "idle",
    updated_at: new Date().toISOString(),
    queued_operations: [],
  };
  return snapshot.sync_state;
}

/** Captured reviewed writer/reader. Never refresh its original destination on retry. */
export interface LocalVaultReviewedMoveAccess {
  loadSnapshot(): Promise<LocalVaultSnapshot>;
  commitSnapshotMutation(mutate: VaultSnapshotMutationBuilder): Promise<LocalVaultSnapshot>;
}

export interface LocalVaultAdapter {
  provider: SyncProviderKind;
  init(): Promise<void>;
  loadSnapshot(): Promise<LocalVaultSnapshot>;
  saveSnapshot(snapshot: LocalVaultSnapshot): Promise<void>;
  commitSnapshotMutation(mutate: VaultSnapshotMutationBuilder): Promise<LocalVaultSnapshot>;
  supportsAtomicSnapshotCommit(): Promise<boolean>;
  supportsReviewedMoveCommit?(): Promise<boolean>;
  openReviewedMoveSession?(verifyCurrent: () => void): Promise<LocalVaultReviewedMoveAccess>;
  admitArchiveCanonicalSnapshot?(snapshot: LocalVaultSnapshot, verifyCurrent: () => void): Promise<void>;
  reset(): Promise<void>;
  setSyncState(state: LocalVaultSyncState): Promise<void>;
  enqueueSyncOperation(
    operation: Omit<VaultQueuedSyncOperation, "id" | "queued_at"> &
      Partial<Pick<VaultQueuedSyncOperation, "id" | "queued_at">>,
  ): Promise<VaultQueuedSyncOperation>;
  listQueuedSyncOperations(): Promise<VaultQueuedSyncOperation[]>;
  updateQueuedSyncOperations(updates: VaultQueuedSyncOperationUpdate[]): Promise<void>;
  clearQueuedSyncOperations(operationIds?: string[]): Promise<void>;
  listSpaces(): Promise<VaultSpace[]>;
  getDocument(documentId: string): Promise<VaultDocument | null>;
  saveDocument(document: VaultDocument): Promise<void>;
  deleteDocument(documentId: string): Promise<void>;
  saveAsset(asset: VaultAsset): Promise<void>;
  deleteAsset(assetId: string): Promise<void>;
  listAssets(): Promise<VaultAsset[]>;
  saveTemplate(template: VaultTemplate): Promise<void>;
  deleteTemplate(templateId: string): Promise<void>;
  listTemplates(): Promise<VaultTemplate[]>;
  saveWorkflowTemplate(template: VaultWorkflowTemplate): Promise<void>;
  deleteWorkflowTemplate(templateId: string): Promise<void>;
  listWorkflowTemplates(): Promise<VaultWorkflowTemplate[]>;
  saveDocumentWorkflow(workflow: VaultDocumentWorkflow): Promise<void>;
  deleteDocumentWorkflow(documentId: string): Promise<void>;
  getDocumentWorkflow(documentId: string): Promise<VaultDocumentWorkflow | null>;
  getSyncState(): Promise<LocalVaultSyncState>;
}

function getStorageKey(provider: SyncProviderKind): string {
  return `starlane_local_vault_v${LOCAL_VAULT_SCHEMA_VERSION}_${provider}`;
}

function canUseLocalStorage(): boolean {
  return typeof window !== "undefined" && !!window.localStorage;
}

function createEmptySnapshot(): LocalVaultSnapshot {
  return {
    schema_version: LOCAL_VAULT_SCHEMA_VERSION,
    spaces: [],
    folders: [],
    documents: [],
    drafts: [],
    assets: [],
    templates: [],
    workflow_templates: [],
    document_workflows: [],
    document_workflow_events: [],
    property_schemas: [],
    property_values: [],
    tags: [],
    doc_tags: [],
    search_index: [],
    sync_state: {
      state: "idle",
      updated_at: new Date().toISOString(),
      queued_operations: [],
    },
  };
}

// Archive admission never repairs or invents durable replay identity. Ordinary
// local builders keep their legacy defaulting in normalizeQueuedSyncOperations.
export function assertImportedQueuedSyncOperations(snapshot: { sync_state?: unknown }): void {
  if (snapshot.sync_state === undefined) return;
  const sync = snapshot.sync_state;
  if (!sync || typeof sync !== "object" || Array.isArray(sync))
    throw new Error("Imported sync state is malformed");
  const state = sync as Record<string, unknown>;
  if (!isLocalVaultSyncState(state.state) ||
      (state.updated_at !== undefined && (typeof state.updated_at !== "string" || !state.updated_at.trim())))
    throw new Error("Imported sync state is malformed");
  if (state.queued_operations === undefined) return;
  if (!Array.isArray(state.queued_operations)) throw new Error("Imported replay queue is malformed");
  const ids = new Set<string>();
  for (const raw of state.queued_operations) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw new Error("Imported replay operation is malformed");
    const operation = raw as Record<string, unknown>;
    for (const field of ["id", "entity_type", "entity_id", "action", "queued_at"])
      if (typeof operation[field] !== "string" || !(operation[field] as string).trim())
        throw new Error("Imported replay operation identity is missing or malformed");
    const id = operation.id as string;
    if (ids.has(id)) throw new Error("Imported replay operation identity is duplicated");
    ids.add(id);
    if (operation.attempts !== undefined && (!Number.isSafeInteger(operation.attempts) || (operation.attempts as number) < 0))
      throw new Error("Imported replay attempts are malformed");
    if (operation.last_error !== undefined && typeof operation.last_error !== "string")
      throw new Error("Imported replay error is malformed");
    if (operation.replay_provider !== undefined && !["none", "icloud-drive", "google-drive"].includes(operation.replay_provider as string))
      throw new Error("Imported replay provider binding is malformed");
    if (operation.server_context !== undefined) {
      const context = operation.server_context;
      if (!context || typeof context !== "object" || Array.isArray(context) ||
          ["server_base", "account_id", "session_scope"].some((field) =>
            typeof (context as Record<string, unknown>)[field] !== "string" ||
            !((context as Record<string, unknown>)[field] as string).trim()))
        throw new Error("Imported replay account binding is malformed");
    }
    if (["update_identifier", "update_maintenance"].includes(operation.action as string) && operation.replay_provider === undefined)
      throw new Error("Imported replay provider binding is missing");
  }
}

function syncMetadataExtras(sync: LocalVaultSnapshot["sync_state"]): Record<string, unknown> {
  const extras = { ...sync } as Record<string, unknown>;
  delete extras.state;
  delete extras.updated_at;
  delete extras.queued_operations;
  return extras;
}

function normalizeSnapshot(
  snapshot: Partial<LocalVaultSnapshot> | null | undefined,
): LocalVaultSnapshot {
  assertReviewedMoveRecords(snapshot?.reviewed_move_records);
  assertReviewedDraftTransferRecords(snapshot?.reviewed_draft_transfer_records);
  if (!snapshot || snapshot.schema_version !== LOCAL_VAULT_SCHEMA_VERSION) {
    return createEmptySnapshot();
  }

  return {
    ...(snapshot.canonical_revision !== undefined
      ? { canonical_revision: snapshot.canonical_revision }
      : {}),
    ...(snapshot.mirror_pending !== undefined ? { mirror_pending: snapshot.mirror_pending } : {}),
    ...(snapshot.reviewed_move_records !== undefined ? { reviewed_move_records: cloneSnapshotRecords(snapshot.reviewed_move_records) } : {}),
    ...(snapshot.reviewed_draft_transfer_records !== undefined ? { reviewed_draft_transfer_records: cloneReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records) } : {}),
    schema_version: LOCAL_VAULT_SCHEMA_VERSION,
    spaces: Array.isArray(snapshot.spaces) ? snapshot.spaces : [],
    folders: Array.isArray(snapshot.folders) ? snapshot.folders : [],
    documents: normalizeDocumentIdentifiers(Array.isArray(snapshot.documents) ? snapshot.documents : []).map(normalizeDocumentMaintenanceFields),
    ...(snapshot.maintenance_target_catalogs !== undefined ? { maintenance_target_catalogs: normalizeDocumentMaintenanceCatalogs(snapshot.maintenance_target_catalogs) } : {}),
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
    sync_state:
      snapshot.sync_state && isLocalVaultSyncState(snapshot.sync_state.state)
        ? {
            ...snapshot.sync_state,
            state: snapshot.sync_state.state,
            updated_at: snapshot.sync_state.updated_at ?? new Date().toISOString(),
            queued_operations: normalizeQueuedSyncOperations(snapshot.sync_state.queued_operations),
          }
        : {
            state: "idle",
            updated_at: new Date().toISOString(),
            queued_operations: [],
          },
  };
}

function isLocalVaultSyncState(value: unknown): value is LocalVaultSyncState {
  return value === "idle" || value === "syncing" || value === "conflict" || value === "error";
}

function normalizeQueuedSyncOperations(value: unknown): VaultQueuedSyncOperation[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((operation): VaultQueuedSyncOperation | null => {
      if (!operation || typeof operation !== "object") return null;
      const candidate = operation as Partial<VaultQueuedSyncOperation>;
      if (
        typeof candidate.entity_type !== "string" ||
        typeof candidate.entity_id !== "string" ||
        typeof candidate.action !== "string"
      ) {
        return null;
      }
      if (candidate.replay_provider !== undefined && !["none", "icloud-drive", "google-drive"].includes(candidate.replay_provider)) throw new Error("Queued identifier provider binding is invalid");
      if (candidate.server_context !== undefined && (!candidate.server_context || typeof candidate.server_context !== "object" || typeof candidate.server_context.server_base !== "string" || !candidate.server_context.server_base || typeof candidate.server_context.account_id !== "string" || !candidate.server_context.account_id || typeof candidate.server_context.session_scope !== "string" || !candidate.server_context.session_scope)) throw new Error("Queued identifier account binding is invalid");
      return {
        ...candidate,
        id: typeof candidate.id === "string" ? candidate.id : makeSyncOperationId(),
        entity_type: candidate.entity_type,
        entity_id: candidate.entity_id,
        action: candidate.action,
        queued_at:
          typeof candidate.queued_at === "string" ? candidate.queued_at : new Date().toISOString(),
        ...("payload" in candidate ? { payload: candidate.payload } : {}),
        ...(candidate.attempts !== undefined ? { attempts: candidate.attempts } : {}),
        ...(candidate.last_error !== undefined ? { last_error: candidate.last_error } : {}),
      };
    })
    .filter((operation): operation is VaultQueuedSyncOperation => operation !== null);
}

function makeSyncOperationId(): string {
  return `syncop-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

function hasSnapshotContent(snapshot: LocalVaultSnapshot): boolean {
  return (
    snapshot.spaces.length > 0 ||
    snapshot.folders.length > 0 ||
    snapshot.documents.length > 0 ||
    snapshot.assets.length > 0 ||
    snapshot.templates.length > 0 ||
    (snapshot.workflow_templates?.length ?? 0) > 0 ||
    (snapshot.document_workflows?.length ?? 0) > 0 ||
    (snapshot.document_workflow_events?.length ?? 0) > 0 ||
    (snapshot.property_schemas?.length ?? 0) > 0 ||
    (snapshot.property_values?.length ?? 0) > 0 ||
    (snapshot.tags?.length ?? 0) > 0 ||
    (snapshot.doc_tags?.length ?? 0) > 0 ||
    (snapshot.reviewed_move_records?.length ?? 0) > 0
  );
}

export interface LocalVaultStructureCheckResult {
  ok: boolean;
  repairs: string[];
  blockers: string[];
  snapshot: LocalVaultSnapshot;
}

export interface LocalVaultStartupCheckResult {
  provider: SyncProviderKind;
  status: "ok" | "repaired";
  repairs: string[];
  counts: {
    spaces: number;
    folders: number;
    documents: number;
    templates: number;
  };
}

export function repairLocalVaultSnapshotStructure(
  snapshot: LocalVaultSnapshot,
): LocalVaultStructureCheckResult {
  const repairs: string[] = [];
  const blockers: string[] = [];
  const spaceIds = new Set(snapshot.spaces.map((space) => space.id).filter(Boolean));

  const folders = snapshot.folders.map((folder) => ({ ...folder }));
  const folderById = new Map(folders.map((folder) => [folder.id, folder]));

  for (const folder of folders) {
    if (!spaceIds.has(folder.space_id)) {
      blockers.push(`Folder ${folder.id} references missing space ${folder.space_id}`);
      continue;
    }
    if (!folder.parent_id) continue;
    const parent = folderById.get(folder.parent_id);
    if (!parent || parent.space_id !== folder.space_id || folder.parent_id === folder.id) {
      repairs.push(`Detached folder ${folder.id} from invalid parent`);
      folder.parent_id = undefined;
      continue;
    }
    const seen = new Set<string>([folder.id]);
    let ancestorId = parent.parent_id;
    while (ancestorId) {
      if (seen.has(ancestorId)) {
        repairs.push(`Detached folder ${folder.id} from cyclic parent chain`);
        folder.parent_id = undefined;
        break;
      }
      seen.add(ancestorId);
      ancestorId = folderById.get(ancestorId)?.parent_id;
    }
  }

  const normalizedFolderById = new Map(folders.map((folder) => [folder.id, folder]));
  const documents = snapshot.documents.map((document) => ({ ...document }));
  const documentById = new Map(documents.map((document) => [document.id, document]));

  for (const document of documents) {
    if (!spaceIds.has(document.space_id)) {
      blockers.push(`Document ${document.id} references missing space ${document.space_id}`);
      continue;
    }
    if (document.folder_id) {
      const folder = normalizedFolderById.get(document.folder_id);
      if (!folder || folder.space_id !== document.space_id) {
        repairs.push(`Detached document ${document.id} from invalid folder`);
        document.folder_id = undefined;
      }
    }
    if (!document.parent_id) continue;
    const parent = documentById.get(document.parent_id);
    if (
      !parent ||
      parent.space_id !== document.space_id ||
      parent.folder_id !== document.folder_id ||
      document.parent_id === document.id
    ) {
      repairs.push(`Detached document ${document.id} from invalid parent page`);
      document.parent_id = undefined;
      continue;
    }
  }
  // Resolve each edge once. Repair malformed cycles, never valid depth.
  const completed = new Set<string>();
  for (const document of documents) {
    if (completed.has(document.id)) continue;
    const path: string[] = [], seen = new Set<string>();
    let id: string | undefined = document.id;
    while (id && !completed.has(id)) {
      if (seen.has(id)) {
        const cyclic = documentById.get(id)!;
        repairs.push(`Detached document ${cyclic.id} from cyclic parent chain`);
        cyclic.parent_id = undefined;
        break;
      }
      seen.add(id); path.push(id);
      id = documentById.get(id)?.parent_id;
    }
    for (const visited of path) completed.add(visited);
  }

  return {
    ok: blockers.length === 0 && repairs.length === 0,
    repairs,
    blockers,
    snapshot: {
      ...snapshot,
      folders,
      documents,
    },
  };
}

export async function runLocalVaultStartupCheck(
  provider: SyncProviderKind,
): Promise<LocalVaultStartupCheckResult> {
  const vault = createLocalVaultAdapter(provider);
  await vault.init();
  const snapshot = await vault.loadSnapshot();
  const checked = repairLocalVaultSnapshotStructure(snapshot);

  if (checked.blockers.length > 0) {
    throw new Error(checked.blockers[0]);
  }

  if (checked.repairs.length > 0) {
    await vault.saveSnapshot(checked.snapshot);
  }

  return {
    provider,
    status: checked.repairs.length > 0 ? "repaired" : "ok",
    repairs: checked.repairs,
    counts: {
      spaces: checked.snapshot.spaces.length,
      folders: checked.snapshot.folders.length,
      documents: checked.snapshot.documents.length,
      templates: checked.snapshot.templates.length,
    },
  };
}

function canUseTauriNativeVault(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

interface NativeRecordMutation {
  type: "upsert" | "delete";
  collection: NativeLocalDatabaseRecordCollection;
  id: string;
  payload?: unknown;
}

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriCoreModule })
    .__TAURI_INTERNALS__;
  if (internals?.invoke) {
    return internals.invoke<T>(cmd, args);
  }

  const coreModulePath = "@tauri-apps/api/core";
  const coreMod = (await import(/* @vite-ignore */ coreModulePath)) as unknown as TauriCoreModule;
  return coreMod.invoke<T>(cmd, args);
}

/**
 * Import downloaded bytes into the native vault so attachments have a real
 * file on disk instead of only a data URL in the snapshot.
 */
export async function importNativeVaultAssetFromDataUrl(input: {
  name: string;
  mime_type?: string;
  data_url: string;
  space_id?: string;
  document_id?: string;
}): Promise<VaultAsset | null> {
  if (!canUseTauriNativeVault()) return null;

  const asset = await invokeTauri<VaultAsset>("import_vault_asset_from_data_url", {
    provider: "none",
    name: input.name,
    mimeType: input.mime_type,
    dataUrl: input.data_url,
    spaceId: input.space_id,
    documentId: input.document_id,
  });
  const adapter = createLocalVaultAdapter("none");
  await adapter.saveAsset(asset);
  return asset;
}

class LocalStorageVaultAdapter implements LocalVaultAdapter {
  provider: SyncProviderKind;
  private storageKey: string;
  private spaces = new Map<string, VaultSpace>();
  private folders = new Map<string, VaultFolder>();
  private documents = new Map<string, VaultDocument>();
  private drafts = new Map<string, VaultDocumentDraft>();
  private assets = new Map<string, VaultAsset>();
  private templates = new Map<string, VaultTemplate>();
  private workflowTemplates = new Map<string, VaultWorkflowTemplate>();
  private documentWorkflows = new Map<string, VaultDocumentWorkflow>();
  private documentWorkflowEvents = new Map<string, VaultDocumentWorkflowEvent>();
  private propertySchemas = new Map<string, VaultPropertySchema>();
  private propertyValues = new Map<string, VaultPropertyValue>();
  private tags = new Map<string, VaultTag>();
  private docTags = new Map<string, VaultDocTag>();
  private searchIndex = new Map<string, VaultSearchIndexEntry>();
  private reviewedMoveRecords: LocalReviewedMoveRecord[] | undefined;
  private reviewedDraftTransferRecords: ReviewedDraftTransferRecord[] | undefined;
  private maintenanceCatalogs: DocumentMaintenanceCatalog[] | undefined;
  private syncMetadata: Record<string, unknown> = {};
  private syncState: LocalVaultSyncState = "idle";
  private queuedOperations: VaultQueuedSyncOperation[] = [];
  private initialized = false;

  constructor(provider: SyncProviderKind) {
    this.provider = provider;
    this.storageKey = getStorageKey(provider);
  }

  private readSnapshotFromStorage(): LocalVaultSnapshot {
    if (!canUseLocalStorage()) return createEmptySnapshot();
    const raw = window.localStorage.getItem(this.storageKey);
    if (!raw) return createEmptySnapshot();
    try {
      const parsed = JSON.parse(raw) as Partial<LocalVaultSnapshot>;
      return normalizeSnapshot(parsed);
    } catch (cause) {
      if (cause instanceof ReviewedMoveEvidenceError || cause instanceof ReviewedDraftTransferEvidenceError) throw cause;
      return createEmptySnapshot();
    }
  }

  private canUseIndexedDb(): boolean {
    return typeof window !== "undefined" && "indexedDB" in window;
  }

  private openIndexedDb(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = window.indexedDB.open(LOCAL_VAULT_DB_NAME, 1);
      request.onupgradeneeded = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains(LOCAL_VAULT_DB_STORE)) {
          db.createObjectStore(LOCAL_VAULT_DB_STORE);
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error ?? new Error("IndexedDB open failed"));
    });
  }

  private async readSnapshotFromIndexedDb(strict = false): Promise<LocalVaultSnapshot | null> {
    if (!this.canUseIndexedDb()) return null;
    try {
      const db = await this.openIndexedDb();
      return await new Promise((resolve, reject) => {
        const tx = db.transaction(LOCAL_VAULT_DB_STORE, "readonly");
        const store = tx.objectStore(LOCAL_VAULT_DB_STORE);
        const req = store.get(this.storageKey);
        req.onsuccess = () => {
          const value = req.result as Partial<LocalVaultSnapshot> | undefined;
          try { resolve(value ? normalizeSnapshot(value) : null); } catch (cause) { reject(cause); }
        };
        req.onerror = () => reject(req.error ?? new Error("IndexedDB read failed"));
      });
    } catch (cause) {
      if (strict || cause instanceof ReviewedMoveEvidenceError || cause instanceof ReviewedDraftTransferEvidenceError) throw cause;
      return null;
    }
  }

  private async writeSnapshotToIndexedDb(snapshot: LocalVaultSnapshot): Promise<void> {
    if (!this.canUseIndexedDb()) return;
    const db = await this.openIndexedDb();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(LOCAL_VAULT_DB_STORE, "readwrite");
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error ?? new Error("IndexedDB write failed"));
      tx.onabort = () => reject(tx.error ?? new Error("IndexedDB transaction aborted"));
      tx.objectStore(LOCAL_VAULT_DB_STORE).put(snapshot, this.storageKey);
    });
  }

  private async deleteSnapshotFromIndexedDb(): Promise<void> {
    if (!this.canUseIndexedDb()) return;
    try {
      const db = await this.openIndexedDb();
      await new Promise<void>((resolve, reject) => {
        const tx = db.transaction(LOCAL_VAULT_DB_STORE, "readwrite");
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error ?? new Error("IndexedDB delete failed"));
        tx.objectStore(LOCAL_VAULT_DB_STORE).delete(this.storageKey);
      });
    } catch {
      // best effort
    }
  }

  private writeSnapshotToStorage(snapshot: LocalVaultSnapshot): void {
    if (!canUseLocalStorage()) return;
    window.localStorage.setItem(this.storageKey, JSON.stringify(snapshot));
  }

  private applySnapshot(snapshot: LocalVaultSnapshot): void {
    assertReviewedMoveRecords(snapshot.reviewed_move_records);
    assertReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
    this.reviewedMoveRecords = snapshot.reviewed_move_records === undefined ? undefined : cloneSnapshotRecords(snapshot.reviewed_move_records);
    this.reviewedDraftTransferRecords = snapshot.reviewed_draft_transfer_records === undefined ? undefined : cloneReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
    this.maintenanceCatalogs = snapshot.maintenance_target_catalogs === undefined ? undefined : normalizeDocumentMaintenanceCatalogs(snapshot.maintenance_target_catalogs);
    this.spaces = new Map(snapshot.spaces.map((space) => [space.id, space]));
    this.folders = new Map(snapshot.folders.map((folder) => [folder.id, folder]));
    this.documents = new Map(snapshot.documents.map((doc) => [doc.id, doc]));
    this.drafts = new Map((snapshot.drafts ?? []).map((draft) => [draft.id, draft]));
    this.assets = new Map(snapshot.assets.map((asset) => [asset.id, asset]));
    this.templates = new Map(snapshot.templates.map((template) => [template.id, template]));
    this.workflowTemplates = new Map(
      (snapshot.workflow_templates ?? []).map((template) => [template.id, template]),
    );
    this.documentWorkflows = new Map(
      (snapshot.document_workflows ?? []).map((workflow) => [workflow.document_id, workflow]),
    );
    this.documentWorkflowEvents = new Map(
      (snapshot.document_workflow_events ?? []).map((event) => [event.id, event]),
    );
    this.propertySchemas = new Map(
      (snapshot.property_schemas ?? []).map((schema) => [schema.id, schema]),
    );
    this.propertyValues = new Map(
      (snapshot.property_values ?? []).map((value) => [value.id, value]),
    );
    this.tags = new Map((snapshot.tags ?? []).map((tag) => [tag.id, tag]));
    this.docTags = new Map(
      (snapshot.doc_tags ?? []).map((docTag) => [`${docTag.document_id}:${docTag.tag_id}`, docTag]),
    );
    this.searchIndex = new Map(
      (snapshot.search_index ?? []).map((entry) => [entry.document_id, entry]),
    );
    this.syncMetadata = syncMetadataExtras(snapshot.sync_state);
    this.syncState = snapshot.sync_state?.state ?? "idle";
    this.queuedOperations = snapshot.sync_state?.queued_operations ?? [];
  }

  async init(): Promise<void> {
    if (this.initialized) return;
    const indexed = await this.readSnapshotFromIndexedDb();
    const snapshot = indexed ?? this.readSnapshotFromStorage();
    this.applySnapshot(snapshot);
    this.initialized = true;
  }

  async loadSnapshot(): Promise<LocalVaultSnapshot> {
    await this.init();
    return {
      schema_version: LOCAL_VAULT_SCHEMA_VERSION,
      spaces: Array.from(this.spaces.values()),
      folders: Array.from(this.folders.values()),
      documents: Array.from(this.documents.values()),
      maintenance_target_catalogs: this.maintenanceCatalogs,
      ...(this.reviewedMoveRecords !== undefined ? { reviewed_move_records: cloneSnapshotRecords(this.reviewedMoveRecords) } : {}),
      ...(this.reviewedDraftTransferRecords !== undefined ? { reviewed_draft_transfer_records: cloneReviewedDraftTransferRecords(this.reviewedDraftTransferRecords) } : {}),
      drafts: Array.from(this.drafts.values()),
      assets: Array.from(this.assets.values()),
      templates: Array.from(this.templates.values()),
      workflow_templates: Array.from(this.workflowTemplates.values()),
      document_workflows: Array.from(this.documentWorkflows.values()),
      document_workflow_events: Array.from(this.documentWorkflowEvents.values()),
      property_schemas: Array.from(this.propertySchemas.values()),
      property_values: Array.from(this.propertyValues.values()),
      tags: Array.from(this.tags.values()),
      doc_tags: Array.from(this.docTags.values()),
      search_index: Array.from(this.searchIndex.values()),
      sync_state: {
        ...this.syncMetadata,
        state: this.syncState,
        updated_at: new Date().toISOString(),
        queued_operations: this.queuedOperations,
      },
    };
  }

  async saveSnapshot(snapshot: LocalVaultSnapshot): Promise<void> {
    assertReviewedMoveRecords(snapshot.reviewed_move_records);
    assertReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
    if (this.provider === "google-drive") {
      await this.commitSnapshotMutation(() => ({ snapshot }));
      return;
    }
    await this.init();
    this.syncState = "syncing";
    try {
      const normalized: LocalVaultSnapshot = {
        schema_version: LOCAL_VAULT_SCHEMA_VERSION,
        ...(snapshot.reviewed_move_records !== undefined ? { reviewed_move_records: cloneSnapshotRecords(snapshot.reviewed_move_records) } : {}),
        ...(snapshot.reviewed_draft_transfer_records !== undefined ? { reviewed_draft_transfer_records: cloneReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records) } : {}),
        spaces: snapshot.spaces,
        folders: snapshot.folders,
        documents: snapshot.documents,
        ...(snapshot.maintenance_target_catalogs !== undefined ? { maintenance_target_catalogs: normalizeDocumentMaintenanceCatalogs(snapshot.maintenance_target_catalogs) } : {}),
        drafts: snapshot.drafts ?? [],
        assets: snapshot.assets,
        templates: snapshot.templates,
        workflow_templates: snapshot.workflow_templates ?? [],
        document_workflows: snapshot.document_workflows ?? [],
        document_workflow_events: snapshot.document_workflow_events ?? [],
        property_schemas: snapshot.property_schemas ?? [],
        property_values: snapshot.property_values ?? [],
        tags: snapshot.tags ?? [],
        doc_tags: snapshot.doc_tags ?? [],
        search_index: snapshot.search_index ?? [],
        sync_state: snapshot.sync_state ?? {
          state: "idle",
          updated_at: new Date().toISOString(),
          queued_operations: [],
        },
      };
      if (this.canUseIndexedDb()) {
        await this.writeSnapshotToIndexedDb(normalized);
        try {
          this.writeSnapshotToStorage(normalized);
        } catch {
          /* Cache is repairable. */
        }
      } else {
        this.writeSnapshotToStorage(normalized);
      }
      this.applySnapshot(normalized);
      this.syncState = normalized.sync_state?.state ?? "idle";
    } catch {
      this.syncState = "error";
      throw new Error("Failed to persist local vault snapshot");
    }
  }

  private async commitMutationToIndexedDb(
    mutate: VaultSnapshotMutationBuilder,
    initial: LocalVaultSnapshot,
  ): Promise<LocalVaultSnapshot> {
    const db = await this.openIndexedDb();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(LOCAL_VAULT_DB_STORE, "readwrite");
      let candidate: LocalVaultSnapshot | undefined;
      let mutationError: unknown;
      tx.oncomplete = () =>
        candidate
          ? resolve(candidate)
          : reject(new Error("IndexedDB commit did not produce a snapshot"));
      tx.onerror = () => reject(mutationError ?? tx.error ?? new Error("IndexedDB write failed"));
      tx.onabort = () =>
        reject(mutationError ?? tx.error ?? new Error("IndexedDB transaction aborted"));
      const store = tx.objectStore(LOCAL_VAULT_DB_STORE);
      const request = store.get(this.storageKey);
      request.onerror = () => reject(request.error ?? new Error("IndexedDB read failed"));
      request.onsuccess = () => {
        try {
          candidate = prepareSnapshotMutation(
            request.result ? normalizeSnapshot(request.result as LocalVaultSnapshot) : initial,
            mutate,
          );
          store.put(candidate, this.storageKey);
        } catch (cause) {
          mutationError = cause;
          tx.abort();
        }
      };
    });
  }

  async supportsAtomicSnapshotCommit(): Promise<boolean> {
    return true;
  }

  async supportsReviewedMoveCommit(): Promise<boolean> {
    return typeof window.indexedDB?.open === "function" || typeof window.navigator?.locks?.request === "function";
  }

  async openReviewedMoveSession(verifyCurrent: () => void): Promise<LocalVaultReviewedMoveAccess> {
    verifyCurrent();
    if (canUseTauriNativeVault()) throw new Error("Original adapter changed; reload the workspace before native review");
    if (!await this.supportsReviewedMoveCommit()) throw new Error("Reviewed moves need transactional local storage");
    verifyCurrent();
    return {
      loadSnapshot: async () => { verifyCurrent(); const saved = await new LocalStorageVaultAdapter(this.provider).loadSnapshot(); verifyCurrent(); return saved; },
      commitSnapshotMutation: async mutate => { verifyCurrent(); const saved = await this.commitSnapshotMutation(committed => { verifyCurrent(); return mutate(committed); }); verifyCurrent(); return saved; },
    };
  }

  async commitSnapshotMutation(mutate: VaultSnapshotMutationBuilder): Promise<LocalVaultSnapshot> {
    await this.init();
    let candidate: LocalVaultSnapshot;
    if (this.canUseIndexedDb()) {
      // Read, validate/build, and write in one transaction, including other tabs.
      candidate = await this.commitMutationToIndexedDb(mutate, await this.loadSnapshot());
      try {
        this.writeSnapshotToStorage(candidate);
      } catch {
        /* Cache is repairable. */
      }
    } else {
      if (!canUseLocalStorage()) throw new Error("Durable local storage is unavailable");
      const commit = () => {
        const next = prepareSnapshotMutation(this.readSnapshotFromStorage(), mutate);
        this.writeSnapshotToStorage(next);
        return next;
      };
      candidate = window.navigator?.locks
        ? await window.navigator.locks.request(`starlane-vault-${this.provider}`, async () =>
            commit(),
          )
        : commit();
    }
    this.applySnapshot(candidate);
    return cloneSnapshot(candidate);
  }

  async reset(): Promise<void> {
    if (this.provider === "google-drive") {
      await this.commitSnapshotMutation(committed => {
        requireNoPendingGoogleReviewedMoves(committed.sync_state?.queued_operations ?? []);
        return { snapshot: createEmptySnapshot() };
      });
      return;
    }
    this.reviewedMoveRecords = undefined;
    this.reviewedDraftTransferRecords = undefined;
    this.spaces.clear();
    this.folders.clear();
    this.documents.clear();
    this.assets.clear();
    this.templates.clear();
    this.workflowTemplates.clear();
    this.documentWorkflows.clear();
    this.documentWorkflowEvents.clear();
    this.propertySchemas.clear();
    this.propertyValues.clear();
    this.tags.clear();
    this.docTags.clear();
    this.searchIndex.clear();
    this.syncMetadata = {};
    this.syncState = "idle";
    this.queuedOperations = [];
    this.initialized = false;
    await this.deleteSnapshotFromIndexedDb();
    if (canUseLocalStorage()) {
      window.localStorage.removeItem(this.storageKey);
    }
  }

  async listSpaces(): Promise<VaultSpace[]> {
    await this.init();
    return Array.from(this.spaces.values());
  }

  async getDocument(documentId: string): Promise<VaultDocument | null> {
    await this.init();
    return this.documents.get(documentId) ?? null;
  }

  async saveDocument(document: VaultDocument): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      upsertSnapshotRecord(snapshot, "documents", document, "id");
      return { snapshot };
    });
  }

  async deleteDocument(documentId: string): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      deleteSnapshotRecord(snapshot, "documents", documentId, "id");
      return { snapshot };
    });
  }

  async saveAsset(asset: VaultAsset): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      upsertSnapshotRecord(snapshot, "assets", asset, "id");
      return { snapshot };
    });
  }

  async deleteAsset(assetId: string): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      deleteSnapshotRecord(snapshot, "assets", assetId, "id");
      return { snapshot };
    });
  }

  async listAssets(): Promise<VaultAsset[]> {
    await this.init();
    return Array.from(this.assets.values());
  }

  async saveTemplate(template: VaultTemplate): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      upsertSnapshotRecord(snapshot, "templates", template, "id");
      return { snapshot };
    });
  }

  async deleteTemplate(templateId: string): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      deleteSnapshotRecord(snapshot, "templates", templateId, "id");
      return { snapshot };
    });
  }

  async listTemplates(): Promise<VaultTemplate[]> {
    await this.init();
    return Array.from(this.templates.values());
  }

  async saveWorkflowTemplate(template: VaultWorkflowTemplate): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      upsertSnapshotRecord(snapshot, "workflow_templates", template, "id");
      return { snapshot };
    });
  }

  async deleteWorkflowTemplate(templateId: string): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      deleteSnapshotRecord(snapshot, "workflow_templates", templateId, "id");
      return { snapshot };
    });
  }

  async listWorkflowTemplates(): Promise<VaultWorkflowTemplate[]> {
    await this.init();
    return Array.from(this.workflowTemplates.values());
  }

  async saveDocumentWorkflow(workflow: VaultDocumentWorkflow): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      upsertSnapshotRecord(snapshot, "document_workflows", workflow, "document_id");
      return { snapshot };
    });
  }

  async deleteDocumentWorkflow(documentId: string): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      deleteSnapshotRecord(snapshot, "document_workflows", documentId, "document_id");
      return { snapshot };
    });
  }

  async getDocumentWorkflow(documentId: string): Promise<VaultDocumentWorkflow | null> {
    await this.init();
    return this.documentWorkflows.get(documentId) ?? null;
  }

  async getSyncState(): Promise<LocalVaultSyncState> {
    await this.init();
    return this.syncState;
  }

  async setSyncState(state: LocalVaultSyncState): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      const sync = snapshotSyncState(snapshot);
      sync.state = state;
      sync.updated_at = new Date().toISOString();
      return { snapshot };
    });
  }

  async enqueueSyncOperation(
    operation: Omit<VaultQueuedSyncOperation, "id" | "queued_at"> &
      Partial<Pick<VaultQueuedSyncOperation, "id" | "queued_at">>,
  ): Promise<VaultQueuedSyncOperation> {
    const id = operation.id ?? makeSyncOperationId();
    const saved = await this.commitSnapshotMutation((snapshot) => ({
      snapshot,
      operation: { ...operation, id },
    }));
    return saved.sync_state!.queued_operations.find((queued) => queued.id === id)!;
  }

  async listQueuedSyncOperations(): Promise<VaultQueuedSyncOperation[]> {
    await this.init();
    return [...this.queuedOperations];
  }

  async updateQueuedSyncOperations(updates: VaultQueuedSyncOperationUpdate[]): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      const updatesById = new Map(updates.map((update) => [update.id, update]));
      const sync = snapshotSyncState(snapshot);
      sync.queued_operations = sync.queued_operations.map((operation) => {
        const update = updatesById.get(operation.id);
        return update
          ? {
              ...operation,
              attempts: update.attempts ?? operation.attempts,
              last_error: update.last_error,
            }
          : operation;
      });
      return { snapshot };
    });
  }

  async clearQueuedSyncOperations(operationIds?: string[]): Promise<void> {
    await this.commitSnapshotMutation((snapshot) => {
      const sync = snapshotSyncState(snapshot);
      const ids = new Set(operationIds);
      sync.queued_operations = operationIds?.length
        ? sync.queued_operations.filter((operation) => !ids.has(operation.id))
        : [];
      return { snapshot };
    });
  }
}

function recordMutationId(collection: NativeLocalDatabaseRecordCollection, value: unknown): string {
  const record = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  if (collection === "doc_tags") {
    return `${String(record.document_id ?? "document")}:${String(record.tag_id ?? "tag")}`;
  }
  if (collection === "search_index") {
    return String(record.document_id ?? "");
  }
  if (collection === "document_workflows") {
    return String(record.id ?? record.document_id ?? "");
  }
  if (collection === "property_values") {
    return String(
      record.id ??
        `${String(record.document_id ?? "document")}:${String(record.property_id ?? "property")}`,
    );
  }
  if (collection === "reviewed_draft_transfer_records") {
    return String(record.operation_id ?? "");
  }
  return String(record.id ?? "");
}

function snapshotMutationEntries(snapshot: LocalVaultSnapshot): Array<{
  collection: NativeLocalDatabaseRecordCollection;
  id: string;
  payload: unknown;
}> {
  const entries: Array<{
    collection: NativeLocalDatabaseRecordCollection;
    id: string;
    payload: unknown;
  }> = [];
  const append = <T>(collection: NativeLocalDatabaseRecordCollection, values: T[] | undefined) => {
    for (const value of values ?? []) {
      const id = recordMutationId(collection, value);
      if (id) entries.push({ collection, id, payload: value });
    }
  };

  append("spaces", snapshot.spaces);
  append("folders", snapshot.folders);
  append("documents", snapshot.documents);
  append("drafts", snapshot.drafts);
  append("reviewed_move_records", snapshot.reviewed_move_records);
  append("reviewed_draft_transfer_records", snapshot.reviewed_draft_transfer_records);
  append("assets", snapshot.assets);
  append("templates", snapshot.templates);
  append("workflow_templates", snapshot.workflow_templates);
  append("document_workflows", snapshot.document_workflows);
  append("document_workflow_events", snapshot.document_workflow_events);
  append("property_schemas", snapshot.property_schemas);
  append("property_values", snapshot.property_values);
  append("tags", snapshot.tags);
  append("doc_tags", snapshot.doc_tags);
  append("search_index", snapshot.search_index);
  if (snapshot.sync_state) {
    entries.push({ collection: "sync_state", id: "sync_state", payload: snapshot.sync_state });
    append("queued_operations", snapshot.sync_state.queued_operations);
  }

  return entries;
}

function buildSnapshotRecordMutations(
  previous: LocalVaultSnapshot,
  next: LocalVaultSnapshot,
): NativeRecordMutation[] {
  const previousEntries = snapshotMutationEntries(previous);
  const nextEntries = snapshotMutationEntries(next);
  const previousByKey = new Map(
    previousEntries.map((entry) => [`${entry.collection}:${entry.id}`, entry]),
  );
  const nextByKey = new Map(nextEntries.map((entry) => [`${entry.collection}:${entry.id}`, entry]));
  const mutations: NativeRecordMutation[] = [];

  for (const [key, entry] of nextByKey) {
    const previousEntry = previousByKey.get(key);
    if (JSON.stringify(previousEntry?.payload) !== JSON.stringify(entry.payload)) {
      mutations.push({
        type: "upsert",
        collection: entry.collection,
        id: entry.id,
        payload: entry.payload,
      });
    }
  }

  for (const [key, entry] of previousByKey) {
    if (!nextByKey.has(key)) {
      mutations.push({ type: "delete", collection: entry.collection, id: entry.id });
    }
  }

  return mutations;
}

class TauriVaultAdapter implements LocalVaultAdapter {
  private canonicalRevision: string | null = null;
  private mirrorPending = false;
  private knownModernDescriptor: NativeLocalDatabaseDescriptor | null = null;
  provider: SyncProviderKind;
  private fallback: LocalStorageVaultAdapter;
  private spaces = new Map<string, VaultSpace>();
  private folders = new Map<string, VaultFolder>();
  private documents = new Map<string, VaultDocument>();
  private drafts = new Map<string, VaultDocumentDraft>();
  private assets = new Map<string, VaultAsset>();
  private templates = new Map<string, VaultTemplate>();
  private workflowTemplates = new Map<string, VaultWorkflowTemplate>();
  private documentWorkflows = new Map<string, VaultDocumentWorkflow>();
  private documentWorkflowEvents = new Map<string, VaultDocumentWorkflowEvent>();
  private propertySchemas = new Map<string, VaultPropertySchema>();
  private propertyValues = new Map<string, VaultPropertyValue>();
  private tags = new Map<string, VaultTag>();
  private docTags = new Map<string, VaultDocTag>();
  private searchIndex = new Map<string, VaultSearchIndexEntry>();
  private reviewedMoveRecords: LocalReviewedMoveRecord[] | undefined;
  private reviewedDraftTransferRecords: ReviewedDraftTransferRecord[] | undefined;
  private maintenanceCatalogs: DocumentMaintenanceCatalog[] | undefined;
  private syncMetadata: Record<string, unknown> = {};
  private syncState: LocalVaultSyncState = "idle";
  private queuedOperations: VaultQueuedSyncOperation[] = [];
  private initialized = false;

  constructor(provider: SyncProviderKind) {
    this.provider = provider;
    this.fallback = new LocalStorageVaultAdapter(provider);
  }

  private applySnapshot(snapshot: LocalVaultSnapshot): void {
    assertReviewedMoveRecords(snapshot.reviewed_move_records);
    assertReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
    this.reviewedMoveRecords = snapshot.reviewed_move_records === undefined ? undefined : cloneSnapshotRecords(snapshot.reviewed_move_records);
    this.reviewedDraftTransferRecords = snapshot.reviewed_draft_transfer_records === undefined ? undefined : cloneReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
    this.maintenanceCatalogs = snapshot.maintenance_target_catalogs === undefined ? undefined : normalizeDocumentMaintenanceCatalogs(snapshot.maintenance_target_catalogs);
    this.spaces = new Map(snapshot.spaces.map((space) => [space.id, space]));
    this.folders = new Map(snapshot.folders.map((folder) => [folder.id, folder]));
    this.documents = new Map(snapshot.documents.map((doc) => [doc.id, doc]));
    this.drafts = new Map((snapshot.drafts ?? []).map((draft) => [draft.id, draft]));
    this.assets = new Map(snapshot.assets.map((asset) => [asset.id, asset]));
    this.templates = new Map(snapshot.templates.map((template) => [template.id, template]));
    this.workflowTemplates = new Map(
      (snapshot.workflow_templates ?? []).map((template) => [template.id, template]),
    );
    this.documentWorkflows = new Map(
      (snapshot.document_workflows ?? []).map((workflow) => [workflow.document_id, workflow]),
    );
    this.documentWorkflowEvents = new Map(
      (snapshot.document_workflow_events ?? []).map((event) => [event.id, event]),
    );
    this.propertySchemas = new Map(
      (snapshot.property_schemas ?? []).map((schema) => [schema.id, schema]),
    );
    this.propertyValues = new Map(
      (snapshot.property_values ?? []).map((value) => [value.id, value]),
    );
    this.tags = new Map((snapshot.tags ?? []).map((tag) => [tag.id, tag]));
    this.docTags = new Map(
      (snapshot.doc_tags ?? []).map((docTag) => [`${docTag.document_id}:${docTag.tag_id}`, docTag]),
    );
    this.searchIndex = new Map(
      (snapshot.search_index ?? []).map((entry) => [entry.document_id, entry]),
    );
    this.syncMetadata = syncMetadataExtras(snapshot.sync_state);
    this.syncState = snapshot.sync_state?.state ?? "idle";
    this.queuedOperations = snapshot.sync_state?.queued_operations ?? [];
  }

  private buildSnapshot(): LocalVaultSnapshot {
    return {
      canonical_revision: this.canonicalRevision,
      mirror_pending: this.mirrorPending,
      schema_version: LOCAL_VAULT_SCHEMA_VERSION,
      spaces: Array.from(this.spaces.values()),
      folders: Array.from(this.folders.values()),
      documents: Array.from(this.documents.values()),
      maintenance_target_catalogs: this.maintenanceCatalogs,
      ...(this.reviewedMoveRecords !== undefined ? { reviewed_move_records: cloneSnapshotRecords(this.reviewedMoveRecords) } : {}),
      ...(this.reviewedDraftTransferRecords !== undefined ? { reviewed_draft_transfer_records: cloneReviewedDraftTransferRecords(this.reviewedDraftTransferRecords) } : {}),
      drafts: Array.from(this.drafts.values()),
      assets: Array.from(this.assets.values()),
      templates: Array.from(this.templates.values()),
      workflow_templates: Array.from(this.workflowTemplates.values()),
      document_workflows: Array.from(this.documentWorkflows.values()),
      document_workflow_events: Array.from(this.documentWorkflowEvents.values()),
      property_schemas: Array.from(this.propertySchemas.values()),
      property_values: Array.from(this.propertyValues.values()),
      tags: Array.from(this.tags.values()),
      doc_tags: Array.from(this.docTags.values()),
      search_index: Array.from(this.searchIndex.values()),
      sync_state: {
        ...this.syncMetadata,
        state: this.syncState,
        updated_at: new Date().toISOString(),
        queued_operations: this.queuedOperations,
      },
    };
  }

  private async readNativeSnapshot(): Promise<LocalVaultSnapshot | null> {
    const descriptor = await this.describeNativeLocalDatabase();
    if (descriptor?.supports_snapshot_bridge) {
      const snapshot = await invokeTauri<Partial<LocalVaultSnapshot> | null>(
        NATIVE_LOCAL_DATABASE_COMMANDS.readSnapshot,
        { provider: this.provider },
      );
      this.canonicalRevision = snapshot?.canonical_revision ?? null;
      this.mirrorPending = snapshot?.mirror_pending === true;
      return snapshot ? normalizeSnapshot(snapshot) : null;
    }

    const snapshot = await invokeTauri<Partial<LocalVaultSnapshot> | null>("read_vault_snapshot", {
      provider: this.provider,
    });
    return snapshot ? normalizeSnapshot(snapshot) : null;
  }

  private async writeNativeSnapshot(snapshot: LocalVaultSnapshot): Promise<void> {
    const descriptor = await this.describeNativeLocalDatabase();
    if (snapshot.reviewed_move_records !== undefined && (!descriptor?.supports_atomic_snapshot_commit || descriptor.supports_reviewed_move_evidence !== true || descriptor.supports_reviewed_move_destination_binding !== true)) throw new ReviewedMoveEvidenceError("Update the desktop shell before retaining reviewed move recovery records");
    if (snapshot.reviewed_draft_transfer_records !== undefined && (!descriptor?.supports_atomic_snapshot_commit || descriptor.supports_reviewed_draft_transfer_evidence !== true)) throw new ReviewedDraftTransferEvidenceError("Update the desktop shell before retaining reviewed draft transfer evidence");
    if (descriptor?.supports_atomic_snapshot_commit) {
      const receipt = await invokeTauri<NativeLocalDatabaseCommitReceipt>(
        NATIVE_LOCAL_DATABASE_COMMANDS.commitSnapshot,
        { provider: this.provider, snapshot, expectedRevision: this.canonicalRevision },
      );
      if (!receipt.canonical_committed) throw new Error("Native commit was not acknowledged");
      this.canonicalRevision = receipt.canonical_revision;
      this.mirrorPending = receipt.mirror_pending;
      return;
    }
    if (descriptor?.supports_snapshot_bridge) {
      await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot, {
        provider: this.provider,
        snapshot,
      });
      return;
    }

    await invokeTauri<void>("write_vault_snapshot", {
      provider: this.provider,
      snapshot,
    });
  }

  private async requireReviewedEvidencePersistence(snapshot: LocalVaultSnapshot): Promise<void> {
    if (snapshot.reviewed_move_records === undefined && snapshot.reviewed_draft_transfer_records === undefined) return;
    assertReviewedMoveRecords(snapshot.reviewed_move_records);
    assertReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
    const descriptor = await this.describeNativeLocalDatabase();
    if (snapshot.reviewed_move_records !== undefined && (!descriptor?.supports_atomic_snapshot_commit || descriptor.supports_reviewed_move_evidence !== true || descriptor.supports_reviewed_move_destination_binding !== true)) throw new ReviewedMoveEvidenceError("Update the desktop shell before retaining reviewed move recovery records");
    if (snapshot.reviewed_draft_transfer_records !== undefined && (!descriptor?.supports_atomic_snapshot_commit || descriptor.supports_reviewed_draft_transfer_evidence !== true)) throw new ReviewedDraftTransferEvidenceError("Update the desktop shell before retaining reviewed draft transfer evidence");
  }

  private async upsertNativeRecord(
    collection: NativeLocalDatabaseRecordCollection,
    id: string,
    payload: unknown,
    rowOrder?: number,
  ): Promise<void> {
    await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord, {
      provider: this.provider,
      collection,
      id,
      payload,
      rowOrder,
    });
  }

  private async deleteNativeRecord(
    collection: NativeLocalDatabaseRecordCollection,
    id: string,
  ): Promise<void> {
    await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord, {
      provider: this.provider,
      collection,
      id,
    });
  }

  private async syncNativeMirror(snapshot: LocalVaultSnapshot): Promise<void> {
    await this.requireReviewedEvidencePersistence(snapshot);
    try {
      await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.syncMirror, {
        provider: this.provider,
      });
    } catch {
      // Older native shells do not have the batched mirror command. A full
      // snapshot write keeps their portable JSON and Markdown mirror current.
      await this.writeNativeSnapshot(snapshot);
    }
  }

  private async saveNativeRecordWithSnapshotMirror(
    collection: NativeLocalDatabaseRecordCollection,
    id: string,
    payload: unknown,
    snapshot: LocalVaultSnapshot,
    rowOrder?: number,
  ): Promise<void> {
    await this.requireReviewedEvidencePersistence(snapshot);
    try {
      await this.upsertNativeRecord(collection, id, payload, rowOrder);
      await this.syncNativeMirror(snapshot);
    } catch {
      await this.writeNativeSnapshot(snapshot);
      await this.fallback.saveSnapshot(snapshot);
      return;
    }
    await this.fallback.saveSnapshot(snapshot);
  }

  private async deleteNativeRecordWithSnapshotMirror(
    collection: NativeLocalDatabaseRecordCollection,
    id: string,
    snapshot: LocalVaultSnapshot,
  ): Promise<void> {
    await this.requireReviewedEvidencePersistence(snapshot);
    try {
      await this.deleteNativeRecord(collection, id);
      await this.syncNativeMirror(snapshot);
    } catch {
      await this.writeNativeSnapshot(snapshot);
      await this.fallback.saveSnapshot(snapshot);
      return;
    }
    await this.fallback.saveSnapshot(snapshot);
  }

  private async resetNativeStore(): Promise<void> {
    const descriptor = await this.describeNativeLocalDatabase();
    if (descriptor?.supports_snapshot_bridge) {
      await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.reset, { provider: this.provider });
      return;
    }

    await invokeTauri<void>("reset_vault_snapshot", { provider: this.provider });
  }

  private async describeNativeLocalDatabase(): Promise<NativeLocalDatabaseDescriptor | null> {
    if (this.knownModernDescriptor) return this.knownModernDescriptor;
    try {
      const descriptor = await invokeTauri<NativeLocalDatabaseDescriptor>(
        NATIVE_LOCAL_DATABASE_COMMANDS.describe,
        { provider: this.provider },
      );
      if (descriptor.supports_atomic_snapshot_commit) this.knownModernDescriptor = descriptor;
      return descriptor;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      if (/unknown command|command .*not found|not found.*command/i.test(message)) return null;
      // A transport failure cannot downgrade a modern shell to blind legacy writes.
      throw cause;
    }
  }

  async init(): Promise<void> {
    if (this.initialized) return;

    try {
      const nativeSnapshot = await this.readNativeSnapshot();
      if (nativeSnapshot) {
        await this.requireReviewedEvidencePersistence(nativeSnapshot);
        this.applySnapshot(nativeSnapshot);
        if (this.mirrorPending) {
          try {
            await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.syncMirror, {
              provider: this.provider,
            });
            this.mirrorPending = false;
          } catch {
            /* Keep canonical data and retry the mirror later. */
          }
        }
      } else {
        await this.fallback.init();
        const fallbackSnapshot = await this.fallback.loadSnapshot();
        this.applySnapshot(fallbackSnapshot);
        if (hasSnapshotContent(fallbackSnapshot)) {
          await this.writeNativeSnapshot(fallbackSnapshot);
        }
      }
      this.initialized = true;
      return;
    } catch (cause) {
      if (cause instanceof ReviewedMoveEvidenceError || cause instanceof ReviewedDraftTransferEvidenceError) throw cause;
      if (this.knownModernDescriptor) {
        // A concurrent bootstrap may win its CAS. Re-read canonical SQLite rather
        // than publishing or rewriting a stale browser fallback snapshot.
        const canonical = await this.readNativeSnapshot();
        if (!canonical) throw cause;
        this.applySnapshot(canonical);
        this.initialized = true;
        return;
      }
      await this.fallback.init();
      this.applySnapshot(await this.fallback.loadSnapshot());
      this.syncState = "error";
      this.initialized = true;
    }
  }

  async loadSnapshot(): Promise<LocalVaultSnapshot> {
    await this.init();
    return this.buildSnapshot();
  }

  async saveSnapshot(snapshot: LocalVaultSnapshot): Promise<void> {
    if (snapshot.reviewed_move_records?.some(row => row.schema_version === 2)) throw new ReviewedMoveEvidenceError("Native Google reviewed move history is unavailable; retain it in its browser workspace");
    assertReviewedMoveRecords(snapshot.reviewed_move_records);
    assertReviewedDraftTransferRecords(snapshot.reviewed_draft_transfer_records);
    if (snapshot.reviewed_move_records !== undefined) {
      const evidenceDescriptor = await this.describeNativeLocalDatabase();
      if (!evidenceDescriptor?.supports_atomic_snapshot_commit || evidenceDescriptor.supports_reviewed_move_evidence !== true || evidenceDescriptor.supports_reviewed_move_destination_binding !== true) throw new ReviewedMoveEvidenceError("Update the desktop shell before retaining reviewed move recovery records");
    }
    if (snapshot.reviewed_draft_transfer_records !== undefined) {
      const evidenceDescriptor = await this.describeNativeLocalDatabase();
      if (!evidenceDescriptor?.supports_atomic_snapshot_commit || evidenceDescriptor.supports_reviewed_draft_transfer_evidence !== true) throw new ReviewedDraftTransferEvidenceError("Update the desktop shell before retaining reviewed draft transfer evidence");
    }
    await this.init();
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation(() => ({ snapshot }));
      return;
    }
    this.syncState = "syncing";
    const previous = this.buildSnapshot();
    const normalized = normalizeSnapshot(snapshot);
    try {
      this.applySnapshot(normalized);
      const mutations = buildSnapshotRecordMutations(previous, normalized);
      let needsNativeSnapshotBridge = true;
      try {
        for (const mutation of mutations) {
          if (mutation.type === "upsert") {
            await this.upsertNativeRecord(mutation.collection, mutation.id, mutation.payload);
          } else {
            await this.deleteNativeRecord(mutation.collection, mutation.id);
          }
        }
        needsNativeSnapshotBridge = false;
      } catch {
        // Older native shells still persist through the full snapshot bridge below.
      }
      if (needsNativeSnapshotBridge) {
        await this.writeNativeSnapshot(normalized);
      } else {
        await this.syncNativeMirror(normalized);
      }
      await this.fallback.saveSnapshot(normalized);
      this.syncState = normalized.sync_state?.state ?? "idle";
    } catch {
      this.syncState = "error";
      throw new Error("Failed to persist native vault snapshot");
    }
  }

  async supportsAtomicSnapshotCommit(): Promise<boolean> {
    return (await this.describeNativeLocalDatabase())?.supports_atomic_snapshot_commit === true;
  }

  async supportsReviewedMoveCommit(): Promise<boolean> {
    // Evidence bridge alone cannot bind the accepted preview root through IPC.
    return false;
  }

  async admitArchiveCanonicalSnapshot(snapshot: LocalVaultSnapshot, verifyCurrent: () => void): Promise<void> {
    verifyCurrent();
    assertLocalVaultSnapshot(snapshot);
    assertImportedQueuedSyncOperations(snapshot);
    if (typeof snapshot.canonical_revision !== "string" || !snapshot.canonical_revision)
      throw new Error("Original archive database current revision is unavailable");
    const current = normalizeSnapshot(snapshot);
    verifyCurrent();
    this.canonicalRevision = current.canonical_revision ?? null;
    this.mirrorPending = current.mirror_pending === true;
    this.applySnapshot(current);
    this.initialized = true;
  }

  async openReviewedMoveSession(verifyCurrent: () => void): Promise<LocalVaultReviewedMoveAccess> {
    verifyCurrent();
    if (this.provider !== "none") throw new Error("Reviewed native moves require the standalone provider");
    // No init: init may read/repair/mirror the current selection, rather than this proof.
    const descriptor = await invokeTauri<NativeLocalDatabaseDescriptor | null>(NATIVE_LOCAL_DATABASE_COMMANDS.describe, { provider: "none" }); verifyCurrent();
    const expectedDestination = captureReviewedMoveDestination(descriptor);
    const read = async (): Promise<LocalVaultSnapshot> => {
      verifyCurrent();
      const raw = await invokeTauri<Partial<LocalVaultSnapshot> | null>(NATIVE_LOCAL_DATABASE_COMMANDS.readSnapshot, { provider: "none", expectedDestination });
      verifyCurrent();
      if (!raw || raw.schema_version !== LOCAL_VAULT_SCHEMA_VERSION || (raw.canonical_revision !== undefined && raw.canonical_revision !== null && typeof raw.canonical_revision !== "string")) throw new Error("Original native canonical snapshot is unavailable; retained browser or portable copies cannot acknowledge this move");
      return normalizeSnapshot(raw);
    };
    return {
      loadSnapshot: read,
      commitSnapshotMutation: async mutate => {
        const committed = await read(); verifyCurrent();
        const candidate = prepareSnapshotMutation(committed, snapshot => { verifyCurrent(); return mutate(snapshot); });
        verifyCurrent();
        const previousIds = new Set((committed.reviewed_move_records ?? []).map(row => row.id));
        const created = (candidate.reviewed_move_records ?? []).filter(row => !previousIds.has(row.id));
        if (created.length !== 1) throw new Error("A reviewed native move must retain exactly one new operation record");
        const matchesOperation = (saved: LocalVaultSnapshot) => (saved.reviewed_move_records ?? []).some(row => row.id === created[0].id && nativeJsonValueEquals(row, created[0]));
        let saved: LocalVaultSnapshot;
        try {
          const receipt = await invokeTauri<NativeLocalDatabaseCommitReceipt>(NATIVE_LOCAL_DATABASE_COMMANDS.commitSnapshot, { provider: "none", snapshot: candidate, expectedRevision: committed.canonical_revision ?? null, expectedDestination });
          verifyCurrent();
          if (!receipt || receipt.canonical_committed !== true || typeof receipt.canonical_revision !== "string" || !receipt.canonical_revision || typeof receipt.mirror_pending !== "boolean" || (receipt.mirror_error !== null && typeof receipt.mirror_error !== "string")) throw new Error("Native move acknowledgment is malformed");
          saved = await read(); verifyCurrent();
          if (!matchesOperation(saved)) throw new Error("Native move operation evidence is unavailable");
        } catch {
          // Recovery uses the SAME original bound canonical reader, never current-root
          // descriptor refresh, RAM, portable mirrors or browser fallback.
          verifyCurrent();
          const recovered = await read(); verifyCurrent();
          if (!matchesOperation(recovered)) throw new Error("Native move outcome is unknown; retain the original request for exact evidence inspection");
          saved = recovered;
        }
        verifyCurrent();
        this.canonicalRevision = saved.canonical_revision ?? null;
        this.mirrorPending = saved.mirror_pending === true;
        this.applySnapshot(saved); this.initialized = true;
        // No fallback.saveSnapshot or mirror command: those do not carry this proof.
        return cloneSnapshot(saved);
      },
    };
  }

  async commitSnapshotMutation(mutate: VaultSnapshotMutationBuilder): Promise<LocalVaultSnapshot> {
    await this.init();
    const descriptor = await this.describeNativeLocalDatabase();
    if (!descriptor?.supports_atomic_snapshot_commit) {
      throw new Error(
        "This native shell needs an update before atomic property writes are available",
      );
    }
    const committed = (await this.readNativeSnapshot()) ?? this.buildSnapshot();
    const candidate = prepareSnapshotMutation(committed, mutate);
    if (candidate.reviewed_move_records?.some(row => row.schema_version === 2)) throw new ReviewedMoveEvidenceError("Native Google reviewed move history is unavailable; retain it in its browser workspace");
    if (candidate.reviewed_move_records !== undefined && (descriptor.supports_reviewed_move_evidence !== true || descriptor.supports_reviewed_move_destination_binding !== true)) throw new ReviewedMoveEvidenceError("Update the desktop shell before retaining reviewed move recovery records");
    if (candidate.reviewed_draft_transfer_records !== undefined && descriptor.supports_reviewed_draft_transfer_evidence !== true) throw new ReviewedDraftTransferEvidenceError("Update the desktop shell before retaining reviewed draft transfer evidence");
    try {
      const receipt = await invokeTauri<NativeLocalDatabaseCommitReceipt>(
        NATIVE_LOCAL_DATABASE_COMMANDS.commitSnapshot,
        { provider: this.provider, snapshot: candidate, expectedRevision: this.canonicalRevision },
      );
      if (!receipt.canonical_committed) throw new Error("Native commit was not acknowledged");
      this.canonicalRevision = receipt.canonical_revision;
      this.mirrorPending = receipt.mirror_pending;
    } catch {
      // An IPC acknowledgement can be lost after commit. Reconcile the complete
      // candidate before reporting failure; never redirect writes to browser storage.
      const recovered = await this.readNativeSnapshot().catch(() => null);
      if (
        !recovered ||
        snapshotCommitFingerprint(recovered) !== snapshotCommitFingerprint(candidate)
      ) {
        throw new Error("Native commit failed or its outcome is unknown; reload before retrying");
      }
    }
    candidate.canonical_revision = this.canonicalRevision;
    candidate.mirror_pending = this.mirrorPending;
    this.applySnapshot(candidate);
    try {
      await this.fallback.saveSnapshot(candidate);
    } catch {
      /* Native SQLite is canonical. */
    }
    return cloneSnapshot(candidate);
  }

  async reset(): Promise<void> {
    // A retained archive footprint may refuse reset; keep RAM/work until ACK.
    await this.resetNativeStore();
    this.reviewedMoveRecords = undefined;
    this.reviewedDraftTransferRecords = undefined;
    this.spaces.clear();
    this.folders.clear();
    this.documents.clear();
    this.assets.clear();
    this.templates.clear();
    this.workflowTemplates.clear();
    this.documentWorkflows.clear();
    this.documentWorkflowEvents.clear();
    this.propertySchemas.clear();
    this.propertyValues.clear();
    this.tags.clear();
    this.docTags.clear();
    this.searchIndex.clear();
    this.syncMetadata = {};
    this.syncState = "idle";
    this.queuedOperations = [];
    this.initialized = false;
    await this.fallback.reset();
  }

  async listSpaces(): Promise<VaultSpace[]> {
    await this.init();
    return Array.from(this.spaces.values());
  }

  async getDocument(documentId: string): Promise<VaultDocument | null> {
    await this.init();
    return this.documents.get(documentId) ?? null;
  }

  async saveDocument(document: VaultDocument): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        upsertSnapshotRecord(snapshot, "documents", document, "id");
        return { snapshot };
      });
      return;
    }

    await this.init();
    this.documents.set(document.id, document);
    await this.saveNativeRecordWithSnapshotMirror(
      "documents",
      document.id,
      document,
      this.buildSnapshot(),
    );
  }

  async deleteDocument(documentId: string): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        deleteSnapshotRecord(snapshot, "documents", documentId, "id");
        return { snapshot };
      });
      return;
    }

    await this.init();
    this.documents.delete(documentId);
    await this.deleteNativeRecordWithSnapshotMirror("documents", documentId, this.buildSnapshot());
  }

  async saveAsset(asset: VaultAsset): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        upsertSnapshotRecord(snapshot, "assets", asset, "id");
        return { snapshot };
      });
      return;
    }

    await this.init();
    this.assets.set(asset.id, asset);
    await this.saveNativeRecordWithSnapshotMirror("assets", asset.id, asset, this.buildSnapshot());
  }

  async deleteAsset(assetId: string): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      let asset: VaultAsset | undefined;
      await this.commitSnapshotMutation((snapshot) => {
        asset = snapshot.assets.find((item) => item.id === assetId);
        deleteSnapshotRecord(snapshot, "assets", assetId, "id");
        return { snapshot };
      });
      if (asset?.source === "tauri" && asset.vault_path) {
        try {
          await invokeTauri<void>("delete_vault_asset", { vaultPath: asset.vault_path });
        } catch {
          console.warn("Asset metadata was removed; native file cleanup needs a retry.");
        }
      }
      return;
    }

    await this.init();
    const asset = this.assets.get(assetId);
    if (asset?.source === "tauri" && asset.vault_path) {
      await invokeTauri<void>("delete_vault_asset", { vaultPath: asset.vault_path });
    }
    this.assets.delete(assetId);
    await this.deleteNativeRecordWithSnapshotMirror("assets", assetId, this.buildSnapshot());
  }

  async listAssets(): Promise<VaultAsset[]> {
    await this.init();
    return Array.from(this.assets.values());
  }

  async saveTemplate(template: VaultTemplate): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        upsertSnapshotRecord(snapshot, "templates", template, "id");
        return { snapshot };
      });
      return;
    }

    await this.init();
    this.templates.set(template.id, template);
    await this.saveNativeRecordWithSnapshotMirror(
      "templates",
      template.id,
      template,
      this.buildSnapshot(),
    );
  }

  async deleteTemplate(templateId: string): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        deleteSnapshotRecord(snapshot, "templates", templateId, "id");
        return { snapshot };
      });
      return;
    }

    await this.init();
    this.templates.delete(templateId);
    await this.deleteNativeRecordWithSnapshotMirror("templates", templateId, this.buildSnapshot());
  }

  async listTemplates(): Promise<VaultTemplate[]> {
    await this.init();
    return Array.from(this.templates.values());
  }

  async saveWorkflowTemplate(template: VaultWorkflowTemplate): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        upsertSnapshotRecord(snapshot, "workflow_templates", template, "id");
        return { snapshot };
      });
      return;
    }

    await this.init();
    this.workflowTemplates.set(template.id, template);
    await this.saveNativeRecordWithSnapshotMirror(
      "workflow_templates",
      template.id,
      template,
      this.buildSnapshot(),
    );
  }

  async deleteWorkflowTemplate(templateId: string): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        deleteSnapshotRecord(snapshot, "workflow_templates", templateId, "id");
        return { snapshot };
      });
      return;
    }

    await this.init();
    this.workflowTemplates.delete(templateId);
    await this.deleteNativeRecordWithSnapshotMirror(
      "workflow_templates",
      templateId,
      this.buildSnapshot(),
    );
  }

  async listWorkflowTemplates(): Promise<VaultWorkflowTemplate[]> {
    await this.init();
    return Array.from(this.workflowTemplates.values());
  }

  async saveDocumentWorkflow(workflow: VaultDocumentWorkflow): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        upsertSnapshotRecord(snapshot, "document_workflows", workflow, "document_id");
        return { snapshot };
      });
      return;
    }

    await this.init();
    this.documentWorkflows.set(workflow.document_id, workflow);
    await this.saveNativeRecordWithSnapshotMirror(
      "document_workflows",
      workflow.id ?? workflow.document_id,
      workflow,
      this.buildSnapshot(),
    );
  }

  async deleteDocumentWorkflow(documentId: string): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        deleteSnapshotRecord(snapshot, "document_workflows", documentId, "document_id");
        return { snapshot };
      });
      return;
    }

    await this.init();
    const workflowId = this.documentWorkflows.get(documentId)?.id ?? documentId;
    this.documentWorkflows.delete(documentId);
    await this.deleteNativeRecordWithSnapshotMirror(
      "document_workflows",
      workflowId,
      this.buildSnapshot(),
    );
  }

  async getDocumentWorkflow(documentId: string): Promise<VaultDocumentWorkflow | null> {
    await this.init();
    return this.documentWorkflows.get(documentId) ?? null;
  }

  async getSyncState(): Promise<LocalVaultSyncState> {
    await this.init();
    return this.syncState;
  }

  async setSyncState(state: LocalVaultSyncState): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        const sync = snapshotSyncState(snapshot);
        sync.state = state;
        sync.updated_at = new Date().toISOString();
        return { snapshot };
      });
      return;
    }

    await this.init();
    this.syncState = state;
    const snapshot = this.buildSnapshot();
    await this.saveNativeRecordWithSnapshotMirror(
      "sync_state",
      this.provider,
      snapshot.sync_state,
      snapshot,
    );
  }

  async enqueueSyncOperation(
    operation: Omit<VaultQueuedSyncOperation, "id" | "queued_at"> &
      Partial<Pick<VaultQueuedSyncOperation, "id" | "queued_at">>,
  ): Promise<VaultQueuedSyncOperation> {
    if (await this.supportsAtomicSnapshotCommit()) {
      const id = operation.id ?? makeSyncOperationId();
      const saved = await this.commitSnapshotMutation((snapshot) => ({
        snapshot,
        operation: { ...operation, id },
      }));
      return saved.sync_state!.queued_operations.find((queued) => queued.id === id)!;
    }

    await this.init();
    const queued: VaultQueuedSyncOperation = {
      id: operation.id ?? makeSyncOperationId(),
      entity_type: operation.entity_type,
      entity_id: operation.entity_id,
      action: operation.action,
      queued_at: operation.queued_at ?? new Date().toISOString(),
      payload: operation.payload,
      attempts: operation.attempts ?? 0,
      last_error: operation.last_error,
    };
    this.queuedOperations = [...this.queuedOperations, queued];
    const snapshot = this.buildSnapshot();
    await this.saveNativeRecordWithSnapshotMirror("queued_operations", queued.id, queued, snapshot);
    return queued;
  }

  async listQueuedSyncOperations(): Promise<VaultQueuedSyncOperation[]> {
    await this.init();
    return [...this.queuedOperations];
  }

  async updateQueuedSyncOperations(updates: VaultQueuedSyncOperationUpdate[]): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        const updatesById = new Map(updates.map((update) => [update.id, update]));
        const sync = snapshotSyncState(snapshot);
        sync.queued_operations = sync.queued_operations.map((operation) => {
          const update = updatesById.get(operation.id);
          return update
            ? {
                ...operation,
                attempts: update.attempts ?? operation.attempts,
                last_error: update.last_error,
              }
            : operation;
        });
        return { snapshot };
      });
      return;
    }

    await this.init();
    const updateById = new Map(updates.map((update) => [update.id, update]));
    this.queuedOperations = this.queuedOperations.map((operation) => {
      const update = updateById.get(operation.id);
      if (!update) return operation;
      return {
        ...operation,
        attempts: update.attempts ?? operation.attempts,
        last_error: update.last_error,
      };
    });
    const snapshot = this.buildSnapshot();
    let needsNativeSnapshotBridge = true;
    try {
      await Promise.all(
        this.queuedOperations
          .filter((operation) => updateById.has(operation.id))
          .map((operation) =>
            this.upsertNativeRecord("queued_operations", operation.id, operation),
          ),
      );
      needsNativeSnapshotBridge = false;
    } catch {
      // Fall back to the full snapshot write below.
    }
    if (needsNativeSnapshotBridge) {
      await this.writeNativeSnapshot(snapshot);
    }
    await this.fallback.saveSnapshot(snapshot);
  }

  async clearQueuedSyncOperations(operationIds?: string[]): Promise<void> {
    if (await this.supportsAtomicSnapshotCommit()) {
      await this.commitSnapshotMutation((snapshot) => {
        const sync = snapshotSyncState(snapshot);
        const ids = new Set(operationIds);
        sync.queued_operations = operationIds?.length
          ? sync.queued_operations.filter((operation) => !ids.has(operation.id))
          : [];
        return { snapshot };
      });
      return;
    }

    await this.init();
    const operationIdsToDelete =
      operationIds && operationIds.length > 0
        ? operationIds
        : this.queuedOperations.map((operation) => operation.id);
    if (operationIds && operationIds.length > 0) {
      const ids = new Set(operationIds);
      this.queuedOperations = this.queuedOperations.filter((operation) => !ids.has(operation.id));
    } else {
      this.queuedOperations = [];
    }
    const snapshot = this.buildSnapshot();
    let needsNativeSnapshotBridge = true;
    try {
      await Promise.all([
        ...operationIdsToDelete.map((operationId) =>
          this.deleteNativeRecord("queued_operations", operationId),
        ),
        this.upsertNativeRecord("sync_state", this.provider, snapshot.sync_state),
      ]);
      needsNativeSnapshotBridge = false;
    } catch {
      // Fall back to the full snapshot write below.
    }
    if (needsNativeSnapshotBridge) {
      await this.writeNativeSnapshot(snapshot);
    }
    await this.fallback.saveSnapshot(snapshot);
  }
}

const vaultPersistenceTails = new Map<SyncProviderKind, Promise<unknown>>();

export function createLocalVaultAdapter(provider: SyncProviderKind): LocalVaultAdapter {
  const adapter: LocalVaultAdapter = canUseTauriNativeVault()
    ? new TauriVaultAdapter(provider)
    : new LocalStorageVaultAdapter(provider);
  return new Proxy(adapter, {
    get(target, key) {
      const value = Reflect.get(target, key);
      if (typeof value !== "function") return value;
      return (...args: unknown[]) => {
        const previous = vaultPersistenceTails.get(provider) ?? Promise.resolve();
        const next = previous.catch(() => undefined).then(() => Reflect.apply(value, target, args));
        vaultPersistenceTails.set(provider, next);
        return next;
      };
    },
  });
}
