import { captureNativeArchiveLocalSession } from "./nativeArchiveCurrentContext";
import {
  assertLocalVaultSnapshot,
  type LocalVaultSnapshot,
  type VaultAsset,
  type VaultQueuedSyncOperation,
} from "./localVault";

const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
const TOKEN = /^[a-f0-9]{64}$/;
const DIGEST = /^[a-f0-9]{64}$/;
const LIMIT = 100 * 1024 * 1024;
const CHUNK = 65536;
const STORES = ["originals", "progress", "files"];
const encodeNativeAssetPath = (path: string): string =>
  encodeURIComponent(path).replace(/[!'()*]/g, character =>
    `%${character.charCodeAt(0).toString(16).toUpperCase()}`,
  );
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const equal = (a: unknown, b: unknown): boolean =>
  JSON.stringify(a, (_key, value: unknown) =>
    value && typeof value === "object" && !Array.isArray(value)
      ? Object.fromEntries(Object.entries(value).sort(([x], [y]) => x.localeCompare(y)))
      : value,
  ) ===
  JSON.stringify(b, (_key, value: unknown) =>
    value && typeof value === "object" && !Array.isArray(value)
      ? Object.fromEntries(Object.entries(value).sort(([x], [y]) => x.localeCompare(y)))
      : value,
  );

type Binding = Awaited<ReturnType<ReturnType<typeof captureNativeArchiveLocalSession>["describe"]>>;
type SelectedDraft = {
  space_id: string;
  document_id: string;
  draft_id: string;
  draft_created_at: string;
};
interface SourceProof {
  schema_version: 1;
  source_path: string;
  stamp: Record<string, number>;
  binary_sha256: string;
}
interface OriginalRequest extends SelectedDraft {
  operation_id: string;
  name: string;
  mime_type: string;
  byte_length: number;
  original_binding: Binding;
  path_source?: SourceProof;
}
interface OriginalSelection {
  format: 1;
  operationId: string;
  selected: SelectedDraft;
  source: { kind: "blob"; name: string; mime_type: string; byte_length: number } | { kind: "path"; path: string };
}
interface Progress {
  operationId: string;
  state: "selection_retained" | "transfer_unknown" | "canonical_committed";
  request: OriginalRequest | null;
  received: number;
  acknowledgement: unknown;
}

export class NativeLocalImportAdoptionError extends Error {
  constructor(readonly operationId: string, readonly asset: VaultAsset, cause: unknown) {
    super("The original native import committed but could not be adopted in the current context.");
    this.name = "NativeLocalImportAdoptionError";
    Object.assign(this, { cause });
  }
}

async function invoke(command: string, args: Record<string, unknown>): Promise<unknown> {
  const native = (window as unknown as {
    __TAURI_INTERNALS__?: { invoke(command: string, args: Record<string, unknown>): Promise<unknown> };
  }).__TAURI_INTERNALS__;
  if (!native?.invoke) throw new Error("Updated native local import creator unavailable");
  try {
    return await native.invoke(command, args);
  } catch (error) {
    throw error instanceof Error
      ? error
      : new Error(typeof error === "string" ? error : "Native local import operation unavailable");
  }
}

async function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === "undefined") {
      reject(new Error("Durable original local import journal unavailable"));
      return;
    }
    let rejected = false;
    const request = indexedDB.open("starlane-native-local-import-originals", 1);
    request.onupgradeneeded = () => {
      for (const name of STORES) {
        if (!request.result.objectStoreNames.contains(name)) request.result.createObjectStore(name, { keyPath: "operationId" });
      }
    };
    request.onsuccess = () => {
      const db = request.result;
      db.onversionchange = () => db.close();
      if (rejected) db.close();
      else resolve(db);
    };
    request.onerror = request.onblocked = () => {
      rejected = true;
      reject(new Error("Durable original local import journal unavailable"));
    };
  });
}

