import { api, ApiError, getApiBase } from "@/lib/api";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { capturePageMetadataLoadGuard } from "@/lib/pageMetadataLoad";

export type RestrictionMode = "inherited" | "restricted";
export interface RestrictionProposal {
  mode: RestrictionMode;
  allowlist: { users: string[]; groups: string[] };
}
export interface RestrictionInspection {
  schema_version: 1;
  space_id: string;
  document_id: string;
  checked_at: string;
  policy: {
    mode: RestrictionMode;
    revision: string | null;
    allowlist: {
      users: { id: string; available: boolean }[];
      groups: { id: string; available: boolean }[];
    };
  };
  eligible_recipients: {
    users: { id: string; display_name: string }[];
    groups: { id: string; name: string }[];
  };
  can_apply: boolean;
}
export interface RestrictionPreview {
  schema_version: 1;
  space_id: string;
  document_id: string;
  preview_token: string;
  expires_at: string;
  scope_notice: string;
  readable_pages: {
    document_id: string;
    public_link_configured: boolean;
    public_link_policy_before: boolean;
    public_link_policy_after: boolean;
  }[];
}
export interface RestrictionApplied {
  schema_version: 1;
  space_id: string;
  document_id: string;
  revision: string;
  mode: RestrictionMode;
  scope_notice: string;
}
export type RestrictionContextGuard = () => void;
function invalid(): never {
  throw new Error("Unable to verify this page's restrictions. Inspect again before reviewing.");
}
function object(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return invalid();
  const row = value as Record<string, unknown>;
  if (Object.keys(row).length !== keys.length || keys.some((key) => !Object.prototype.hasOwnProperty.call(row, key)))
    return invalid();
  return row;
}
function id(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}
function mode(value: unknown): value is RestrictionMode {
  return value === "inherited" || value === "restricted";
}
function timestamp(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:Z|[+-](\d{2}):(\d{2}))$/.exec(value);
  if (!match) return false;
  const [, y, m, d, h, minute, second, offsetHour, offsetMinute] = match;
  const year = Number(y), month = Number(m), day = Number(d);
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return month >= 1 && month <= 12 && day >= 1 && day <= days[month - 1] &&
    Number(h) <= 23 && Number(minute) <= 59 && Number(second) <= 59 &&
    (offsetHour === undefined || (Number(offsetHour) <= 23 && Number(offsetMinute) <= 59)) &&
    Number.isFinite(Date.parse(value));
}
function target(row: Record<string, unknown>, spaceId: string, pageId: string): void {
  if (!id(spaceId) || !id(pageId) || row.schema_version !== 1 ||
      row.space_id !== spaceId || row.document_id !== pageId) invalid();
}
function unique<T>(value: unknown, parse: (entry: unknown) => T, readId: (entry: T) => string): T[] {
  if (!Array.isArray(value)) return invalid();
  const result = value.map(parse);
  if (new Set(result.map(readId)).size !== result.length) return invalid();
  return result;
}
export function parseRestrictionInspection(value: unknown, spaceId: string, pageId: string): RestrictionInspection {
  const row = object(value, ["schema_version", "space_id", "document_id", "checked_at", "policy", "eligible_recipients", "can_apply"]);
  target(row, spaceId, pageId);
  if (!timestamp(row.checked_at) || typeof row.can_apply !== "boolean") return invalid();
  const policy = object(row.policy, ["mode", "revision", "allowlist"]);
  if (!mode(policy.mode) || (policy.revision !== null && !id(policy.revision)) ||
      (policy.mode === "restricted" && policy.revision === null)) return invalid();
  const allowlist = object(policy.allowlist, ["users", "groups"]);
  const catalog = object(row.eligible_recipients, ["users", "groups"]);
  for (const kind of ["users", "groups"] as const) {
    const label = kind === "users" ? "display_name" : "name";
    const eligible = unique(catalog[kind], (entry) => {
      const r = object(entry, ["id", label]);
      if (!id(r.id) || typeof r[label] !== "string") return invalid();
      return r;
    }, (r) => r.id as string);
    unique(allowlist[kind], (entry) => {
      const r = object(entry, ["id", "available"]);
      if (!id(r.id) || typeof r.available !== "boolean" ||
          r.available !== eligible.some((candidate) => candidate.id === r.id)) return invalid();
      return r;
    }, (r) => r.id as string);
  }
  if (policy.revision === null && ((allowlist.users as unknown[]).length || (allowlist.groups as unknown[]).length)) return invalid();
  return row as unknown as RestrictionInspection;
}
export function normalizeRestrictionProposal(value: RestrictionProposal): RestrictionProposal {
  const row = object(value, ["mode", "allowlist"]);
  const allowlist = object(row.allowlist, ["users", "groups"]);
  if (!mode(row.mode)) return invalid();
  const lists = (["users", "groups"] as const).map((kind) => unique(allowlist[kind], (entry) => {
    if (!id(entry) || entry.trim() !== entry || new TextEncoder().encode(entry).length > 256)
      return invalid();
    return entry;
  }, (entry) => entry).sort());
  if (lists[0].length + lists[1].length > 100 ||
      (row.mode === "inherited" && (lists[0].length || lists[1].length))) return invalid();
  return { mode: row.mode, allowlist: { users: lists[0], groups: lists[1] } };
}
export function parseRestrictionPreview(value: unknown, spaceId: string, pageId: string): RestrictionPreview {
  const row = object(value, ["schema_version", "space_id", "document_id", "preview_token", "expires_at", "scope_notice", "readable_pages"]);
  target(row, spaceId, pageId);
  if (!id(row.preview_token) || row.preview_token.length > 65536 || !timestamp(row.expires_at) ||
      typeof row.scope_notice !== "string") return invalid();
  unique(row.readable_pages, (entry) => {
    const r = object(entry, ["document_id", "public_link_configured", "public_link_policy_before", "public_link_policy_after"]);
    if (!id(r.document_id) || ["public_link_configured", "public_link_policy_before", "public_link_policy_after"].some((key) => typeof r[key] !== "boolean") ||
        (!r.public_link_configured && (r.public_link_policy_before || r.public_link_policy_after))) return invalid();
    return r;
  }, (r) => r.document_id as string);
  return row as unknown as RestrictionPreview;
}
export function parseRestrictionApplied(value: unknown, spaceId: string, pageId: string, expectedMode: RestrictionMode): RestrictionApplied {
  const row = object(value, ["schema_version", "space_id", "document_id", "revision", "mode", "scope_notice"]);
  target(row, spaceId, pageId);
  if (!id(row.revision) || !mode(row.mode) || row.mode !== expectedMode || typeof row.scope_notice !== "string") return invalid();
  return row as unknown as RestrictionApplied;
}
/** Only these three expiring-context requests bypass the shared client's automatic refresh/retry. */
async function restrictionPost<T>(spaceId: string, pageId: string, stage: "inspect" | "preview" | "apply", guard: RestrictionContextGuard, parse: (value: unknown) => T, body?: RestrictionProposal | { preview_token: string; confirm: true }): Promise<T> {
  const assertMetadata = capturePageMetadataLoadGuard();
  const assertCurrent = () => { assertMetadata(); guard(); };
  if (getCurrentLaunchMode() !== "server") throw new Error("Restriction management requires a connected server.");
  assertCurrent();
  if (!id(spaceId) || !id(pageId)) return invalid();
  const token = api.getToken();
  const base = getApiBase();
  if (!token) throw new ApiError("Sign in again and reopen Share page before reviewing restrictions.", 401, "session_expired");
  const headers: Record<string, string> = { Authorization: `Bearer ${token}` };
  if (body !== undefined) headers["Content-Type"] = "application/json";
  assertCurrent();
  let response: Response;
  try {
    response = await fetch(`${base}/spaces/${encodeURIComponent(spaceId)}/documents/${encodeURIComponent(pageId)}/restrictions/${stage}`, {
      method: "POST", headers, cache: "no-store", redirect: "error",
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
  } catch (reason) { assertCurrent(); throw reason; }
  assertCurrent();
  if (response.ok && response.status !== 200) {
    throw new ApiError("Unexpected restriction response. Inspect current restrictions before reviewing again.", response.status, "unexpected_status");
  }
  let value: unknown;
  try { value = await response.json(); }
  catch {
    assertCurrent();
    throw new ApiError(stage === "inspect" && response.status === 404 ? "Restriction management is unavailable for this page." : response.ok ? "Unable to verify the restriction response. Inspect again before reviewing." : `Restriction request failed (HTTP ${response.status}).`, response.status);
  }
  assertCurrent();
  if (!response.ok) {
    const error = value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
    const message = stage === "inspect" && response.status === 404
      ? "Restriction management is unavailable for this page."
      : typeof error.message === "string" ? error.message : `Restriction request failed (HTTP ${response.status}).`;
    throw new ApiError(message, response.status, typeof error.error === "string" ? error.error : undefined);
  }
  return parse(value);
}
export function inspectPageRestrictions(spaceId: string, pageId: string, guard: RestrictionContextGuard): Promise<RestrictionInspection> {
  return restrictionPost(spaceId, pageId, "inspect", guard, (value) => parseRestrictionInspection(value, spaceId, pageId));
}
export function previewPageRestrictions(spaceId: string, pageId: string, proposal: RestrictionProposal, guard: RestrictionContextGuard): Promise<RestrictionPreview> {
  const normalized = normalizeRestrictionProposal(proposal);
  return restrictionPost(spaceId, pageId, "preview", guard, (value) => parseRestrictionPreview(value, spaceId, pageId), normalized);
}
export function applyPageRestrictions(spaceId: string, pageId: string, preview: RestrictionPreview, expectedMode: RestrictionMode, guard: RestrictionContextGuard): Promise<RestrictionApplied> {
  const checked = parseRestrictionPreview(preview, spaceId, pageId);
  if (Date.parse(checked.expires_at) <= Date.now()) throw new Error("Restriction review expired. Inspect and review again before confirming.");
  return restrictionPost(spaceId, pageId, "apply", guard, (value) => parseRestrictionApplied(value, spaceId, pageId, expectedMode), { preview_token: checked.preview_token, confirm: true });
}
