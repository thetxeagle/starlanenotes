import type { LocalVaultSnapshot, VaultQueuedSyncOperation } from "@/lib/localVault";
import type { SyncProviderKind } from "@/lib/storageProviders";

export type NativeLocalDatabaseStoreKind = "sqlite" | "snapshot-json";
export type ServerReplayBackend = "sqlite" | "postgresql";
export type ReplayTransport = "server-api";

export interface NativeLocalDatabaseDescriptor {
  schema_version: number;
  provider: SyncProviderKind;
  store_kind: NativeLocalDatabaseStoreKind;
  database_path?: string;
  snapshot_path?: string;
  supports_snapshot_bridge: boolean;
  supports_atomic_snapshot_commit?: boolean;
  supports_lossless_replay_queue?: boolean;
  supports_reviewed_move_evidence?: boolean;
  supports_reviewed_move_destination_binding?: boolean;
  supports_reviewed_draft_transfer_evidence?: boolean;
  supports_operation_queue: boolean;
  replay_backends: ServerReplayBackend[];
}

export interface NativeLocalDatabaseCommitReceipt {
  canonical_committed: true;
  canonical_revision: string;
  mirror_pending: boolean;
  mirror_error: string | null;
}

export interface NativeLocalDatabaseSnapshotRequest {
  provider: SyncProviderKind;
}

// Copy from the descriptor accepted by the preview; never refresh to rebase.
export interface NativeSnapshotExpectedDestination {
  schema_version: 1;
  provider: "none";
  store_kind: "sqlite";
  database_path: string;
  supports_reviewed_move_evidence: true;
}

export interface NativeLocalDatabaseSnapshotCommitRequest extends NativeLocalDatabaseSnapshotWriteRequest {
  expectedRevision: string | null;
  expectedDestination?: NativeSnapshotExpectedDestination;
}

export interface NativeLocalDatabaseSnapshotReadRequest extends NativeLocalDatabaseSnapshotRequest {
  expectedDestination?: NativeSnapshotExpectedDestination;
}

export interface NativeLocalDatabaseSnapshotWriteRequest extends NativeLocalDatabaseSnapshotRequest {
  snapshot: LocalVaultSnapshot;
}

export type NativeLocalDatabaseRecordCollection =
  | "spaces"
  | "folders"
  | "documents"
  | "drafts"
  | "assets"
  | "templates"
  | "workflow_templates"
  | "document_workflows"
  | "document_workflow_events"
  | "property_schemas"
  | "property_values"
  | "tags"
  | "doc_tags"
  | "search_index"
  | "reviewed_move_records"
  | "reviewed_draft_transfer_records"
  | "sync_state"
  | "queued_operations";

export interface NativeLocalDatabaseRecordMutationRequest extends NativeLocalDatabaseSnapshotRequest {
  collection: NativeLocalDatabaseRecordCollection;
  id: string;
}

export interface NativeLocalDatabaseRecordUpsertRequest extends NativeLocalDatabaseRecordMutationRequest {
  payload: unknown;
  rowOrder?: number;
}

export interface NativeLocalDatabaseReplayContract {
  schema_version: number;
  operation_order: "queued_at_ascending";
  idempotency_key: "operation.id";
  transport: ReplayTransport;
  server_backends: ServerReplayBackend[];
  required_payload_status: "available";
  conflict_policy: "server_validates_version_and_permissions";
  web_canonical_store: "server-api";
}

export interface NativeLocalDatabaseReplayReadiness {
  ready: boolean;
  ready_operation_count: number;
  blocked_operation_count: number;
  blockers: string[];
  target_backends: ServerReplayBackend[];
}

export const NATIVE_LOCAL_DATABASE_SCHEMA_VERSION = 1;
export const SERVER_REPLAY_BACKENDS: ServerReplayBackend[] = ["sqlite", "postgresql"];

