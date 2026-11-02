import { uploadDraftHeaders,validateUploadAudience,type DraftAttachmentAudience } from "./uploadOperationStore";
import { api, captureOfflineServerContext, getApiBase } from "./api";
import {
  sameOfflineReplayOrigin,
  verifyOfflineReplayContext,
  type OfflineReplayOrigin,
} from "./offlineReplayOrigin";
import type { SelectedAttachmentVersion } from "./nativeAttachmentProduct";
export interface ReplacementTarget {
  familyId: string;
  versionId: string;
  revision: number;
  spaceId: string;
  documentId: string;
}
interface Operation {
  format: 1 | 2;
  draftId?:string;
  id: string;
  origin: OfflineReplayOrigin;
  target: ReplacementTarget;
  bytes: Blob;
  name: string;
  state: "prepared" | "unknown" | "acknowledged";
  result?: ReplacementResult;
}
export interface ReplacementResult {
  audience?:DraftAttachmentAudience|null;
  expected: { id: string; current_version_id: string; revision: number };
  version: {
    version_id: string;
    logical_attachment_id: string;
    version_number: number;
    original_name: string;
    byte_length: number;
    mime_type: string;
    original_space_id: string;
    original_document_id: string;
    upload_operation_id: string;
  };
  origin: string;
}
const canonical = (s: string) =>
  /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(s);