async function retainSelection(selection: OriginalSelection, blob?: Blob): Promise<void> {
  const db = await database();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORES, "readwrite");
      tx.objectStore("originals").add(selection);
      tx.objectStore("progress").add({
        operationId: selection.operationId,
        state: "selection_retained",
        request: null,
        received: 0,
        acknowledgement: null,
      } satisfies Progress);
      if (blob) tx.objectStore("files").add({ operationId: selection.operationId, blob });
      tx.oncomplete = () => resolve();
      tx.onabort = tx.onerror = () => reject(new Error("Original local import selection could not be retained"));
    });
  } finally {
    db.close();
  }
}

async function retainProgress(progress: Progress, releaseBlob = false): Promise<void> {
  const db = await database();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORES, "readwrite");
      tx.objectStore("progress").put(progress);
      if (releaseBlob) tx.objectStore("files").delete(progress.operationId);
      tx.oncomplete = () => resolve();
      tx.onabort = tx.onerror = () => reject(new Error("Original local import progress could not be retained"));
    });
  } finally {
    db.close();
  }
}

async function retained(operationId: string): Promise<{ selection: OriginalSelection; progress: Progress }> {
  if (!UUID.test(operationId)) throw new Error("Exact original local import operation required");
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORES, "readonly");
      const original = tx.objectStore("originals").get(operationId);
      const progress = tx.objectStore("progress").get(operationId);
      let failure: Error | null = null;
      tx.oncomplete = () => {
        try {
          const selection = original.result as OriginalSelection | undefined;
          const state = progress.result as Progress | undefined;
          if (!selection || selection.format !== 1 || selection.operationId !== operationId || !state || state.operationId !== operationId || !["selection_retained", "transfer_unknown", "canonical_committed"].includes(state.state) || !Number.isSafeInteger(state.received) || state.received < 0) {
            throw new Error("Original local import journal is invalid or unavailable");
          }
          resolve({ selection, progress: state });
        } catch (error) {
          reject(error);
        }
      };
      tx.onabort = tx.onerror = () => reject(failure ?? new Error("Original local import journal unavailable"));
      original.onerror = progress.onerror = () => { failure = new Error("Original local import journal unavailable"); tx.abort(); };
    });
  } finally {
    db.close();
  }
}

function validSelected(selected: SelectedDraft): boolean {
  return (
    Object.keys(selected).length === 4 &&
    [selected.space_id, selected.document_id, selected.draft_id].every(
      value => typeof value === "string" && value.length > 0 && value.length <= 256 && !/[\\/\0]/.test(value),
    ) &&
    typeof selected.draft_created_at === "string" &&
    selected.draft_created_at.length > 0 &&
    selected.draft_created_at.length <= 256
  );
}

function checkedSourceProof(value: unknown, path: string, byteLength: number): SourceProof {
  const proof = value as SourceProof;
  const stampKeys = ["device", "inode", "uid", "mode", "links", "length", "ctime", "ctime_nsec", "mtime", "mtime_nsec"];
  if (
    !proof ||
    Object.keys(proof).length !== 4 ||
    proof.schema_version !== 1 ||
    proof.source_path !== path ||
    !DIGEST.test(proof.binary_sha256) ||
    !proof.stamp ||
    Object.keys(proof.stamp).length !== stampKeys.length ||
    !stampKeys.every(key => Number.isSafeInteger(proof.stamp[key]) && proof.stamp[key] >= 0) ||
    proof.stamp.length !== byteLength
  ) {
    throw new Error("Original native path source acknowledgement differs");
  }
  return clone(proof);
}

