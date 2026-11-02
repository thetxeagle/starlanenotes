import { api, ApiError, getApiBase } from "@/lib/api";
import { capturePageMetadataLoadGuard } from "@/lib/pageMetadataLoad";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import type { PageViewerProposal, PageViewerInspection, PageViewerReview, PageViewerConfirmed, SharedPagesInspection } from "@starlane-notes/shared-types";
export type PageViewerContext = () => void;
function invalid(): never { throw new Error("Unable to verify page sharing. Keep your request and inspect current access."); }
function row(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
}
function string(value: unknown): value is string { return typeof value === "string" && value.length > 0; }
function digest(value: unknown): value is string { return typeof value === "string" && /^[0-9a-f]{64}$/.test(value); }
function nullable(value: unknown) { return value === null || typeof value === "string"; }
function target(value: unknown, space: string, page: string) {
  const r = row(value);
  if (r.schema_version !== 1 || r.space_id !== space || r.document_id !== page) return invalid();
  return r;
}
export function parsePageViewerInspection(value: unknown, space: string, page: string): PageViewerInspection {
  const r = target(value, space, page);
  if (typeof r.scope_notice !== "string" || !Array.isArray(r.grants)) return invalid();
  const seen = new Set<string>();
  for (const entry of r.grants) {
    const g = row(entry);
    if (!string(g.user_id) || seen.has(g.user_id) || !string(g.revision) || g.role !== "viewer" || g.scope !== "page" || !nullable(g.display_name) || typeof g.available !== "boolean" || (!g.available && g.display_name !== null)) return invalid();
    seen.add(g.user_id);
  }
  return r as unknown as PageViewerInspection;
}
export function parsePageViewerReview(value: unknown, space: string, page: string, proposal: PageViewerProposal): PageViewerReview {
  const r = target(value, space, page), p = row(r.proposal), recipient = row(r.recipient);
  if (!digest(r.source_digest) || p.user_id !== proposal.user_id || p.action !== proposal.action || recipient.id !== proposal.user_id || !nullable(recipient.display_name) || !nullable(recipient.email)) return invalid();
  if (r.existing_grant !== null) validateGrant(r.existing_grant, space, page, proposal.user_id);
  if (proposal.action === "revoke" && r.existing_grant === null) return invalid();
  return r as unknown as PageViewerReview;
}
function validateGrant(value: unknown, space: string, page: string, user: string) {
  const g = row(value);
  if (!string(g.id) || g.document_id !== page || g.space_id !== space || g.user_id !== user || g.role !== "viewer" || g.scope !== "page" || !string(g.revision) || !string(g.created_by) || !string(g.created_at) || !string(g.updated_at)) return invalid();
}
export function parsePageViewerConfirmed(value: unknown, review: PageViewerReview): PageViewerConfirmed {
  const r = target(value, review.space_id, review.document_id);
  if (r.confirmed !== true || r.user_id !== review.proposal.user_id || r.action !== review.proposal.action || r.source_digest !== review.source_digest) return invalid();
  if (review.proposal.action === "grant") validateGrant(r.grant, review.space_id, review.document_id, review.proposal.user_id);
  else if (r.grant !== null) return invalid();
  return r as unknown as PageViewerConfirmed;
}
/** Captures original credentials once; no refresh, offline queue, retry or cache mutation. */
export function capturePageViewerTransport(guard: PageViewerContext) {
  const metadata = capturePageMetadataLoadGuard(), token = api.getToken(), base = getApiBase();
  const verify = () => { metadata(); guard(); if (getCurrentLaunchMode() !== "server" || api.getToken() !== token || getApiBase() !== base) throw new Error("Sharing context changed. Reopen this page."); };
  verify(); if (!token) throw new ApiError("Sign in to inspect shared pages.", 401);
  return async <T>(path: string, method: "GET" | "POST", body: unknown, parse: (value: unknown) => T): Promise<T> => {
    verify();
    const copiedBody = body === undefined ? undefined : JSON.stringify(body);
    const response = await fetch(`${base}${path}`, { method, cache: "no-store", redirect: "error", headers: { Authorization: `Bearer ${token}`, ...(copiedBody === undefined ? {} : { "Content-Type": "application/json" }) }, ...(copiedBody === undefined ? {} : { body: copiedBody }) });
    verify(); const value: unknown = await response.json(); verify();
    if (response.status !== 200) throw new ApiError("Page sharing request was not confirmed. Keep the original request and inspect access.", response.status);
    const admitted = parse(value); verify(); return admitted;
  };
}
export function pageViewerPath(space: string, page: string, stage: "inspect" | "review" | "confirm") {
  return `/spaces/${encodeURIComponent(space)}/documents/${encodeURIComponent(page)}/viewer-grants/${stage}`;
}
export function parseSharedPages(value: unknown): SharedPagesInspection {
  const r = row(value); if (!Array.isArray(r.pages) || !nullable(r.next_cursor)) return invalid();
  const seen = new Set<string>();
  for (const entry of r.pages) {
    const p = row(entry), key = `${p.space_id}/${p.id}`;
    if (!string(p.id) || !string(p.space_id) || typeof p.title !== "string" || !string(p.published_at) || !string(p.updated_at) || !nullable(p.human_identifier) || !nullable(p.icon) || seen.has(key)) return invalid();
    seen.add(key);
  }
  return r as unknown as SharedPagesInspection;
}
export function parsePageViewerPrincipals(value: unknown): { id: string; display_name: string; email: string }[] {
  const r = row(value); if (!Array.isArray(r.users)) return invalid();
  const seen = new Set<string>();
  return r.users.map(entry => { const u = row(entry); if (!string(u.id) || seen.has(u.id) || typeof u.display_name !== "string" || typeof u.email !== "string") return invalid(); seen.add(u.id); return { id: u.id, display_name: u.display_name, email: u.email }; });
}

/** Unconfirmed requests survive separate reviews in memory; no durable replay journal. */
export function createPageViewerUncertainty() {
  const retained: PageViewerReview[] = [];
  let blocked = false, freshlyInspected = false;
  return {
    dispatched(review: PageViewerReview) {
      if (blocked) throw new Error("Inspect before starting a separate review; the original request remains unconfirmed.");
      retained.push(structuredClone(review)); blocked = true; freshlyInspected = false;
    },
    confirmed(review: PageViewerReview) {
      const last = retained[retained.length - 1];
      if (!last || last.source_digest !== review.source_digest || last.proposal.action !== review.proposal.action || last.proposal.user_id !== review.proposal.user_id || last.document_id !== review.document_id || last.space_id !== review.space_id) return invalid();
      retained.pop(); blocked = false; freshlyInspected = false;
    },
    inspected() { freshlyInspected = true; },
    separateReview() {
      if (!blocked || !freshlyInspected) throw new Error("Inspect current access before starting a separate review.");
      blocked = false; freshlyInspected = false;
    },
    blocked: () => blocked,
    retained: () => structuredClone(retained),
  };
}

/** Catalog denial must not suppress a separately authorized exact revoke list. */
export async function loadPageViewerManagerProjection(
  inspect: () => Promise<PageViewerInspection>,
  principals: () => Promise<{id: string; display_name: string; email: string}[]>,
  verifyCurrent: () => void,
) {
  const inspection = await inspect(); verifyCurrent();
  try {
    const users = await principals(); verifyCurrent();
    return {inspection,users,catalog_available:true};
  } catch (reason) {
    verifyCurrent();
    if (reason instanceof ApiError && (reason.status === 403 || reason.status === 404))
      return {inspection,users:[],catalog_available:false};
    throw reason;
  }
}
