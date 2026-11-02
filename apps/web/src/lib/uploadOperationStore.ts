import { isOfflineReplayOrigin, sameOfflineReplayOrigin, type OfflineReplayOrigin } from "./offlineReplayOrigin";
interface UploadOperationFields {
  operationId: string;
  origin: OfflineReplayOrigin;
  spaceId: string;
  documentId: string;
  originalName: string;
  mimeType: string;
  size: number;
  bytes: Blob;
  state: "prepared" | "unknown" | "acknowledged";
}
export type UploadOperation = UploadOperationFields & ({version:1;draftId?:never}|{version:2;draftId:string});
export interface DraftAttachmentAudience {schema_version:2;draft_id:string;draft_created_at:string}
export function validateUploadAudience(draftId:string|undefined,audience:unknown):void {
  if(draftId===undefined){if(audience!==undefined&&audience!==null)throw new Error("Legacy upload acknowledgement changed audience");return;}
  const a=audience as Partial<DraftAttachmentAudience>|null;
  if(!canonical(draftId)||!a||a.schema_version!==2||a.draft_id!==draftId||typeof a.draft_created_at!=="string"||a.draft_created_at.length>128||!a.draft_created_at||a.draft_created_at.trim()!==a.draft_created_at||!Number.isFinite(Date.parse(a.draft_created_at)))
    throw new Error("Upload acknowledgement differs from the original draft audience; retain original outcome");
}
export function uploadDraftHeaders(operation:Pick<UploadOperation,"version"|"draftId">):Record<string,string>{
  if(operation.version===1&&operation.draftId===undefined)return {};
  if(operation.version===2&&typeof operation.draftId==="string"&&canonical(operation.draftId))return {"X-Starlane-Upload-Draft":operation.draftId};
  throw new Error("Original upload draft provenance invalid");
}
const canonical = (s: string) => /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(s);
function open(): Promise<IDBDatabase> {
  if (typeof indexedDB === "undefined") return Promise.reject(new Error("Durable upload storage is unavailable; upload was not dispatched"));
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("starlane-direct-upload-operations", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("operations", { keyPath: "operationId" });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(new Error("Durable upload storage could not be opened; upload was not dispatched"));
    request.onblocked = () => reject(new Error("Durable upload storage is blocked; upload was not dispatched"));
  });
}
export function isUploadOperation(value: unknown): value is UploadOperation {
  if (!value || typeof value !== "object") return false;
  const v = value as UploadOperation;
  return ((v.version===1&&v.draftId===undefined)||(v.version===2&&typeof v.draftId==="string"&&canonical(v.draftId))) && typeof v.operationId === "string" && canonical(v.operationId) && isOfflineReplayOrigin(v.origin) &&
    typeof v.spaceId === "string" && !!v.spaceId && typeof v.documentId === "string" && !!v.documentId &&
    typeof v.originalName === "string" && !!v.originalName.trim() && typeof v.mimeType === "string" &&
    Number.isSafeInteger(v.size) && v.size >= 0 && v.bytes instanceof Blob && v.bytes.size === v.size &&
    ["prepared", "unknown", "acknowledged"].includes(v.state);
}
export async function prepareUploadOperation(origin: OfflineReplayOrigin, spaceId: string, documentId: string, originalName: string, bytes: Blob, draftId?:string): Promise<UploadOperation> {
  if (!isOfflineReplayOrigin(origin) || typeof crypto.randomUUID !== "function") throw new Error("Upload origin or stable identity unavailable; upload was not dispatched");
  const operation: UploadOperation = { ...(draftId===undefined?{version:1 as const}:{version:2 as const,draftId}), operationId: crypto.randomUUID(), origin: { ...origin }, spaceId, documentId,
    originalName, mimeType: bytes.type || "application/octet-stream", size: bytes.size, bytes, state: "prepared" };
  if (!isUploadOperation(operation)) throw new Error("Upload operation metadata is invalid; upload was not dispatched");
  const db = await open();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("operations", "readwrite");
      // Native Blob structured clone: no arrayBuffer(), dataURL conversion, or full JS byte copy.
      tx.objectStore("operations").add(operation);
      tx.oncomplete = () => resolve();
      tx.onabort = tx.onerror = () => reject(new Error("Upload operation could not be persisted; upload was not dispatched"));
    });
  } finally { db.close(); }
  return operation;
}
export async function readUploadOperation(operationId: string, origin: OfflineReplayOrigin): Promise<UploadOperation> {
  if (!canonical(operationId)) throw new Error("Upload operation identity is invalid");
  const db = await open();
  try {
    const result = await new Promise<unknown>((resolve, reject) => {
      const tx = db.transaction("operations", "readonly"); const get = tx.objectStore("operations").get(operationId);
      tx.oncomplete = () => resolve(get.result); tx.onabort = tx.onerror = () => reject(new Error("Retained upload operation could not be read"));
    });
    if (!isUploadOperation(result) || !sameOfflineReplayOrigin(result.origin, origin)) throw new Error("Retained upload belongs to a different original context");
    return result;
  } finally { db.close(); }
}
export async function setUploadOperationState(expected: UploadOperation, state: UploadOperation["state"]): Promise<void> {
  const db = await open();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("operations", "readwrite"); const store = tx.objectStore("operations"); const get = store.get(expected.operationId);
      get.onsuccess = () => {
        const current: unknown = get.result;
        if (!isUploadOperation(current) || !sameOfflineReplayOrigin(current.origin, expected.origin) || current.spaceId !== expected.spaceId ||
          current.documentId !== expected.documentId || current.originalName !== expected.originalName || current.mimeType !== expected.mimeType ||
          current.size !== expected.size || current.version!==expected.version || current.draftId!==expected.draftId || current.state !== expected.state) { tx.abort(); return; }
        store.put({ ...current, state });
      };
      tx.oncomplete = () => resolve(); tx.onabort = tx.onerror = () => reject(new Error("Retained upload changed or persistence failed; preserve the original operation"));
    });
  } finally { db.close(); }
}

export function validateUploadAcknowledgement(operation: UploadOperation, result: unknown): void {
  if (!result || typeof result !== "object") throw new Error("Upload response invalid; original outcome remains unknown");
  const r = result as Record<string, unknown>;
  validateUploadAudience(operation.draftId,r.audience);
  if (typeof r.id !== "string" || !canonical(r.id) || r.document_id !== operation.documentId || r.original_name !== operation.originalName ||
    r.mime_type !== operation.mimeType || r.size_bytes !== operation.size || typeof r.filename !== "string" || !r.filename.startsWith(`${r.id}.`) ||
    typeof r.url !== "string" || !r.url.startsWith("/api/v1/files/spaces/") || !r.url.endsWith(`/attachments/${operation.documentId}/${r.filename}`) ||
    typeof r.created_at !== "string" || !r.created_at) throw new Error("Upload response differs from retained request; original outcome remains unknown");
}

// Preserve the original error class/status while exposing the retained UUID to callers.
const retainedFailures = new WeakMap<object, string>();
export function attachRetainedUploadOperation(error: unknown, operationId: string): unknown {
  const failure = error !== null && (typeof error === "object" || typeof error === "function") ? error as object : new Error(String(error));
  retainedFailures.set(failure, operationId); return failure;
}
export function retainedUploadOperationId(error: unknown): string | null {
  return error !== null && (typeof error === "object" || typeof error === "function") ? retainedFailures.get(error as object) ?? null : null;
}