export function checkedNativeLocalImportAcknowledgement(
  value: unknown,
  request: OriginalRequest,
): { asset: VaultAsset; operation: VaultQueuedSyncOperation; snapshot: LocalVaultSnapshot } {
  if (new TextEncoder().encode(JSON.stringify(value)).length > 16 * 1024 * 1024) {
    throw new Error("Native local import acknowledgement exceeds bound");
  }
  const result = value as {
    format?: unknown;
    operation_id?: unknown;
    canonical_committed?: unknown;
    asset?: VaultAsset;
    canonical_snapshot?: unknown;
    receipt_json?: unknown;
  };
  assertLocalVaultSnapshot(result.canonical_snapshot);
  const asset = result.asset;
  const expectedPath = `${request.original_binding.root}/assets/local-imports/asset-import-${request.operation_id}.bin`;
  const assetKeys = ["created_at", "document_id", "id", "mime_type", "name", "size", "source", "space_id", "url", "vault_path"];
  if (
    Object.keys(result).length !== 6 ||
    result.format !== 1 ||
    result.operation_id !== request.operation_id ||
    result.canonical_committed !== true ||
    !asset ||
    !equal(Object.keys(asset).sort(), assetKeys) ||
    asset.id !== `asset-import-${request.operation_id}` ||
    asset.name !== request.name ||
    asset.mime_type !== request.mime_type ||
    asset.size !== request.byte_length ||
    asset.space_id !== request.space_id ||
    asset.document_id !== request.document_id ||
    asset.source !== "tauri" ||
    asset.vault_path !== expectedPath ||
    asset.url !== `asset://localhost/${encodeNativeAssetPath(expectedPath)}` ||
    typeof asset.created_at !== "string" ||
    !asset.created_at ||
    typeof result.receipt_json !== "string" ||
    new TextEncoder().encode(result.receipt_json).length > 65536 ||
    !result.canonical_snapshot.assets.some(row => equal(row, asset))
  ) {
    throw new Error("Native original local import acknowledgement differs");
  }
  const receipt = JSON.parse(result.receipt_json) as Record<string, unknown>;
  const receiptKeys = ["asset", "canonical_committed", "canonical_revision", "creator", "creator_directory_stamp", "format", "operation_id", "queued_operation", "request", "request_sha256", "seal_file", "seal_json"];
  const operation = receipt.queued_operation as VaultQueuedSyncOperation;
  const expectedOperation: VaultQueuedSyncOperation = {
    id: `syncop-import-${request.operation_id}`,
    entity_type: "asset",
    entity_id: asset.id,
    action: "create",
    queued_at: asset.created_at,
    attempts: 0,
    payload: {
      id: asset.id,
      name: asset.name,
      mime_type: asset.mime_type,
      size: asset.size,
      url: asset.url,
      vault_path: asset.vault_path,
      source: asset.source,
      created_at: asset.created_at,
      space_id: request.space_id,
      document_id: request.document_id,
      draft_id: request.draft_id,
      local_draft_source: {
        schema_version: 1,
        space_id: request.space_id,
        document_id: request.document_id,
        draft_id: request.draft_id,
        draft_created_at: request.draft_created_at,
      },
    },
  };
  const seal = typeof receipt.seal_json === "string" ? JSON.parse(receipt.seal_json) as Record<string, unknown> : null;
  if (
    !equal(Object.keys(receipt).sort(), receiptKeys) ||
    receipt.format !== 1 ||
    receipt.operation_id !== request.operation_id ||
    !equal(receipt.request, request) ||
    receipt.canonical_committed !== true ||
    !equal(receipt.asset, asset) ||
    !equal(operation, expectedOperation) ||
    !result.canonical_snapshot.sync_state?.queued_operations.some(row => equal(row, operation)) ||
    !seal ||
    seal.operation_id !== request.operation_id ||
    !equal(seal.asset, asset) ||
    typeof seal.binary_sha256 !== "string" ||
    !DIGEST.test(seal.binary_sha256) ||
    (request.path_source && seal.binary_sha256 !== request.path_source.binary_sha256)
  ) {
    throw new Error("Native original local import receipt differs");
  }
  return { asset: clone(asset), operation: clone(operation), snapshot: result.canonical_snapshot };
}

function checkedPrepared(value: unknown, operationId: string): string {
  const result = value as Record<string, unknown>;
  if (
    !result ||
    result.format !== 1 ||
    result.operation_id !== operationId ||
    typeof result.producer_token !== "string" ||
    !TOKEN.test(result.producer_token)
  ) throw new Error("Original native local import prepare acknowledgement differs");
  return result.producer_token;
}

