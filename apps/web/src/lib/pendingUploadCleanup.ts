import { api, captureOfflineServerContext, getApiBase } from "./api";
import {
  isOfflineReplayOrigin,
  sameOfflineReplayOrigin,
  verifyOfflineReplayContext,
  type OfflineReplayContext,
  type OfflineReplayOrigin,
} from "./offlineReplayOrigin";

export interface PendingUploadRow {
  attempt_id: string;
  producer_sha256: string;
  claim_sha256: string;
  producer_kind: "original_direct" | "original_replay";
  original_operation_id: string;
}
export interface PendingUploadSelection {
  readonly spaceId: string;
  readonly documentId: string;
  readonly row: Readonly<PendingUploadRow>;
}
export interface PendingUploadDiscovery {
  availability: "available" | "not_available";
  reason: string | null;
  attempts: PendingUploadSelection[];
}
export interface PendingUploadCleanupResult {
  attempt_id: string;
  outcome: "pending" | "retained_review" | "physically_completed";
  reason: string | null;
  disposition_sha256: string | null;
}
interface Receipt {
  version: 1;
  origin: OfflineReplayOrigin;
  spaceId: string;
  documentId: string;
  row: PendingUploadRow;
  result: PendingUploadCleanupResult | null;
}
const key = "starlane_pending_upload_cleanup_v1",
  limit = 131072;
const owned = new WeakMap<PendingUploadSelection, { context: OfflineReplayContext }>();
const object = (v: unknown): v is Record<string, unknown> =>
  !!v && typeof v === "object" && !Array.isArray(v);
const exact = (v: Record<string, unknown>, keys: string[]) =>
  Object.keys(v).length === keys.length &&
  keys.every((k) => Object.prototype.hasOwnProperty.call(v, k));
const uuid = (v: unknown): v is string =>
  typeof v === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v);
const sha = (v: unknown): v is string => typeof v === "string" && /^[0-9a-f]{64}$/.test(v);
const reason = (v: unknown): v is string | null =>
  v === null || (typeof v === "string" && /^[a-z][a-z0-9_]{0,127}$/.test(v));
const id = (v: unknown): v is string =>
  typeof v === "string" && v.length > 0 && v.length <= 1024 && !/[\u0000-\u001f\u007f]/.test(v);
