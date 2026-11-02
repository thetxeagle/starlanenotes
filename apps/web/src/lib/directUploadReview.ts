import { api, captureOfflineServerContext, getApiBase } from "./api";
import {
  isOfflineReplayOrigin,
  sameOfflineReplayOrigin,
  verifyOfflineReplayContext,
  type OfflineReplayOrigin,
} from "./offlineReplayOrigin";
import {
  readUploadOperation,
  uploadDraftHeaders,
  validateUploadAudience,
  setUploadOperationState,
  validateUploadAcknowledgement,
  type UploadOperation,
} from "./uploadOperationStore";
import type { AttachmentRecord } from "./dataClient";
interface Review {
  version: 1 | 2;
  draftId?:string;
  operationId: string;
  origin: OfflineReplayOrigin;
  spaceId: string;
  documentId: string;
  originalName: string;
  mimeType: string;
  size: number;
  state: "prepared" | "unknown" | "acknowledged" | "adopted";
  ack: AttachmentRecord | null;
}
const canonical = (id: string) =>
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id);
const same = (a: Review, b: Review) =>
  a.version===b.version && a.draftId===b.draftId &&
  a.operationId === b.operationId &&
  sameOfflineReplayOrigin(a.origin, b.origin) &&
  a.spaceId === b.spaceId &&
  a.documentId === b.documentId &&
  a.originalName === b.originalName &&
  a.mimeType === b.mimeType &&
  a.size === b.size;
