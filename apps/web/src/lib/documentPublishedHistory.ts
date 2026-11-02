import { getStorageSelection } from "./storageProviders";
import { getCurrentLaunchMode } from "./launchMode";
import { ApiError, captureOfflineServerContext, getApiBase, api } from "./api";
import { verifyOfflineReplayContext, type OfflineReplayContext } from "./offlineReplayOrigin";

// Agreed product-safe snapshot read contract; immutable legacy CRDT API stays separate.
export type PublicationSnapshotKind = "publication" | "baseline";
export type PublicationSnapshotBinding = "capture_v2" | "legacy_content_v1";
export type PublicationSnapshotExtraction = "exact_published_draft" | "proven_generated_header" | "raw_only";
export interface PublicationSnapshotSummary {
  schema_version: 1;
  id: string;
  document_id: string;
  captured_space_id: string;
  kind: PublicationSnapshotKind;
  capture_operation_id: string;
  publication_operation_id: string | null;
  published_at: string;
  captured_at: string;
  captured_by: string;
  snapshot_digest: string;
  binding: PublicationSnapshotBinding;
  content: {
    title: string;
    icon: string | null;
    cover_reference: string | null;
    extraction: PublicationSnapshotExtraction;
    raw_sha256: string;
    raw_byte_length: number;
  };
}
export interface PublicationSnapshotDetail extends PublicationSnapshotSummary {
  content: PublicationSnapshotSummary["content"] & { portable_markdown: string | null };
  comparison_policy: "comparison_only_capture_v2_bound" | "comparison_only_observed_at_commit_original_prepare_unbound";
  asset_policy: "reference_only_current_authorization_and_existence_required";
  comparisons: {
    properties_schema: { id: string; name: string; property_type: string; options_raw: string | null; sort_order: number }[];
    property_values: { property_id: string; value: string | null }[];
    attachments: { id: string; original_name: string; mime_type: string; size_bytes: number }[];
    drawings: { id: string; filename: string }[];
  };
  raw_download_url: string;
}
export interface PublicationSnapshotRaw {
  bytes: Uint8Array;
  /** Null means bytes cannot be represented as strict UTF-8; retain/download bytes. */
  text: string | null;
}
export interface PublicationHistoryTransport {
  capture(): OfflineReplayContext;
  verify(context: OfflineReplayContext): void;
  fetch: typeof fetch;
  online(): boolean;
}
const unproven = () => new Error("Published snapshot response is unproven. Reconnect and review current history access.");
const object = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const str = (v: unknown): v is string => typeof v === "string";
const id = (v: unknown): v is string => str(v) && !!v.trim() && v.length <= 512;
const nullableText = (v: unknown): v is string | null => v === null || str(v);
const digest = (v: unknown): v is string => str(v) && /^[a-f0-9]{64}$/.test(v);
const integer = (v: unknown): v is number => typeof v === "number" && Number.isSafeInteger(v);
const route = (space: string, document: string) => `/spaces/${encodeURIComponent(space)}/documents/${encodeURIComponent(document)}/publication-snapshots`;
const rawPath = (space: string, document: string, snapshot: string) => `${route(space, document)}/${encodeURIComponent(snapshot)}/raw`;
function validateSummary(value: unknown, document: string): PublicationSnapshotSummary {
  if (!object(value) || value.schema_version !== 1 || !id(value.id) || value.document_id !== document ||
      !id(value.captured_space_id) || !["publication", "baseline"].includes(String(value.kind)) ||
      !id(value.capture_operation_id) || !nullableText(value.publication_operation_id) ||
      (value.kind === "baseline" ? value.publication_operation_id !== null : value.publication_operation_id !== value.capture_operation_id) ||
      !str(value.published_at) || !str(value.captured_at) || !id(value.captured_by) ||
      !digest(value.snapshot_digest) || !["capture_v2", "legacy_content_v1"].includes(String(value.binding)) || !object(value.content)) throw unproven();
  const c = value.content;
  if (!str(c.title) || !nullableText(c.icon) || !nullableText(c.cover_reference) ||
      !["exact_published_draft", "proven_generated_header", "raw_only"].includes(String(c.extraction)) ||
      !digest(c.raw_sha256) || !integer(c.raw_byte_length) || c.raw_byte_length < 0 || c.raw_byte_length > 6_000_000) throw unproven();
  return {
    schema_version: 1, id: value.id, document_id: document, captured_space_id: value.captured_space_id,
    kind: value.kind as PublicationSnapshotKind, capture_operation_id: value.capture_operation_id,
    publication_operation_id: value.publication_operation_id, published_at: value.published_at,
    captured_at: value.captured_at, captured_by: value.captured_by, snapshot_digest: value.snapshot_digest,
    binding: value.binding as PublicationSnapshotBinding,
    content: { title: c.title, icon: c.icon, cover_reference: c.cover_reference,
      extraction: c.extraction as PublicationSnapshotExtraction, raw_sha256: c.raw_sha256, raw_byte_length: c.raw_byte_length },
  };
}
function mappedRows<T>(value: unknown, read: (row: Record<string, unknown>) => T): T[] {
  if (!Array.isArray(value)) throw unproven();
  const rows = value.map((row) => { if (!object(row)) throw unproven(); return read(row); });
  const identities = rows.map((row) => { const r = row as Record<string, unknown>; return r.id ?? r.property_id; });
  if (new Set(identities).size !== identities.length) throw unproven();
  return rows;
}
function validateDetail(value: unknown, space: string, document: string, snapshot: string): PublicationSnapshotDetail {
  const summary = validateSummary(value, document);
  if (!object(value) || !object(value.content) || !object(value.comparisons) || summary.id !== snapshot ||
      !nullableText(value.content.portable_markdown) ||
      (summary.content.extraction === "raw_only" ? value.content.portable_markdown !== null : !str(value.content.portable_markdown)) ||
      value.comparison_policy !== (summary.binding === "capture_v2" ? "comparison_only_capture_v2_bound" : "comparison_only_observed_at_commit_original_prepare_unbound") ||
      value.asset_policy !== "reference_only_current_authorization_and_existence_required" ||
      value.raw_download_url !== `/api/v1${rawPath(space, document, snapshot)}`) throw unproven();
  const rows = value.comparisons;
  return { ...summary, content: { ...summary.content, portable_markdown: value.content.portable_markdown },
    comparison_policy: value.comparison_policy as PublicationSnapshotDetail["comparison_policy"],
    asset_policy: value.asset_policy,
    raw_download_url: value.raw_download_url,
    comparisons: {
      properties_schema: mappedRows(rows.properties_schema, (r) => {
        if (!id(r.id) || !str(r.name) || !str(r.property_type) || !nullableText(r.options_raw) || !integer(r.sort_order)) throw unproven();
        return { id:r.id, name:r.name, property_type:r.property_type, options_raw:r.options_raw, sort_order:r.sort_order };
      }),
      property_values: mappedRows(rows.property_values, (r) => {
        if (!id(r.property_id) || !nullableText(r.value)) throw unproven();
        return { property_id:r.property_id, value:r.value };
      }),
      attachments: mappedRows(rows.attachments, (r) => {
        if (!id(r.id) || !str(r.original_name) || !str(r.mime_type) || !integer(r.size_bytes) || r.size_bytes < 0) throw unproven();
        return { id:r.id, original_name:r.original_name, mime_type:r.mime_type, size_bytes:r.size_bytes };
      }),
      drawings: mappedRows(rows.drawings, (r) => {
        if (!id(r.id) || !str(r.filename)) throw unproven(); return { id:r.id, filename:r.filename };
      }),
    },
  };
}
/** Current-authority reads only: no generic offline cache or mutation queue. */
export function createPublicationHistoryReader(transport: PublicationHistoryTransport) {
  async function request(path: string) {
    if (!path.startsWith("/spaces/")) throw unproven();
    if (!transport.online()) throw new Error("Reconnect to verify current Edit access to published snapshot history.");
    const context = transport.capture();
    if (context.origin.storageMode !== "server") throw new Error("Published snapshot history is unavailable in local vaults.");
    transport.verify(context);
    const response = await transport.fetch(`${context.origin.serverBase}${path}`, { method:"GET", headers:{Authorization:`Bearer ${context.bearer}`}, cache:"no-store" });
    transport.verify(context);
    if (response.status !== 200) {
      const raw = await response.text(); transport.verify(context);
      let message = `Published snapshot history returned ${response.status}`;
      try { const error: unknown = JSON.parse(raw); if (object(error) && str(error.message)) message = error.message; } catch { /* Status remains definitive. */ }
      throw new ApiError(message,response.status);
    }
    return { context, response };
  }
  return {
    async list(space: string, document: string) {
      if (!id(space) || !id(document)) throw unproven();
      const {context,response} = await request(route(space,document));
      const value: unknown = await response.json(); transport.verify(context);
      if (!object(value) || value.schema_version !== 1 || value.space_id !== space || value.document_id !== document || !Array.isArray(value.snapshots)) throw unproven();
      const records = value.snapshots.map((row) => validateSummary(row,document));
      if (new Set(records.map((row) => row.id)).size !== records.length) throw unproven();
      return records;
    },
    async get(space: string, document: string, snapshot: string, expected?: PublicationSnapshotSummary) {
      if (!id(space) || !id(document) || !id(snapshot)) throw unproven();
      const frozen = expected ? validateSummary(structuredClone(expected),document) : null;
      const {context,response} = await request(`${route(space,document)}/${encodeURIComponent(snapshot)}`);
      const value: unknown = await response.json(); transport.verify(context);
      if (!object(value) || value.schema_version !== 1 || value.space_id !== space || value.document_id !== document) throw unproven();
      const detail = validateDetail(value.snapshot,space,document,snapshot);
      if (frozen && JSON.stringify(validateSummary(detail,document)) !== JSON.stringify(frozen)) throw unproven();
      return detail;
    },
    async raw(space: string, document: string, snapshot: PublicationSnapshotSummary): Promise<PublicationSnapshotRaw> {
      if (!id(space) || !id(document)) throw unproven();
      // Freeze the exact proof selected before awaiting any response.
      const expected = validateSummary(structuredClone(snapshot),document);
      const {context,response} = await request(rawPath(space,document,expected.id));
      if (response.headers.get("content-type")?.split(";")[0].trim().toLowerCase() !== "application/octet-stream" ||
          response.headers.get("content-length") !== String(expected.content.raw_byte_length) ||
          response.headers.get("x-starlane-snapshot-id") !== expected.id ||
          response.headers.get("x-starlane-snapshot-sha256") !== expected.content.raw_sha256 ||
          !response.headers.get("cache-control")?.split(",").some((part) => part.trim().toLowerCase() === "no-store")) throw unproven();
      const bytes = new Uint8Array(await response.arrayBuffer()); transport.verify(context);
      if (bytes.length !== expected.content.raw_byte_length) throw unproven();
      const hash = await crypto.subtle.digest("SHA-256",bytes); transport.verify(context);
      if ([...new Uint8Array(hash)].map((byte) => byte.toString(16).padStart(2,"0")).join("") !== expected.content.raw_sha256) throw unproven();
      let text: string | null = null;
      try { text = new TextDecoder("utf-8",{fatal:true,ignoreBOM:true}).decode(bytes); } catch { /* Exact bytes remain downloadable. */ }
      return {bytes,text};
    },
  };
}
const live = () => createPublicationHistoryReader({
  capture:captureOfflineServerContext,
  verify:(context) => verifyOfflineReplayContext(context,getApiBase(),api.getToken()),
  fetch:globalThis.fetch.bind(globalThis), online:() => typeof navigator === "undefined" || navigator.onLine,
});
export const listDocumentPublicationSnapshots = (space:string,document:string) => live().list(space,document);
export const getDocumentPublicationSnapshot = (space:string,document:string,snapshot:string,expected?:PublicationSnapshotSummary) => live().get(space,document,snapshot,expected);
export const readDocumentPublicationSnapshotRaw = (space:string,document:string,snapshot:PublicationSnapshotSummary) => live().raw(space,document,snapshot);

