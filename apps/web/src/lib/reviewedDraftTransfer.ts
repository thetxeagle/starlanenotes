import { api, getApiBase, isTauri } from "@/lib/api";
import { captureNativeArchiveLocalSession } from "@/lib/nativeArchiveCurrentContext";
import { nativeJsonValueEquals } from "@/lib/nativeLocalDatabase";
import { getSyncProvider } from "@/lib/storageProviders";
import {
  dataClient,
  publishedReplayFingerprint,
  type LocalSyncReplayDraftBindingSidecarRecord,
  type LocalSyncReplayManifestRecord,
  type ReviewedDraftTransferReceiptRecord,
  type ReviewedDraftTransferRequestRecord,
} from "@/lib/dataClient";
import type { VaultDocumentDraft } from "@/lib/localVault";

const DATABASE = "starlane-reviewed-draft-transfers";
const STORE = "records";
const VERSION = 1;

export interface ReviewedDraftTransferRecord {
  schema_version: 1;
  operation_id: string;
  state: "pending" | "unknown" | "confirmed";
  server_base: string;
  server_account_id: string;
  server_space_id: string;
  server_space_name: string;
  server_document_id: string;
  server_document_title: string;
  request: ReviewedDraftTransferRequestRecord;
  exact_manifest: LocalSyncReplayManifestRecord;
  receipt?: ReviewedDraftTransferReceiptRecord;
  binding?: LocalSyncReplayDraftBindingSidecarRecord;
  last_error?: string;
  prepared_at: string;
  updated_at: string;
}

type TargetSource = ReviewedDraftTransferRequestRecord["server_source"];

function copy<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function equal(left: unknown, right: unknown): boolean {
  return nativeJsonValueEquals(left, right);
}

async function digest(value: unknown): Promise<string> {
  return publishedReplayFingerprint(value);
}

async function openDatabase(): Promise<IDBDatabase> {
  if (typeof indexedDB === "undefined") throw new Error("Durable transfer review storage is unavailable; no request was sent");
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, VERSION);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(STORE)) request.result.createObjectStore(STORE, { keyPath: "operation_id" });
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("Durable transfer review storage failed to open"));
  });
}

async function putIndexedDbRecord(record: ReviewedDraftTransferRecord): Promise<ReviewedDraftTransferRecord> {
  const database = await openDatabase();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(STORE, "readwrite");
      transaction.objectStore(STORE).put(copy(record));
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(transaction.error ?? new Error("Durable transfer review write failed"));
      transaction.onabort = () => reject(transaction.error ?? new Error("Durable transfer review write was aborted"));
    });
    const saved = await readIndexedDbRecord(record.operation_id);
    if (!saved || !equal(saved, record)) throw new Error("Durable transfer review readback differs; no new request is permitted");
    return saved;
  } finally {
    database.close();
  }
}

async function readIndexedDbRecord(operationId: string): Promise<ReviewedDraftTransferRecord | null> {
  if (typeof indexedDB === "undefined") return null;
  const database = await openDatabase();
  try {
    return await new Promise((resolve, reject) => {
      const request = database.transaction(STORE, "readonly").objectStore(STORE).get(operationId);
      request.onsuccess = () => resolve(request.result ? copy(request.result as ReviewedDraftTransferRecord) : null);
      request.onerror = () => reject(request.error ?? new Error("Durable transfer review read failed"));
    });
  } finally {
    database.close();
  }
}

async function listIndexedDbRecords(): Promise<ReviewedDraftTransferRecord[]> {
  if (typeof indexedDB === "undefined") return [];
  const database = await openDatabase();
  try {
    const records = await new Promise<ReviewedDraftTransferRecord[]>((resolve, reject) => {
      const request = database.transaction(STORE, "readonly").objectStore(STORE).getAll();
      request.onsuccess = () => resolve((request.result as ReviewedDraftTransferRecord[]).map(copy));
      request.onerror = () => reject(request.error ?? new Error("Durable transfer review list failed"));
    });
    return records;
  } finally {
    database.close();
  }
}

function sameOriginal(left: ReviewedDraftTransferRecord, right: ReviewedDraftTransferRecord): boolean {
  return equal(left.request, right.request) && equal(left.exact_manifest, right.exact_manifest);
}