function valid(v: unknown): v is Review {
  if (!v || typeof v !== "object") return false;
  const r = v as Review;
  return (
    ((r.version===1&&r.draftId===undefined)||(r.version===2&&typeof r.draftId==="string"&&canonical(r.draftId))) &&
    canonical(r.operationId) &&
    isOfflineReplayOrigin(r.origin) &&
    typeof r.spaceId === "string" &&
    !!r.spaceId &&
    typeof r.documentId === "string" &&
    !!r.documentId &&
    typeof r.originalName === "string" &&
    !!r.originalName.trim() &&
    typeof r.mimeType === "string" &&
    Number.isSafeInteger(r.size) &&
    r.size >= 0 &&
    ["prepared", "unknown", "acknowledged", "adopted"].includes(r.state) &&
    (r.state === "acknowledged" || r.state === "adopted" ? !!r.ack : r.ack === null)
  );
}
/** Server URLs use the space slug; original page authority remains bound to the stored UUID/context. */
export function validDirectUploadAcknowledgementPath(
  documentId: string,
  ack: Pick<AttachmentRecord, "id" | "filename" | "url">,
): boolean {
  if (!canonical(ack.id) || !ack.filename.startsWith(ack.id + ".")) return false;
  const extension = ack.filename.slice(ack.id.length + 1);
  if (!/^[A-Za-z0-9]{1,16}$/.test(extension) || !documentId || /[\\/\0?#%]/.test(documentId)) return false;
  const prefix = "/api/v1/files/spaces/", suffix = `/attachments/${documentId}/${ack.filename}`;
  if (!ack.url.startsWith(prefix) || !ack.url.endsWith(suffix)) return false;
  const slug = ack.url.slice(prefix.length, -suffix.length);
  return !!slug && slug !== "." && slug !== ".." && !/[\\/\0?#%]/.test(slug);
}
function assertAck(r: Review, ack: AttachmentRecord): void {
  validateUploadAudience(r.draftId,ack.audience);
  if (
    !ack ||
    typeof ack !== "object" ||
    !canonical(ack.id) ||
    ack.document_id !== r.documentId ||
    ack.original_name !== r.originalName ||
    ack.mime_type !== r.mimeType ||
    ack.size_bytes !== r.size ||
    typeof ack.filename !== "string" ||
    !ack.filename.startsWith(ack.id + ".") ||
    typeof ack.url !== "string" ||
    !validDirectUploadAcknowledgementPath(r.documentId, ack) ||
    typeof ack.created_at !== "string" ||
    !ack.created_at
  )
    throw new Error("Retained upload acknowledgement differs from original request");
  if (new TextEncoder().encode(JSON.stringify(ack)).length > 65536)
    throw new Error("Retained upload acknowledgement exceeds bound");
}
function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === "undefined") {
      reject(new Error("Durable upload review storage unavailable"));
      return;
    }
    const request = indexedDB.open("starlane-direct-upload-review", 1);
    request.onupgradeneeded = () =>
      request.result.createObjectStore("reviews", { keyPath: "operationId" });
    request.onsuccess = () => resolve(request.result);
    request.onerror = request.onblocked = () =>
      reject(new Error("Durable upload review storage unavailable"));
  });
}
async function read(id: string): Promise<Review> {
  if (!canonical(id)) throw new Error("Original upload identity invalid");
  const db = await open();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction("reviews", "readonly"),
        get = tx.objectStore("reviews").get(id);
      tx.oncomplete = () => {
        if (!valid(get.result)) {
          reject(new Error("Original upload review unavailable"));
          return;
        }
        const r = get.result;
        if (r.ack)
          try {
            assertAck(r, r.ack);
          } catch (error) {
            reject(error);
            return;
          }
        resolve(r);
      };
      tx.onabort = tx.onerror = () => reject(new Error("Original upload review could not be read"));
    });
  } finally {
    db.close();
  }
}
async function write(expected: Review, next: Review, create = false): Promise<void> {
  const db = await open();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("reviews", "readwrite"),
        store = tx.objectStore("reviews"),
        get = store.get(expected.operationId);
      get.onsuccess = () => {
        const prior = get.result as unknown;
        if (prior === undefined) {
          if (create) {
            store.add(next);
            return;
          }
          tx.abort();
          return;
        }
        if (!valid(prior) || !same(prior, expected)) {
          tx.abort();
          return;
        }
        if (create) return;
        if (prior.ack) {
          if (!next.ack || JSON.stringify(prior.ack) !== JSON.stringify(next.ack)) {
            tx.abort();
            return;
          }
          if (prior.state === "adopted" && next.state !== "adopted") {
            tx.abort();
            return;
          }
        }
        store.put(next);
      };
      tx.oncomplete = () => resolve();
      tx.onabort = tx.onerror = () =>
        reject(new Error("Original upload review changed or could not be persisted"));
    });
  } finally {
    db.close();
  }
}
function original(operation: UploadOperation): Review {
  return {
    version: operation.version,
    ...(operation.version===2?{draftId:operation.draftId}:{}),
    operationId: operation.operationId,
    origin: { ...operation.origin },
    spaceId: operation.spaceId,
    documentId: operation.documentId,
    originalName: operation.originalName,
    mimeType: operation.mimeType,
    size: operation.size,
    state: "prepared",
    ack: null,
  };
}
export async function retainDirectUploadReview(operation: UploadOperation): Promise<void> {
  const r = original(operation);
  if (!valid(r)) throw new Error("Original upload review invalid");
  await write(r, r, true);
}
export async function markDirectUploadUnknown(operation: UploadOperation): Promise<void> {
  const r = await read(operation.operationId);
  if (!same(r, original(operation))) throw new Error("Original upload review context differs");
  await write(r, { ...r, state: "unknown", ack: null });
}
export async function acknowledgeDirectUpload(
  operation: UploadOperation,
  ack: AttachmentRecord,
): Promise<void> {
  validateUploadAcknowledgement(operation, ack);
  const r = await read(operation.operationId);
  if (!same(r, original(operation))) throw new Error("Original upload review context differs");
  assertAck(r, ack);
  await write(r, { ...r, state: "acknowledged", ack: { ...ack } });
}
const acknowledgedOperations = new WeakMap<object, string>();
export function rememberDirectUploadAcknowledgement(
  ack: AttachmentRecord,
  operationId: string,
): void {
  if (!canonical(operationId)) throw new Error("Original upload identity invalid");
  acknowledgedOperations.set(ack, operationId);
}
export function directUploadAcknowledgementId(ack: object): string | null {
  return acknowledgedOperations.get(ack) ?? null;
}