function row(v: unknown): PendingUploadRow {
  if (
    !object(v) ||
    !exact(v, [
      "attempt_id",
      "producer_sha256",
      "claim_sha256",
      "producer_kind",
      "original_operation_id",
    ]) ||
    !uuid(v.attempt_id) ||
    !sha(v.producer_sha256) ||
    !sha(v.claim_sha256) ||
    (v.producer_kind !== "original_direct" && v.producer_kind !== "original_replay") ||
    !id(v.original_operation_id) ||
    (v.producer_kind === "original_direct" && !uuid(v.original_operation_id))
  )
    throw new Error("Original reserved upload evidence is malformed; retain the request");
  return structuredClone(v) as unknown as PendingUploadRow;
}
export function parsePendingUploadDiscovery(v: unknown): {
  availability: "available" | "not_available";
  reason: string | null;
  attempts: PendingUploadRow[];
} {
  if (
    !object(v) ||
    !exact(v, ["availability", "reason", "attempts"]) ||
    (v.availability !== "available" && v.availability !== "not_available") ||
    !reason(v.reason) ||
    !Array.isArray(v.attempts) ||
    v.attempts.length > 128 ||
    (v.availability === "available" && v.reason !== null) ||
    (v.availability === "not_available" && (v.reason === null || v.attempts.length !== 0))
  )
    throw new Error(
      "Reserved upload discovery is unproven; an unavailable list cannot establish absence",
    );
  const attempts = v.attempts.map(row);
  if (new Set(attempts.map((r) => r.attempt_id)).size !== attempts.length)
    throw new Error("Original reserved upload identities repeat; retain evidence");
  return { availability: v.availability, reason: v.reason, attempts };
}
export function parsePendingUploadCleanupResult(
  v: unknown,
  attempt: string,
): PendingUploadCleanupResult {
  if (
    !object(v) ||
    !exact(v, ["attempt_id", "outcome", "reason", "disposition_sha256"]) ||
    v.attempt_id !== attempt ||
    !uuid(attempt) ||
    (v.outcome !== "pending" &&
      v.outcome !== "retained_review" &&
      v.outcome !== "physically_completed") ||
    !reason(v.reason) ||
    (v.outcome === "physically_completed" && (v.reason !== null || !sha(v.disposition_sha256))) ||
    (v.outcome !== "physically_completed" && (v.reason === null || v.disposition_sha256 !== null))
  )
    throw new Error(
      "Original upload cleanup acknowledgment is unproven; retain the original request",
    );
  return structuredClone(v) as unknown as PendingUploadCleanupResult;
}
function selected(
  context: OfflineReplayContext,
  spaceId: string,
  documentId: string,
  r: PendingUploadRow,
): PendingUploadSelection {
  if (!id(spaceId) || !id(documentId)) throw new Error("Original page selection unavailable");
  const selection = Object.freeze({ spaceId, documentId, row: Object.freeze(row(r)) });
  owned.set(selection, { context });
  return selection;
}
function verifier(context: OfflineReplayContext, verifyCurrent: () => void) {
  return () => {
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    verifyCurrent();
  };
}
async function boundedResponse(response: Response, verify: () => void): Promise<unknown> {
  verify();
  if (response.status !== 200 || !response.body)
    throw new Error("Original server response unavailable; retain the request");
  const declared = response.headers.get("Content-Length");
  if (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > 65536))
    throw new Error("Original response exceeds the supported bound");
  const reader = response.body.getReader();
  const parts: Uint8Array[] = [];
  let length = 0;
  try {
    for (;;) {
      verify();
      const next = await reader.read();
      verify();
      if (next.done) break;
      length += next.value.byteLength;
      if (length > 65536) throw new Error("Original response exceeds the supported bound");
      parts.push(next.value.slice());
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const part of parts) {
      bytes.set(part, offset);
      offset += part.byteLength;
    }
    verify();
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
    verify();
  }
}
function path(space: string, document: string) {
  if (!id(space) || !id(document)) throw new Error("Original page selection unavailable");
  return `/spaces/${encodeURIComponent(space)}/documents/${encodeURIComponent(document)}/attachment-upload-attempts`;
}
export async function discoverPendingUploads(
  space: string,
  document: string,
  verifyCurrent: () => void,
): Promise<PendingUploadDiscovery> {
  const context = captureOfflineServerContext(),
    verify = verifier(context, verifyCurrent);
  verify();
  const response = await fetch(context.origin.serverBase + path(space, document), {
    headers: { Authorization: `Bearer ${context.bearer}` },
  });
  verify();
  const parsed = parsePendingUploadDiscovery(await boundedResponse(response, verify));
  verify();
  return { ...parsed, attempts: parsed.attempts.map((r) => selected(context, space, document, r)) };
}
function readReceipts(): Receipt[] {
  const text = localStorage.getItem(key);
  if (text === null) return [];
  if (text.length > limit)
    throw new Error("Saved original cleanup requests exceed the supported bound; retain them");
  const value: unknown = JSON.parse(text);
  if (!Array.isArray(value) || value.length > 128)
    throw new Error("Saved original cleanup requests are malformed; retain them");
  const ids = new Set<string>();
  return value.map((v) => {
    if (
      !object(v) ||
      !exact(v, ["version", "origin", "spaceId", "documentId", "row", "result"]) ||
      v.version !== 1 ||
      !isOfflineReplayOrigin(v.origin) ||
      !id(v.spaceId) ||
      !id(v.documentId)
    )
      throw new Error("Saved original cleanup context is malformed; retain it");
    const r = row(v.row),
      identity = JSON.stringify([v.origin, r.attempt_id]);
    if (ids.has(identity)) throw new Error("Saved original cleanup identities repeat");
    ids.add(identity);
    return {
      version: 1,
      origin: structuredClone(v.origin),
      spaceId: v.spaceId,
      documentId: v.documentId,
      row: r,
      result: v.result === null ? null : parsePendingUploadCleanupResult(v.result, r.attempt_id),
    };
  });
}
function saveReceipts(receipts: Receipt[], verify: () => void) {
  verify();
  const text = JSON.stringify(receipts);
  if (receipts.length > 128 || text.length > limit)
    throw new Error("Saved original cleanup inventory is full; no request was sent");
  localStorage.setItem(key, text);
  verify();
  if (localStorage.getItem(key) !== text)
    throw new Error("Original cleanup receipt durability is unproven; no new request may be sent");
}
export function retainedPendingUploadCleanups(
  space: string,
  document: string,
  verifyCurrent: () => void,
): { attempts: PendingUploadSelection[]; otherContextCount: number } {
  const context = captureOfflineServerContext(),
    verify = verifier(context, verifyCurrent);
  verify();
  const receipts = readReceipts();
  const matches = receipts.filter(
    (r) =>
      sameOfflineReplayOrigin(r.origin, context.origin) &&
      r.spaceId === space &&
      r.documentId === document,
  );
  verify();
  return {
    attempts: matches.map((r) => selected(context, space, document, r.row)),
    otherContextCount: receipts.length - matches.length,
  };
}
export async function cleanupPendingUpload(
  selection: PendingUploadSelection,
  verifyCurrent: () => void,
): Promise<PendingUploadCleanupResult> {
  const owner = owned.get(selection);
  if (!owner)
    throw new Error("Select an original discovered or retained upload request before cleanup");
  const { context } = owner,
    verify = verifier(context, verifyCurrent);
  verify();
  if (!navigator.locks?.request)
    throw new Error(
      "Original cleanup requires cross-window serialization; retain the original request",
    );
  return navigator.locks.request(key, async () => {
    verify();
    const original: Receipt = {
      version: 1,
      origin: structuredClone(context.origin),
      spaceId: selection.spaceId,
      documentId: selection.documentId,
      row: row(selection.row),
      result: null,
    };
    const receipts = readReceipts(),
      index = receipts.findIndex(
        (r) =>
          sameOfflineReplayOrigin(r.origin, original.origin) &&
          r.row.attempt_id === original.row.attempt_id,
      );
    if (index >= 0) {
      if (JSON.stringify({ ...receipts[index], result: null }) !== JSON.stringify(original))
        throw new Error("Retained original request differs; no replacement cleanup may be created");
    } else receipts.push(original);
    saveReceipts(receipts, verify);
    const response = await fetch(
      context.origin.serverBase +
        path(selection.spaceId, selection.documentId) +
        `/${selection.row.attempt_id}/cleanup`,
      {
        method: "POST",
        headers: { Authorization: `Bearer ${context.bearer}`, "Content-Type": "application/json" },
        body: JSON.stringify({
          producer_sha256: selection.row.producer_sha256,
          claim_sha256: selection.row.claim_sha256,
        }),
      },
    );
    verify();
    const result = parsePendingUploadCleanupResult(
      await boundedResponse(response, verify),
      selection.row.attempt_id,
    );
    verify();
    const fresh = readReceipts(),
      match = fresh.findIndex(
        (r) =>
          sameOfflineReplayOrigin(r.origin, original.origin) &&
          r.row.attempt_id === original.row.attempt_id,
      );
    if (match < 0 || JSON.stringify({ ...fresh[match], result: null }) !== JSON.stringify(original))
      throw new Error("Original cleanup receipt changed; retain the unknown outcome");
    if (result.outcome === "physically_completed") fresh.splice(match, 1);
    else fresh[match].result = result;
    saveReceipts(fresh, verify);
    verify();
    return result;
  });
}
