import { api, captureOfflineServerContext, getApiBase } from "./api";
import {
  isOfflineReplayOrigin,
  sameOfflineReplayOrigin,
  verifyOfflineReplayContext,
  type OfflineReplayOrigin,
} from "./offlineReplayOrigin";
import { validateUploadAudience } from "./uploadOperationStore";
import type { SelectedAttachmentVersion } from "./nativeAttachmentProduct";
import type { AttachmentRecord } from "./dataClient";
export interface AttachmentCopyTarget {
  spaceId: string;
  documentId: string;
  draftId: string;
}
export interface AttachmentCopyOperation {
  format: 1;
  id: string;
  origin: OfflineReplayOrigin;
  source: {
    kind: "version_v1" | "legacy_snapshot_v1";
    documentId: string;
    versionId: string;
    rawSource: string;
  };
  target: AttachmentCopyTarget;
  requestJSON: string;
  state: "prepared" | "unknown" | "acknowledged" | "adopted";
  result?: AttachmentRecord;
}
const canonical = (v: unknown): v is string =>
  typeof v === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(v);
const nonempty = (v: unknown): v is string =>
  typeof v === "string" && v.length > 0 && v.length <= 1024;
function requestJSON(op: Pick<AttachmentCopyOperation, "id" | "source">) {
  return JSON.stringify({
    schema_version: 1,
    operation_id: op.id,
    source_document_id: op.source.documentId,
    version_id: op.source.versionId,
    ...(op.source.kind === "version_v1"
      ? { expected_source_version: JSON.parse(op.source.rawSource) }
      : { expected_legacy_source: JSON.parse(op.source.rawSource) }),
  });
}
export function validateAttachmentCopyOperation(
  value: unknown,
): asserts value is AttachmentCopyOperation {
  const op = value as AttachmentCopyOperation;
  if (
    !op ||
    op.format !== 1 ||
    !canonical(op.id) ||
    !isOfflineReplayOrigin(op.origin) ||
    !op.target ||
    !nonempty(op.target.spaceId) ||
    !nonempty(op.target.documentId) ||
    !canonical(op.target.draftId) ||
    !op.source ||
    !nonempty(op.source.documentId) ||
    !nonempty(op.source.versionId) ||
    !["version_v1", "legacy_snapshot_v1"].includes(op.source.kind) ||
    typeof op.source.rawSource !== "string" ||
    new TextEncoder().encode(op.source.rawSource).length > 65536 ||
    !["prepared", "unknown", "acknowledged", "adopted"].includes(op.state)
  )
    throw new Error("Original copy operation invalid; retain it for review.");
  const v = JSON.parse(op.source.rawSource);
  if (
    !v ||
    (op.source.kind === "version_v1"
      ? !canonical(op.source.versionId) ||
        v.version_id !== op.source.versionId ||
        !canonical(v.logical_attachment_id) ||
        !Number.isSafeInteger(v.version_number) ||
        v.version_number < 1 ||
        !Number.isSafeInteger(v.byte_length) ||
        v.byte_length < 0
      : v.kind !== "legacy_snapshot_v1" ||
        v.asset_id !== op.source.versionId ||
        v.space_id !== op.target.spaceId ||
        v.document_id !== op.source.documentId ||
        !Number.isSafeInteger(v.size_bytes) ||
        v.size_bytes < 0 ||
        !nonempty(v.created_at) ||
        !nonempty(v.storage_path)) ||
    !nonempty(v.original_name) ||
    !nonempty(v.mime_type) ||
    typeof v.binary_sha256 !== "string" ||
    !/^[a-f0-9]{64}$/.test(v.binary_sha256) ||
    op.requestJSON !== requestJSON(op)
  )
    throw new Error("Original copy source proof differs; nothing dispatched.");
  if (op.state === "acknowledged" || op.state === "adopted")
    validateAttachmentCopyResult(op, op.result);
  else if (op.result !== undefined)
    throw new Error("Unacknowledged copy contains an invalid result.");
}
export function validateAttachmentCopyResult(
  op: AttachmentCopyOperation,
  result: unknown,
): asserts result is AttachmentRecord {
  const r = result as AttachmentRecord,
    v = JSON.parse(op.source.rawSource);
  validateUploadAudience(op.target.draftId, r?.audience);
  if (
    !r ||
    !canonical(r.id) ||
    r.id === op.source.versionId ||
    r.document_id !== op.target.documentId ||
    r.original_name !== v.original_name ||
    r.mime_type !== v.mime_type ||
    r.size_bytes !== (op.source.kind === "version_v1" ? v.byte_length : v.size_bytes) ||
    !nonempty(r.filename) ||
    !nonempty(r.url) ||
    !nonempty(r.created_at)
  )
    throw new Error("Copy acknowledgement differs; retain the original operation.");
}
function db(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === "undefined") {
      reject(new Error("Durable copy storage unavailable; nothing copied."));
      return;
    }
    const r = indexedDB.open("starlane-attachment-copy-operations", 1);
    r.onupgradeneeded = () => r.result.createObjectStore("operations", { keyPath: "id" });
    r.onsuccess = () => resolve(r.result);
    r.onerror = r.onblocked = () =>
      reject(new Error("Durable copy storage unavailable; retain original work."));
  });
}
async function read(id: string): Promise<AttachmentCopyOperation> {
  const d = await db();
  try {
    return await new Promise((resolve, reject) => {
      const t = d.transaction("operations", "readonly"),
        r = t.objectStore("operations").get(id);
      t.oncomplete = () => {
        try {
          validateAttachmentCopyOperation(r.result);
          resolve(r.result);
        } catch (e) {
          reject(e);
        }
      };
      t.onabort = t.onerror = () => reject(new Error("Original copy operation unreadable."));
    });
  } finally {
    d.close();
  }
}
function sameIntent(a: AttachmentCopyOperation, b: AttachmentCopyOperation) {
  return (
    a.id === b.id &&
    sameOfflineReplayOrigin(a.origin, b.origin) &&
    JSON.stringify(a.source) === JSON.stringify(b.source) &&
    JSON.stringify(a.target) === JSON.stringify(b.target) &&
    a.requestJSON === b.requestJSON
  );
}
async function write(op: AttachmentCopyOperation, initial = false) {
  validateAttachmentCopyOperation(op);
  const d = await db();
  try {
    await new Promise<void>((resolve, reject) => {
      const t = d.transaction("operations", "readwrite"),
        s = t.objectStore("operations");
      if (initial) s.add(op);
      else {
        const r = s.get(op.id);
        r.onsuccess = () => {
          try {
            validateAttachmentCopyOperation(r.result);
            const prior = r.result;
            if (
              !sameIntent(prior, op) ||
              (prior.state === "adopted" && op.state !== "adopted") ||
              ((prior.state === "acknowledged" || prior.state === "adopted") &&
                JSON.stringify(prior.result) !== JSON.stringify(op.result))
            )
              throw new Error();
            s.put(op);
          } catch {
            t.abort();
          }
        };
      }
      t.oncomplete = () => resolve();
      t.onabort = t.onerror = () =>
        reject(new Error("Copy operation persistence failed; retain original work."));
    });
  } finally {
    d.close();
  }
  const held = await read(op.id);
  if (JSON.stringify(held) !== JSON.stringify(op))
    throw new Error("Copy operation durability could not be confirmed.");
}
export async function prepareAttachmentCopy(
  selection: SelectedAttachmentVersion,
  target: AttachmentCopyTarget,
): Promise<string> {
  const context = captureOfflineServerContext();
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  if (
    !sameOfflineReplayOrigin(context.origin, selection.origin) ||
    selection.serverPage.space_id !== target.spaceId
  )
    throw new Error("Original copy source context changed.");
  const rawVersion = selection.rawVersion,
    version = JSON.parse(rawVersion),
    id = crypto.randomUUID();
  const op: AttachmentCopyOperation = {
    format: 1,
    id,
    origin: { ...context.origin },
    source: {
      documentId: selection.serverPage.document_id,
      kind: "version_v1",
      versionId: version.version_id,
      rawSource: rawVersion,
    },
    target: { ...target },
    requestJSON: "",
    state: "prepared",
  };
  op.requestJSON = requestJSON(op);
  validateAttachmentCopyOperation(op);
  await write(op, true);
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  return id;
}
export interface LegacyAttachmentCopySelection {
  rawLegacySource: string;
  origin: OfflineReplayOrigin;
  serverPage: { space_id: string; document_id: string };
}
/** Server creates a byte-bound legacy proof; no fabricated immutable version record. */
export async function fetchLegacyAttachmentCopySource(
  spaceId: string,
  documentId: string,
  assetId: string,
): Promise<LegacyAttachmentCopySelection> {
  if (!nonempty(spaceId) || !nonempty(documentId) || !nonempty(assetId))
    throw new Error("Original legacy source identity required.");
  const context = captureOfflineServerContext(),
    verify = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  verify();
  const response = await fetch(
    context.origin.serverBase +
      `/spaces/${encodeURIComponent(spaceId)}/documents/${encodeURIComponent(documentId)}/attachment-copy-source/${encodeURIComponent(assetId)}`,
    {
      headers: { Authorization: `Bearer ${context.bearer}` },
      credentials: "omit",
      redirect: "error",
    },
  );
  verify();
  if (!response.ok || !response.body)
    throw new Error("Original legacy copy proof unavailable; nothing copied.");
  const reader = response.body.getReader(),
    decoder = new TextDecoder("utf-8", { fatal: true });
  let raw = "",
    length = 0;
  try {
    for (;;) {
      const next = await reader.read();
      verify();
      if (next.done) break;
      length += next.value.length;
      if (length > 65536) throw new Error("Original legacy proof exceeds review bound.");
      raw += decoder.decode(next.value, { stream: true });
    }
    raw += decoder.decode();
  } finally {
    await reader.cancel().catch(() => undefined);
    reader.releaseLock();
  }
  const proof = JSON.parse(raw);
  if (
    proof.kind !== "legacy_snapshot_v1" ||
    proof.asset_id !== assetId ||
    proof.space_id !== spaceId ||
    proof.document_id !== documentId
  )
    throw new Error("Original legacy source association differs.");
  verify();
  return {
    rawLegacySource: raw,
    origin: { ...context.origin },
    serverPage: { space_id: spaceId, document_id: documentId },
  };
}
export async function prepareLegacyAttachmentCopy(
  selection: LegacyAttachmentCopySelection,
  target: AttachmentCopyTarget,
): Promise<string> {
  const context = captureOfflineServerContext();
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  if (
    !sameOfflineReplayOrigin(context.origin, selection.origin) ||
    selection.serverPage.space_id !== target.spaceId
  )
    throw new Error("Original legacy copy context changed.");
  const rawSource = selection.rawLegacySource,
    v = JSON.parse(rawSource),
    id = crypto.randomUUID();
  const op: AttachmentCopyOperation = {
    format: 1,
    id,
    origin: { ...context.origin },
    source: {
      kind: "legacy_snapshot_v1",
      documentId: selection.serverPage.document_id,
      versionId: v.asset_id,
      rawSource,
    },
    target: { ...target },
    requestJSON: "",
    state: "prepared",
  };
  op.requestJSON = requestJSON(op);
  validateAttachmentCopyOperation(op);
  await write(op, true);
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  return id;
}
const inFlight = new Set<string>();
export async function runAttachmentCopy(id: string): Promise<AttachmentRecord> {
  if (!canonical(id) || inFlight.has(id))
    throw new Error("Original copy is invalid or already in progress.");
  inFlight.add(id);
  try {
    const context = captureOfflineServerContext(),
      verify = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    verify();
    let op = await read(id);
    verify();
    if (!sameOfflineReplayOrigin(context.origin, op.origin))
      throw new Error("Original copy account or session changed.");
    if (op.result) {
      validateAttachmentCopyResult(op, op.result);
      return op.result;
    }
    op = { ...op, state: "unknown" };
    await write(op);
    verify();
    const t = op.target;
    const result = await api.fetchOriginalContext<AttachmentRecord>(
      context,
      `/spaces/${encodeURIComponent(t.spaceId)}/documents/${encodeURIComponent(t.documentId)}/drafts/${t.draftId}/attachments/copy`,
      { method: "POST", body: JSON.parse(op.requestJSON) },
    );
    verify();
    validateAttachmentCopyResult(op, result);
    await write({ ...op, state: "acknowledged", result });
    verify();
    return result;
  } finally {
    inFlight.delete(id);
  }
}
export async function markAttachmentCopyAdopted(id: string, target: AttachmentCopyTarget) {
  const context = captureOfflineServerContext();
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  const op = await read(id);
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  if (
    !sameOfflineReplayOrigin(context.origin, op.origin) ||
    op.target.spaceId !== target.spaceId ||
    op.target.documentId !== target.documentId ||
    op.target.draftId !== target.draftId ||
    !op.result
  )
    throw new Error("Original copied file cannot be adopted in this draft.");
  await write({ ...op, state: "adopted" });
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
}
/** Explicit reviewed use. The caller owns insertion guards, including its final
 * post-insertion scope check. Successful insertion may close its own dialog;
 * that unmount must not discard this operation's durable adoption receipt. */
