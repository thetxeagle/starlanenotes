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
  supports_operation_queue: boolean;
  replay_backends: ServerReplayBackend[];
}

export interface NativeLocalDatabaseSnapshotRequest {
  provider: SyncProviderKind;
}

export interface NativeLocalDatabaseSnapshotWriteRequest extends NativeLocalDatabaseSnapshotRequest {
  snapshot: LocalVaultSnapshot;
}

export type NativeLocalDatabaseRecordCollection =
  | "spaces"
  | "folders"
  | "documents"
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
  | "sync_state"
  | "queued_operations";

export interface NativeLocalDatabaseRecordMutationRequest extends NativeLocalDatabaseSnapshotRequest {
  collection: NativeLocalDatabaseRecordCollection;
  id: string;
}

export interface NativeLocalDatabaseRecordUpsertRequest
  extends NativeLocalDatabaseRecordMutationRequest {
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
  upsertRecord: "upsert_native_local_database_record",
  deleteRecord: "delete_native_local_database_record",
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

export function sortReplayOperationsForContract<T extends Pick<VaultQueuedSyncOperation, "queued_at" | "id">>(
  operations: T[],
): T[] {
  return [...operations].sort((left, right) => {
    const byQueuedAt = left.queued_at.localeCompare(right.queued_at);
    return byQueuedAt === 0 ? left.id.localeCompare(right.id) : byQueuedAt;
  });
}

export function preflightReplayOperationsForContract(
  operations: Array<Pick<VaultQueuedSyncOperation, "id"> & { payload_status: "available" | "missing" }>,
  contract: NativeLocalDatabaseReplayContract = buildNativeLocalDatabaseReplayContract(),
): NativeLocalDatabaseReplayReadiness {
  const blockers: string[] = [];
  const missingPayloadIds = operations
    .filter((operation) => operation.payload_status !== contract.required_payload_status)
    .map((operation) => operation.id);

  if (missingPayloadIds.length > 0) {
    blockers.push(`Missing replay payloads: ${missingPayloadIds.join(", ")}`);
  }

  if (!contract.server_backends.includes("sqlite") || !contract.server_backends.includes("postgresql")) {
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