function currentCopy(left: ReviewedDraftTransferRecord, right: ReviewedDraftTransferRecord): ReviewedDraftTransferRecord {
  if (!sameOriginal(left, right)) throw new Error("Reviewed draft transfer journals conflict with the original request; retain both vault copies for review");
  if (left.updated_at === right.updated_at && !equal(left, right)) throw new Error("Reviewed draft transfer journals conflict at the same revision; retain both vault copies for review");
  return left.updated_at >= right.updated_at ? left : right;
}

async function storeRecord(record: ReviewedDraftTransferRecord): Promise<ReviewedDraftTransferRecord> {
  const portable = await dataClient.savePortableReviewedDraftTransfer(copy(record));
  const indexed = await putIndexedDbRecord(portable);
  if (!equal(indexed, portable)) throw new Error("Portable and device transfer journals differ; no new request is permitted");
  return portable;
}

export async function readReviewedDraftTransfer(operationId: string): Promise<ReviewedDraftTransferRecord | null> {
  const [indexed, portable] = await Promise.all([
    readIndexedDbRecord(operationId),
    dataClient.listPortableReviewedDraftTransfers().then(records => records.find(record => record.operation_id === operationId) ?? null),
  ]);
  if (!indexed) {
    if (portable) await putIndexedDbRecord(portable);
    return portable;
  }
  if (!portable) {
    await dataClient.savePortableReviewedDraftTransfer(indexed);
    return indexed;
  }
  const current = currentCopy(indexed, portable);
  if (!equal(current, portable)) await dataClient.savePortableReviewedDraftTransfer(current);
  if (!equal(current, indexed)) await putIndexedDbRecord(current);
  return copy(current);
}

export async function listReviewedDraftTransfers(): Promise<ReviewedDraftTransferRecord[]> {
  const [indexed, portable] = await Promise.all([
    listIndexedDbRecords(),
    dataClient.listPortableReviewedDraftTransfers(),
  ]);
  const byId = new Map(portable.map(record => [record.operation_id, record]));
  for (const record of indexed) {
    const other = byId.get(record.operation_id);
    byId.set(record.operation_id, other ? currentCopy(record, other) : record);
  }
  const records = [...byId.values()];
  for (const record of records) {
    if (!portable.some(candidate => equal(candidate, record))) await dataClient.savePortableReviewedDraftTransfer(record);
    if (!indexed.some(candidate => equal(candidate, record))) await putIndexedDbRecord(record);
  }
  return records.map(copy).sort((left, right) => right.updated_at.localeCompare(left.updated_at));
}

export async function currentLocalTransferRootIdentity(): Promise<string> {
  if (isTauri()) {
    const session = captureNativeArchiveLocalSession();
    const binding = await session.describe();
    session.verify();
    return `native-root:${await digest({ provider: getSyncProvider(), root: binding.root })}`;
  }
  return `browser-root:${await digest({ origin: window.location.origin, provider: getSyncProvider(), store: "local-vault" })}`;
}