export async function adoptReviewedAttachmentCopy(
  id: string,
  target: AttachmentCopyTarget,
  actions: {
    verifySelection: () => void;
    isReferenced: (versionId: string) => boolean;
    onUse: (attachment: AttachmentRecord) => Promise<void>;
  },
): Promise<void> {
  const originalTarget = { ...target },
    context = captureOfflineServerContext();
  const verifyContext = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  verifyContext();
  actions.verifySelection();
  const original = await read(id);
  verifyContext();
  actions.verifySelection();
  if (
    !sameOfflineReplayOrigin(original.origin, context.origin) ||
    original.target.spaceId !== originalTarget.spaceId ||
    original.target.documentId !== originalTarget.documentId ||
    original.target.draftId !== originalTarget.draftId
  )
    throw new Error("Original copied file belongs to another draft.");
  const attachment = await runAttachmentCopy(id);
  verifyContext();
  actions.verifySelection();
  if (!actions.isReferenced(attachment.id)) await actions.onUse(attachment);
  // onUse already proves its original editor scope after insertion. Only the
  // original auth and stored target remain required across its own dialog close.
  verifyContext();
  await markAttachmentCopyAdopted(id, originalTarget);
  verifyContext();
}
export async function retainedAttachmentCopies(
  target: AttachmentCopyTarget,
): Promise<AttachmentCopyOperation[]> {
  const context = captureOfflineServerContext(),
    verify = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  verify();
  const d = await db();
  try {
    const rows = await new Promise<AttachmentCopyOperation[]>((resolve, reject) => {
      const t = d.transaction("operations", "readonly"),
        r = t.objectStore("operations").openCursor(),
        found: AttachmentCopyOperation[] = [];
      let scanned = 0;
      r.onsuccess = () => {
        const c = r.result;
        if (!c) return;
        try {
          if (++scanned > 1000) throw new Error();
          validateAttachmentCopyOperation(c.value);
          const op = c.value;
          if (
            sameOfflineReplayOrigin(op.origin, context.origin) &&
            op.target.spaceId === target.spaceId &&
            op.target.documentId === target.documentId &&
            op.target.draftId === target.draftId &&
            op.state !== "adopted"
          ) {
            if (found.length === 100) throw new Error();
            found.push(op);
          }
          c.continue();
        } catch {
          t.abort();
        }
      };
      t.oncomplete = () => resolve(found);
      t.onabort = t.onerror = () =>
        reject(new Error("Retained copy review unavailable or exceeds its review bound."));
    });
    verify();
    return rows;
  } finally {
    d.close();
  }
}