export const NATIVE_LOCAL_DATABASE_COMMANDS = {
  describe: "describe_native_local_database",
  readSnapshot: "read_native_local_database_snapshot",
  writeSnapshot: "write_native_local_database_snapshot",
  commitSnapshot: "commit_native_local_database_snapshot",
  upsertRecord: "upsert_native_local_database_record",
  deleteRecord: "delete_native_local_database_record",
  syncMirror: "sync_native_local_database_mirror",
  reset: "reset_native_local_database",
  packReplayAsset: "pack_native_replay_asset_payload",
} as const;

export function buildNativeLocalDatabaseReplayContract(): NativeLocalDatabaseReplayContract {
  return {
    schema_version: NATIVE_LOCAL_DATABASE_SCHEMA_VERSION,
    operation_order: "queued_at_ascending",
    idempotency_key: "operation.id",
    transport: "server-api",
    server_backends: SERVER_REPLAY_BACKENDS,
    required_payload_status: "available",
    conflict_policy: "server_validates_version_and_permissions",
    web_canonical_store: "server-api",
  };
}

export function sortReplayOperationsForContract<
  T extends Pick<VaultQueuedSyncOperation, "queued_at" | "id">,
>(operations: T[]): T[] {
  return [...operations].sort((left, right) => {
    const byQueuedAt = left.queued_at.localeCompare(right.queued_at);
    return byQueuedAt === 0 ? left.id.localeCompare(right.id) : byQueuedAt;
  });
}

export function preflightReplayOperationsForContract(
  operations: Array<
    Pick<VaultQueuedSyncOperation, "id"> & { payload_status: "available" | "missing" }
  >,
  contract: NativeLocalDatabaseReplayContract = buildNativeLocalDatabaseReplayContract(),
): NativeLocalDatabaseReplayReadiness {
  const blockers: string[] = [];
  const missingPayloadIds = operations
    .filter((operation) => operation.payload_status !== contract.required_payload_status)
    .map((operation) => operation.id);

  if (missingPayloadIds.length > 0) {
    blockers.push(`Missing replay payloads: ${missingPayloadIds.join(", ")}`);
  }

  if (
    !contract.server_backends.includes("sqlite") ||
    !contract.server_backends.includes("postgresql")
  ) {
    blockers.push("Replay contract must support both SQLite and PostgreSQL server backends");
  }

  return {
    ready: blockers.length === 0,
    ready_operation_count: operations.length - missingPayloadIds.length,
    blocked_operation_count: missingPayloadIds.length,
    blockers,
    target_backends: contract.server_backends,
  };
}

/** Exact original desktop destination projection; no refreshed descriptor rebasing. */
export function captureReviewedMoveDestination(descriptor: NativeLocalDatabaseDescriptor | null): NativeSnapshotExpectedDestination {
  if (!descriptor || descriptor.schema_version !== 1 || descriptor.provider !== "none" || descriptor.store_kind !== "sqlite" || typeof descriptor.database_path !== "string" || !descriptor.database_path.trim() || descriptor.supports_snapshot_bridge !== true || descriptor.supports_atomic_snapshot_commit !== true || descriptor.supports_reviewed_move_evidence !== true || descriptor.supports_reviewed_move_destination_binding !== true) throw new Error("Reviewed native moves require an updated desktop shell with original-destination binding");
  return Object.freeze({ schema_version: 1, provider: "none", store_kind: "sqlite", database_path: descriptor.database_path, supports_reviewed_move_evidence: true });
}

/** Exact JSON value equality: object key order is irrelevant, arrays/fields/types are not. */
export function nativeJsonValueEquals(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (left === null || right === null || typeof left !== "object" || typeof right !== "object") return false;
  if (Array.isArray(left) || Array.isArray(right)) return Array.isArray(left) && Array.isArray(right) && left.length === right.length && left.every((value, index) => nativeJsonValueEquals(value, right[index]));
  const a = left as Record<string, unknown>, b = right as Record<string, unknown>;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every(key => Object.prototype.hasOwnProperty.call(b, key) && nativeJsonValueEquals(a[key], b[key]));
}