async function commitAndAdopt(
  result: unknown,
  progress: Progress,
  verify: () => void,
  admit: (snapshot: LocalVaultSnapshot, verify: () => void) => Promise<void>,
): Promise<{ operationId: string; asset: VaultAsset }> {
  if (!progress.request) throw new Error("Original local import request unavailable");
  const checked = checkedNativeLocalImportAcknowledgement(result, progress.request);
  progress.state = "canonical_committed";
  progress.acknowledgement = result;
  progress.received = progress.request.byte_length;
  await retainProgress(progress, true);
  try {
    verify();
    await admit(checked.snapshot, verify);
    verify();
  } catch (error) {
    throw new NativeLocalImportAdoptionError(progress.operationId, checked.asset, error);
  }
  return { operationId: progress.operationId, asset: checked.asset };
}

async function streamBlob(
  blob: Blob,
  request: OriginalRequest,
  progress: Progress,
  session: ReturnType<typeof captureNativeArchiveLocalSession>,
  verify: () => void,
  admit: (snapshot: LocalVaultSnapshot, verify: () => void) => Promise<void>,
) {
  let response = await invoke("prepare_original_native_local_import", {
    provider: session.context.provider,
    context: session.context,
    current: session.context,
    request,
  });
  verify();
  let token = checkedPrepared(response, request.operation_id);
  for (let index = 0, offset = 0; offset < blob.size; index += 1) {
    verify();
    const bytes = new Uint8Array(await blob.slice(offset, Math.min(offset + CHUNK, blob.size)).arrayBuffer());
    verify();
    response = await invoke("append_original_native_local_import", {
      producerToken: token,
      current: session.context,
      index,
      bytes: Array.from(bytes),
    });
    verify();
    const ack = response as Record<string, unknown>;
    offset += bytes.length;
    if (ack.format !== 1 || ack.operation_id !== request.operation_id || ack.index !== index || ack.received !== offset || typeof ack.producer_token !== "string" || !TOKEN.test(ack.producer_token)) {
      throw new Error("Original native local import chunk acknowledgement differs");
    }
    token = ack.producer_token;
    progress.received = offset;
    await retainProgress(progress);
  }
  const result = await invoke("finish_original_native_local_import", { producerToken: token, current: session.context });
  return commitAndAdopt(result, progress, verify, admit);
}

export async function createOriginalNativeLocalImportFromBlob(
  blob: Blob,
  name: string,
  selected: SelectedDraft,
  verifyCurrent: () => void,
  admit: (snapshot: LocalVaultSnapshot, verify: () => void) => Promise<void>,
) {
  const mimeType = blob.type || "application/octet-stream";
  if (!(blob instanceof Blob) || !name.trim() || name.length > 255 || blob.size > LIMIT || !mimeType || mimeType.length > 255 || /[\x00-\x1f\x7f]/.test(mimeType) || !validSelected(selected)) {
    throw new Error("Select a valid local file no larger than 100 MiB");
  }
  const operationId = crypto.randomUUID();
  if (!UUID.test(operationId)) throw new Error("Original local import operation unavailable");
  await retainSelection({ format: 1, operationId, selected: clone(selected), source: { kind: "blob", name, mime_type: mimeType, byte_length: blob.size } }, blob);
  const session = captureNativeArchiveLocalSession();
  const verify = () => { session.verify(); verifyCurrent(); };
  verify();
  const binding = await session.describe();
  verify();
  const request: OriginalRequest = { operation_id: operationId, name, mime_type: mimeType, byte_length: blob.size, ...clone(selected), original_binding: clone(binding) };
  const progress: Progress = { operationId, state: "transfer_unknown", request, received: 0, acknowledgement: null };
  await retainProgress(progress);
  verify();
  return streamBlob(blob, request, progress, session, verify, admit);
}