// Only an opaque generation escapes this module; credentials never become view keys or DOM data.
let observedViewContext: string | null = null;
let viewContextGeneration = 0;
export function publicationHistoryContextVersion(): number {
  if (typeof window === "undefined") return 0;
  let fingerprint: string;
  try {
    fingerprint = JSON.stringify([getApiBase(), api.getToken(), localStorage.getItem("access_token"),
      localStorage.getItem("refresh_token"), localStorage.getItem("starlane_server_cache_v1:session-scope"),
      getCurrentLaunchMode(), getStorageSelection()]);
  } catch { fingerprint = "unavailable"; }
  if (fingerprint !== observedViewContext) { observedViewContext = fingerprint; viewContextGeneration++; }
  return viewContextGeneration;
}
/** Same-tab storage setters emit no storage event. Check before each visible paint as well. */
export function subscribePublicationHistoryContext(changed: () => void): () => void {
  let observed = publicationHistoryContextVersion(), frame = 0, stopped = false;
  const check = () => { const next = publicationHistoryContextVersion(); if (next !== observed) { observed = next; changed(); } };
  const tick = () => { if (stopped) return; check(); frame = requestAnimationFrame(tick); };
  frame = requestAnimationFrame(tick);
  window.addEventListener("storage", check); window.addEventListener("focus", check);
  document.addEventListener("visibilitychange", check);
  return () => { stopped = true; cancelAnimationFrame(frame); window.removeEventListener("storage",check);
    window.removeEventListener("focus",check); document.removeEventListener("visibilitychange",check); };
}