export interface RetainedDirectUpload {
  operationId: string;
  name: string;
  size: number;
  state: Review["state"];
}
export async function retainedDirectUploads(page: {
  space_id: string;
  document_id: string;
  draft_id?:string;
}, referencedVersionIds: ReadonlySet<string> = new Set()): Promise<RetainedDirectUpload[]> {
  const context = captureOfflineServerContext(),
    check = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  check();
  const db = await open();
  try {
    const rows = await new Promise<RetainedDirectUpload[]>((resolve, reject) => {
      const tx = db.transaction("reviews", "readonly"),
        cursor = tx.objectStore("reviews").openCursor(),
        rows: RetainedDirectUpload[] = [];
      let scanned = 0;
      cursor.onsuccess = () => {
        const item = cursor.result;
        if (!item) return;
        if (++scanned > 10000) {
          tx.abort();
          return;
        }
        const r = item.value as unknown;
        if (!valid(r)) {
          tx.abort();
          return;
        }
        if (
          r.spaceId === page.space_id &&
          r.documentId === page.document_id &&
          (r.version===1||r.draftId===page.draft_id) &&
          sameOfflineReplayOrigin(r.origin, context.origin)
        ) {
          if (r.ack)
            try {
              assertAck(r, r.ack);
            } catch {
              tx.abort();
              return;
            }
          if (r.ack && referencedVersionIds.has(r.ack.id)) {
            item.continue();
            return;
          }
          if (rows.length >= 100) {
            tx.abort();
            return;
          }
          rows.push({
            operationId: r.operationId,
            name: r.originalName,
            size: r.size,
            state: r.state,
          });
        }
        item.continue();
      };
      tx.oncomplete = () => resolve(rows);
      tx.onabort = tx.onerror = () =>
        reject(
          new Error(
            "Retained direct uploads require manual recovery; records invalid or beyond bound",
          ),
        );
    });
    check();
    return rows;
  } finally {
    db.close();
  }
}
/** Caller explicitly reviews an original operation. Known ACK needs neither HTTP nor Blob read. */
export async function reviewRetainedDirectUpload(
  page: { space_id: string; document_id: string; draft_id?:string },
  operationId: string,
): Promise<AttachmentRecord> {
  const context = captureOfflineServerContext(),
    check = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  check();
  let r = await read(operationId);
  check();
  if (
    !sameOfflineReplayOrigin(r.origin, context.origin) ||
    r.spaceId !== page.space_id ||
    r.documentId !== page.document_id ||
    (r.version===2&&r.draftId!==page.draft_id)
  )
    throw new Error("Upload belongs to another original account, session or page");
  if (r.ack) {
    assertAck(r, r.ack);
    return { ...r.ack };
  }
  const operation = await readUploadOperation(operationId, context.origin);
  check();
  if (!same(r, original(operation))) throw new Error("Original upload Blob metadata differs");
  if (operation.state === "prepared") await setUploadOperationState(operation, "unknown");
  check();
  await markDirectUploadUnknown(operation);
  check();
  const body = new FormData();
  body.append("file", operation.bytes, operation.originalName);
  const ack = await api.fetchOriginalContext<AttachmentRecord>(
    context,
    `/spaces/${encodeURIComponent(operation.spaceId)}/documents/${encodeURIComponent(operation.documentId)}/attachments`,
    {
      method: "POST",
      body,
      headers: {
        "X-Starlane-Upload-Operation": operation.operationId,
        "X-Starlane-Upload-Length": String(operation.size),
        ...uploadDraftHeaders(operation),
      },
    },
  );
  check();
  await acknowledgeDirectUpload(operation, ack);
  check();
  if (operation.state !== "acknowledged")
    await setUploadOperationState({ ...operation, state: "unknown" }, "acknowledged");
  check();
  r = await read(operationId);
  check();
  if (!r.ack) throw new Error("Original upload acknowledgement unavailable");
  return { ...r.ack };
}
export async function markDirectUploadAdopted(
  page: { space_id: string; document_id: string; draft_id?:string },
  operationId: string,
): Promise<void> {
  const context = captureOfflineServerContext();
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  const r = await read(operationId);
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  if (
    !sameOfflineReplayOrigin(r.origin, context.origin) ||
    r.spaceId !== page.space_id ||
    r.documentId !== page.document_id ||
    (r.version===2&&r.draftId!==page.draft_id) ||
    !r.ack
  )
    throw new Error("Original acknowledged upload required before adoption");
  await write(r, { ...r, state: "adopted" });
}