export async function createOriginalNativeLocalImportFromPath(
  sourcePath: string,
  selected: SelectedDraft,
  verifyCurrent: () => void,
  admit: (snapshot: LocalVaultSnapshot, verify: () => void) => Promise<void>,
) {
  if (typeof sourcePath !== "string" || !sourcePath.startsWith("/") || sourcePath.length > 4096 || sourcePath.includes("\0") || !validSelected(selected)) {
    throw new Error("Select an exact absolute local file path");
  }
  const operationId = crypto.randomUUID();
  if (!UUID.test(operationId)) throw new Error("Original local import operation unavailable");
  await retainSelection({ format: 1, operationId, selected: clone(selected), source: { kind: "path", path: sourcePath } });
  const session = captureNativeArchiveLocalSession();
  const verify = () => { session.verify(); verifyCurrent(); };
  verify();
  const binding = await session.describe();
  verify();
  const metadata = await invoke("inspect_original_native_local_import_path_source", {
    provider: session.context.provider,
    context: session.context,
    current: session.context,
    originalBinding: binding,
    sourcePath,
  });
  verify();
  const inspected = metadata as Record<string, unknown>;
  if (Object.keys(inspected).length !== 4 || typeof inspected.name !== "string" || !inspected.name.trim() || inspected.name.length > 255 || typeof inspected.mime_type !== "string" || !inspected.mime_type || inspected.mime_type.length > 255 || !Number.isSafeInteger(inspected.byte_length) || Number(inspected.byte_length) < 0 || Number(inspected.byte_length) > LIMIT) {
    throw new Error("Original native path metadata acknowledgement differs");
  }
  const sourceProof = checkedSourceProof(inspected.source_selection, sourcePath, Number(inspected.byte_length));
  const baseRequest: OriginalRequest = {
    operation_id: operationId,
    name: inspected.name,
    mime_type: inspected.mime_type,
    byte_length: Number(inspected.byte_length),
    ...clone(selected),
    original_binding: clone(binding),
  };
  const progress: Progress = { operationId, state: "transfer_unknown", request: baseRequest, received: 0, acknowledgement: null };
  await retainProgress(progress);
  verify();
  const prepared = await invoke("prepare_original_native_local_import_path", {
    provider: session.context.provider,
    context: session.context,
    current: session.context,
    request: baseRequest,
    sourcePath,
    expectedSource: sourceProof,
  });
  verify();
  const token = checkedPrepared(prepared, operationId);
  const fullRequest = (prepared as Record<string, unknown>).request as OriginalRequest;
  if (!equal(fullRequest, { ...baseRequest, path_source: sourceProof })) throw new Error("Original native path request acknowledgement differs");
  progress.request = clone(fullRequest);
  await retainProgress(progress);
  verify();
  const result = await invoke("run_original_native_local_import_path", { producerToken: token, current: session.context });
  return commitAndAdopt(result, progress, verify, admit);
}

/** Read back only the exact retained operation. No new UUID, source selection,
 * queue payload, or current-root substitution is permitted during recovery. */
export async function reconcileOriginalNativeLocalImport(
  operationId: string,
  selected: SelectedDraft,
  verifyCurrent: () => void,
  admit: (snapshot: LocalVaultSnapshot, verify: () => void) => Promise<void>,
) {
  if (!validSelected(selected)) throw new Error("Exact original local import draft required");
  const row = await retained(operationId);
  if (!equal(row.selection.selected, selected) || !row.progress.request) throw new Error("Original local import source selection is unavailable");
  const session = captureNativeArchiveLocalSession();
  const verify = () => { session.verify(); verifyCurrent(); };
  verify();
  const binding = await session.describe();
  verify();
  if (!equal(binding, row.progress.request.original_binding)) throw new Error("Original local import belongs to another vault revision");
  if (row.selection.source.kind === "path" && !row.progress.request.path_source) {
    const recovered = await invoke("recover_original_native_local_import_path_request", {
      provider: session.context.provider,
      context: session.context,
      current: session.context,
      request: row.progress.request,
    });
    verify();
    const request = recovered as OriginalRequest;
    const proof = checkedSourceProof(request?.path_source, row.selection.source.path, row.progress.request.byte_length);
    if (!equal(request, { ...row.progress.request, path_source: proof })) throw new Error("Recovered original native path request differs");
    row.progress.request = clone(request);
    await retainProgress(row.progress);
    verify();
  }
  const result = await invoke("inspect_original_native_local_import", {
    provider: session.context.provider,
    context: session.context,
    current: session.context,
    originalBinding: row.progress.request.original_binding,
    operationId,
  });
  return commitAndAdopt(result, row.progress, verify, admit);
}