export async function prepareReviewedDraftTransfer(input: {
  draft: VaultDocumentDraft;
  rootIdentity: string;
  serverAccountId: string;
  serverSpaceId: string;
  serverSpaceName: string;
  serverDocumentId: string;
  serverDocumentTitle: string;
  serverSource: TargetSource;
  selectedOperationIds: string[];
}): Promise<ReviewedDraftTransferRecord> {
  if (!crypto.randomUUID) throw new Error("Stable transfer operation identity is unavailable");
  if (!input.selectedOperationIds.length || new Set(input.selectedOperationIds).size !== input.selectedOperationIds.length)
    throw new Error("Select at least one exact original asset operation");
  const exactManifest = await dataClient.exportQueuedSyncReplayManifest(input.selectedOperationIds);
  if (exactManifest.operations.length !== input.selectedOperationIds.length || exactManifest.operations.some((operation) => operation.entity_type !== "asset" || operation.action !== "create" || operation.payload_status !== "available"))
    throw new Error("Selected transfer operations are not exact available asset creates");
  for (const operation of exactManifest.operations) {
    const payload = operation.payload as { draft_id?: unknown; local_draft_source?: Record<string, unknown> };
    const source = payload?.local_draft_source;
    if (!source || source.schema_version !== 1 || source.space_id !== input.draft.space_id || source.document_id !== input.draft.document_id || source.draft_id !== input.draft.id || source.draft_created_at !== input.draft.created_at || payload.draft_id !== input.draft.id)
      throw new Error("Selected asset operation differs from the exact original local draft");
  }
  const reviewed = {
    title: input.draft.title,
    icon: input.draft.icon ?? null,
    cover_image: input.draft.cover_image ?? null,
    content: input.draft.content,
  };
  const fingerprints = await Promise.all(exactManifest.operations.map((operation) => digest(operation.payload)));
  const request: ReviewedDraftTransferRequestRecord = {
    schema_version: 1,
    operation_id: crypto.randomUUID(),
    local_source: {
      provider: exactManifest.provider,
      root_identity: input.rootIdentity,
      space_id: input.draft.space_id,
      document_id: input.draft.document_id,
      draft_id: input.draft.id,
      draft_created_at: input.draft.created_at,
      snapshot_digest: await digest(reviewed),
    },
    server_source: copy(input.serverSource),
    reviewed_draft: reviewed,
    selected_operation_fingerprints: fingerprints,
  };
  const now = new Date().toISOString();
  return storeRecord({
    schema_version: 1,
    operation_id: request.operation_id,
    state: "pending",
    server_base: getApiBase(),
    server_account_id: input.serverAccountId,
    server_space_id: input.serverSpaceId,
    server_space_name: input.serverSpaceName,
    server_document_id: input.serverDocumentId,
    server_document_title: input.serverDocumentTitle,
    request,
    exact_manifest: exactManifest,
    prepared_at: now,
    updated_at: now,
  });
}

function bindingFor(record: ReviewedDraftTransferRecord, receipt: ReviewedDraftTransferReceiptRecord): LocalSyncReplayDraftBindingSidecarRecord {
  return {
    schema_version: 1,
    binding_count: record.exact_manifest.operations.length,
    bindings: record.exact_manifest.operations.map((operation, index) => ({
      operation_id: operation.id,
      operation_fingerprint: receipt.selected_operation_fingerprints[index],
      transfer_operation_id: receipt.operation_id,
      transfer_request_fingerprint: receipt.request_fingerprint,
      local_source: copy(receipt.local_source),
      target_space_id: receipt.server_space_id,
      target_document_id: receipt.server_document_id,
      target_draft_id: receipt.target_draft_id,
      target_draft_created_at: receipt.target_draft_created_at,
      target_source_digest: receipt.target_source_digest,
    })),
  };
}

export async function confirmReviewedDraftTransfer(
  record: ReviewedDraftTransferRecord,
): Promise<ReviewedDraftTransferRecord> {
  const retained = await readReviewedDraftTransfer(record.operation_id);
  if (!retained || !equal(retained.request, record.request) || !equal(retained.exact_manifest, record.exact_manifest))
    throw new Error("Exact retained transfer request is unavailable; no request was sent");
  if (getApiBase() !== retained.server_base || getSyncProvider() !== retained.exact_manifest.provider)
    throw new Error("Original transfer server or provider changed; restore it before retrying");
  const [currentAccount, currentRoot] = await Promise.all([
    api.fetch<{ id: string }>("/auth/me"),
    currentLocalTransferRootIdentity(),
  ]);
  if (currentAccount.id !== retained.server_account_id || currentRoot !== retained.request.local_source.root_identity)
    throw new Error("Original transfer account or storage root changed; restore it before retrying");
  try {
    const receipt = await dataClient.createReviewedDraftTransfer(retained.server_space_id, retained.server_document_id, retained.request);
    const confirmed: ReviewedDraftTransferRecord = {
      ...retained,
      state: "confirmed",
      receipt,
      binding: bindingFor(retained, receipt),
      last_error: undefined,
      updated_at: new Date().toISOString(),
    };
    return storeRecord(confirmed);
  } catch (error) {
    const unknown: ReviewedDraftTransferRecord = {
      ...retained,
      state: "unknown",
      last_error: error instanceof Error ? error.message : "Transfer outcome is unknown",
      updated_at: new Date().toISOString(),
    };
    await storeRecord(unknown);
    throw error;
  }
}

export function reviewedDraftTransferDownload(record: ReviewedDraftTransferRecord): unknown {
  return {
    schema_version: 1,
    exported_at: new Date().toISOString(),
    transfer: copy(record),
  };
}
