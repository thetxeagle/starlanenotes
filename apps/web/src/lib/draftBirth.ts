import { api, ApiError, captureOfflineServerContext, getApiBase } from "./api";
import { isOfflineReplayOrigin, sameOfflineReplayOrigin, verifyOfflineReplayContext, type OfflineReplayOrigin } from "./offlineReplayOrigin";
import type { DraftRecord } from "./dataClient";

export interface DraftBirthOptions { force_new?: boolean; source_draft_id?: string; source_digest?: string }
interface BirthIntent {
  version: 1; operation_id: string; space_id: string; document_id: string;
  origin: OfflineReplayOrigin; body: DraftBirthOptions & { operation_id: string };
  state: "prepared" | "dispatched" | "unknown" | "rejected" | "confirmed";
  message?: string;
}
const PREFIX = "starlane_draft_birth_v1:";
const uuid = (v: unknown): v is string => typeof v === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(v);
const digest = (v: unknown): v is string => typeof v === "string" && /^[0-9a-f]{64}$/.test(v);
const nonempty = (v: unknown): v is string => typeof v === "string" && v.trim().length > 0;
export class DraftBirthHeldError extends Error {
  constructor(public operationId: string, message: string) { super(message); this.name = "DraftBirthHeldError"; }
}
function records(): { key: string; intent: BirthIntent }[] {
  const result: { key: string; intent: BirthIntent }[] = [];
  for (let i = 0; i < localStorage.length; i += 1) {
    const key = localStorage.key(i); if (!key?.startsWith(PREFIX)) continue;
    let value: BirthIntent; try { value = JSON.parse(localStorage.getItem(key)!); } catch { throw new Error("Stored draft request is corrupt; retain it for review"); }
    if (!value || value.version !== 1 || !uuid(value.operation_id) || !nonempty(value.space_id) || !nonempty(value.document_id) || !isOfflineReplayOrigin(value.origin) || !value.body || value.body.operation_id !== value.operation_id || !["prepared", "dispatched", "unknown", "rejected", "confirmed"].includes(value.state)) throw new Error("Stored draft request is unavailable; retain it for review");
    validateOptions(value.body);
    result.push({ key, intent: value });
  }
  return result;
}
function validateOptions(options: DraftBirthOptions): void {
  if (options.force_new !== undefined && typeof options.force_new !== "boolean") throw new Error("Invalid draft birth intent");
  const hasId = options.source_draft_id !== undefined, hasDigest = options.source_digest !== undefined;
  if (hasId !== hasDigest || hasId && (!nonempty(options.source_draft_id) || !digest(options.source_digest))) throw new Error("Review the selected draft before forking its original source");
}
function persist(key: string, intent: BirthIntent, expected?: string): string {
  if (expected !== undefined && localStorage.getItem(key) !== expected) throw new Error("Draft request changed during dispatch; keep both intents for review");
  const raw = JSON.stringify(intent);
  try { localStorage.setItem(key, raw); if (localStorage.getItem(key) !== raw) throw new Error("readback differs"); }
  catch { throw new Error("Could not retain the draft request on this device; no confirmed creation can be reported"); }
  return raw;
}
export function selectedDraftFork(source: DraftRecord | null): Pick<DraftBirthOptions, "source_draft_id" | "source_digest"> {
  if (!source) return {};
  if (source.access_role === "viewer" || !digest(source.source_digest)) throw new Error("Select an editable draft before creating a private fork");
  return { source_draft_id: source.id, source_digest: source.source_digest };
}
export function validatesBirthResult(value: unknown, status: number, intent: { operation_id: string; space_id: string; document_id: string; account_id: string; force_new?: boolean }): DraftRecord {
  const v = value as Partial<DraftRecord> | null;
  if ((status !== 200 && status !== 201) || (intent.force_new === true && status !== 201) || !v || !nonempty(v.id) || v.space_id !== intent.space_id || v.document_id !== intent.document_id || v.owner_id !== intent.account_id || v.access_role !== "owner" || v.can_publish !== true || typeof v.owner_name !== "string" || typeof v.document_title !== "string" || typeof v.title !== "string" || typeof v.content !== "string" || !digest(v.source_digest) || !nonempty(v.created_at) || !nonempty(v.updated_at) || !nonempty(v.base_updated_at) || (status === 201 && v.birth_operation_id !== intent.operation_id) || (v.birth_operation_id !== undefined && v.birth_operation_id !== intent.operation_id)) throw new Error("Draft birth response is unproven; review the original request before trying again");
  return JSON.parse(JSON.stringify(v)) as DraftRecord;
}
export interface RetainedDraftBirthInspection {
  operationId: string;
  state: BirthIntent["state"];
  options: DraftBirthOptions;
}
/** Read only the exact retained request under its original current session. */
export function inspectServerDraftBirth(spaceId: string, documentId: string, operationId: string): RetainedDraftBirthInspection | null {
  const context = captureOfflineServerContext();
  const found = records().filter(({ intent }) => intent.operation_id === operationId && intent.space_id === spaceId && intent.document_id === documentId && sameOfflineReplayOrigin(intent.origin, context.origin));
  if (found.length !== 1) return null;
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  const { operation_id: _operationId, ...options } = found[0].intent.body;
  void _operationId;
  return JSON.parse(JSON.stringify({ operationId, state: found[0].intent.state, options }));
}
export async function createServerDraftBirth(spaceId: string, documentId: string, options: DraftBirthOptions = {}, retryOperationId?: string, onPreparedOperationId?: (operationId: string) => void): Promise<DraftRecord> {
  validateOptions(options);
  const context = captureOfflineServerContext();
  const originals = records().filter(({ intent }) => intent.origin.serverBase === context.origin.serverBase && intent.origin.accountId === context.origin.accountId && intent.space_id === spaceId && intent.document_id === documentId  );
  const pending = originals.filter(({ intent }) => ["prepared", "dispatched", "unknown"].includes(intent.state));
  let key: string, intent: BirthIntent, expected: string | undefined;
  if (retryOperationId) {
    const found = originals.filter((record) => record.intent.operation_id === retryOperationId && ["prepared", "dispatched", "unknown", "confirmed"].includes(record.intent.state));
    if (found.length !== 1 || !sameOfflineReplayOrigin(found[0].intent.origin, context.origin)) throw new DraftBirthHeldError(retryOperationId, "The original request cannot be retried under this session; retain it for review");
    ({ key, intent } = found[0]); intent = JSON.parse(JSON.stringify(intent)); expected = localStorage.getItem(key)!;
  } else {
    if (pending.length) throw new DraftBirthHeldError(pending[0].intent.operation_id, "A previous draft request needs review. Retry that original request explicitly; no second draft was dispatched");
    const operationId = crypto.randomUUID();
    intent = { version: 1, operation_id: operationId, space_id: spaceId, document_id: documentId, origin: { ...context.origin }, body: JSON.parse(JSON.stringify({ ...options, operation_id: operationId })), state: "prepared" };
    key = `${PREFIX}${operationId}`;
  }
  const previouslyDispatched = intent.state === "dispatched" || intent.state === "unknown" || intent.state === "confirmed";
  expected = persist(key, intent, expected);
  // Hand the caller the durably prepared identity before any network dispatch.
  onPreparedOperationId?.(intent.operation_id);
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  if (typeof navigator !== "undefined" && navigator.onLine === false) throw new DraftBirthHeldError(intent.operation_id, "Draft request is retained without dispatch. Retry the original request when connected");
  intent = { ...intent, state: "dispatched" }; expected = persist(key, intent, expected);
  try {
    const draft = await api.fetchDraftBirth(context, `/spaces/${encodeURIComponent(spaceId)}/documents/${encodeURIComponent(documentId)}/drafts`, intent.body, (value, status) => validatesBirthResult(value, status, { ...intent, force_new: intent.body.force_new, account_id: context.origin.accountId }));
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    persist(key, { ...intent, state: "confirmed" }, expected);
    return draft;
  } catch (error) {
    const knownRejection = !previouslyDispatched && error instanceof ApiError && (
      [401, 403, 404].includes(error.status) ||
      [400, 409, 422].includes(error.status) && /^(source_required|source_conflict):/.test(error.message)
    );
    const message = error instanceof Error ? error.message : "Draft request could not be confirmed";
    const retained: BirthIntent = { ...intent, state: knownRejection ? "rejected" : "unknown", message };
    try { persist(key, retained, expected); }
    catch { persist(`${PREFIX}${crypto.randomUUID()}`, { ...retained, state: "unknown" }); throw new DraftBirthHeldError(intent.operation_id, "Draft request changed while its outcome was pending; original and newer evidence are retained for explicit review"); }
    if (knownRejection) throw error;
    throw new DraftBirthHeldError(intent.operation_id, "Draft creation outcome is unknown. Your original request is retained; retry it explicitly before creating another draft");
  }
}