function db(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === "undefined") {
      reject(new Error("Durable replacement storage unavailable; nothing uploaded."));
      return;
    }
    const request = indexedDB.open("starlane-attachment-replacement-operations", 1);
    request.onupgradeneeded = () =>
      request.result.createObjectStore("operations", { keyPath: "id" });
    request.onsuccess = () => resolve(request.result);
    request.onerror = request.onblocked = () =>
      reject(new Error("Durable replacement storage unavailable; retain original operation."));
  });
}
async function read(id: string): Promise<Operation> {
  const database = await db();
  try {
    return await new Promise((resolve, reject) => {
      const tx = database.transaction("operations", "readonly"),
        request = tx.objectStore("operations").get(id);
      tx.oncomplete = () => {
        if (!request.result) reject(new Error("Original replacement operation missing."));
        else resolve(request.result as Operation);
      };
      tx.onerror = tx.onabort = () =>
        reject(new Error("Original replacement operation unreadable."));
    });
  } finally {
    database.close();
  }
}
async function write(operation: Operation, initial = false) {
  const database = await db();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = database.transaction("operations", "readwrite"),
        store = tx.objectStore("operations");
      if (initial) store.add(operation);
      else {
        const request = store.get(operation.id);
        request.onsuccess = () => {
          const prior = request.result as Operation | undefined;
          if (
            !prior ||
            prior.id !== operation.id ||
            prior.format!==operation.format || prior.draftId!==operation.draftId ||
            !sameOfflineReplayOrigin(prior.origin, operation.origin) ||
            JSON.stringify(prior.target) !== JSON.stringify(operation.target) ||
            prior.name !== operation.name ||
            prior.bytes.size !== operation.bytes.size ||
            prior.bytes.type !== operation.bytes.type ||
            (prior.state === "acknowledged" &&
              (operation.state !== "acknowledged" ||
                JSON.stringify(prior.result) !== JSON.stringify(operation.result)))
          ) {
            tx.abort();
            return;
          }
          store.put(operation);
        };
      }
      tx.oncomplete = () => resolve();
      tx.onabort = tx.onerror = () =>
        reject(new Error("Replacement persistence failed; retain original operation."));
    });
  } finally {
    database.close();
  }
}
export async function prepareAttachmentReplacement(
  selection: SelectedAttachmentVersion,
  expectedRevision: number,
  file: File,
  draftId?:string,
): Promise<string> {
  const context = captureOfflineServerContext();
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  if (!sameOfflineReplayOrigin(context.origin, selection.origin))
    throw new Error("Selected attachment context changed.");
  const v = JSON.parse(selection.rawVersion);
  if (
    !canonical(v.logical_attachment_id) ||
    !canonical(v.version_id) ||
    !Number.isSafeInteger(expectedRevision) ||
    expectedRevision !== v.version_number ||
    expectedRevision < 1
  )
    throw new Error("Explicit current family head required; reload history.");
  if(draftId!==undefined&&!canonical(draftId))throw new Error("Original replacement draft identity invalid");
  const id = crypto.randomUUID();
  const operation: Operation = {
    format: draftId===undefined?1:2,
    ...(draftId===undefined?{}:{draftId}),
    id,
    origin: { ...context.origin },
    target: {
      familyId: v.logical_attachment_id,
      versionId: v.version_id,
      revision: expectedRevision,
      spaceId: selection.serverPage.space_id,
      documentId: selection.serverPage.document_id,
    },
    bytes: file,
    name: file.name,
    state: "prepared",
  };
  await write(operation, true);
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  return id;
}
export function validateReplacementResult(operation: Operation, result: ReplacementResult) {
  validateUploadAudience(operation.draftId,result?.audience);
  const t = operation.target,
    v = result?.version,
    h = result?.expected;
  if (
    !h ||
    !v ||
    h.id !== t.familyId ||
    h.current_version_id !== t.versionId ||
    h.revision !== t.revision ||
    v.logical_attachment_id !== t.familyId ||
    v.version_number !== t.revision + 1 ||
    v.original_name !== operation.name ||
    v.byte_length !== operation.bytes.size ||
    v.mime_type !== (operation.bytes.type || "application/octet-stream") ||
    v.original_space_id !== t.spaceId ||
    v.original_document_id !== t.documentId ||
    v.upload_operation_id !== operation.id ||
    !canonical(v.version_id) ||
    typeof result.origin !== "string" ||
    !result.origin
  )
    throw new Error("Replacement acknowledgement differs; original outcome remains unknown.");
}
export async function runAttachmentReplacement(id: string): Promise<ReplacementResult> {
  if (!canonical(id)) throw new Error("Original replacement identity invalid.");
  const context = captureOfflineServerContext(),
    verify = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  verify();
  let operation = await read(id);
  verify();
  if (
    !((operation.format===1&&operation.draftId===undefined)||(operation.format===2&&typeof operation.draftId==="string"&&canonical(operation.draftId))) ||
    operation.id !== id ||
    !sameOfflineReplayOrigin(context.origin, operation.origin) ||
    !(operation.bytes instanceof Blob) ||
    !operation.name ||
    !canonical(operation.target.familyId) ||
    !canonical(operation.target.versionId) ||
    !Number.isSafeInteger(operation.target.revision) ||
    operation.target.revision < 1
  )
    throw new Error("Original replacement binding invalid or context changed.");
  if (operation.state === "acknowledged") {
    if (!operation.result) throw new Error("Original replacement acknowledgement missing.");
    validateReplacementResult(operation, operation.result);
    return operation.result;
  }
  if (!["prepared", "unknown"].includes(operation.state))
    throw new Error("Original replacement state invalid.");
  operation = { ...operation, state: "unknown" };
  await write(operation);
  verify();
  const form = new FormData();
  form.append("file", operation.bytes, operation.name);
  const t = operation.target;
  const result = await api.fetchOriginalContext<ReplacementResult>(
    context,
    `/spaces/${encodeURIComponent(t.spaceId)}/documents/${encodeURIComponent(t.documentId)}/attachment-families/${t.familyId}/versions`,
    {
      method: "POST",
      body: form,
      headers: {
        "X-Starlane-Upload-Operation": id,
        ...uploadDraftHeaders({version:operation.format,draftId:operation.draftId}),
        "X-Starlane-Upload-Length": String(operation.bytes.size),
        "X-Starlane-Attachment-Version": t.versionId,
        "X-Starlane-Attachment-Revision": String(t.revision),
      },
    },
  );
  verify();
  validateReplacementResult(operation, result);
  await write({ ...operation, state: "acknowledged", result });
  verify();
  return result;
}
/** Discover retained work for this exact original page pin after a renderer restart. */
export async function retainedAttachmentReplacements(
  selection: SelectedAttachmentVersion,
  draftId?:string,
): Promise<Array<{ id: string; name: string; state: Operation["state"] }>> {
  const context = captureOfflineServerContext(),
    verify = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  verify();
  if (!sameOfflineReplayOrigin(context.origin, selection.origin))
    throw new Error("Original attachment selection changed.");
  const v = JSON.parse(selection.rawVersion),
    database = await db();
  try {
    const found = await new Promise<Array<{ id: string; name: string; state: Operation["state"] }>>(
      (resolve, reject) => {
        const rows: Array<{ id: string; name: string; state: Operation["state"] }> = [],
          tx = database.transaction("operations", "readonly");
        let scanned = 0;
        const request = tx.objectStore("operations").openCursor();
        request.onsuccess = () => {
          const cursor = request.result;
          if (!cursor) return;
          if (++scanned > 1000) {
            tx.abort();
            return;
          }
          const op = cursor.value as Operation;
          if (
            ((op.format===1&&op.draftId===undefined)||(op.format===2&&typeof op.draftId==="string"&&canonical(op.draftId)&&op.draftId===draftId)) &&
            sameOfflineReplayOrigin(op.origin, selection.origin) &&
            op.target.familyId === v.logical_attachment_id &&
            op.target.versionId === v.version_id &&
            op.target.spaceId === selection.serverPage.space_id &&
            op.target.documentId === selection.serverPage.document_id
          ) {
            if (rows.length === 100) {
              tx.abort();
              return;
            }
            rows.push({ id: op.id, name: op.name, state: op.state });
          }
          cursor.continue();
        };
        tx.oncomplete = () => resolve(rows);
        tx.onerror = tx.onabort = () =>
          reject(new Error("Retained replacement list unavailable or exceeds review bound."));
      },
    );
    verify();
    return found;
  } finally {
    database.close();
  }
}
