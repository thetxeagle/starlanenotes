import { api, getApiBase, captureOfflineServerContext, ApiError } from "./api";
import { verifyOfflineReplayContext } from "./offlineReplayOrigin";
import { capturePageMetadataLoadGuard } from "./pageMetadataLoad";
import type {
  ServerMoveIntent,
  ServerMoveViewerGrantChoice,
  ServerMoveViewerGrantDecision,
  ServerMovePreview,
  ServerMoveCommit,
  ServerMoveCommitResult,
  ServerMovePreservedEvidence,
  ServerMovePreservedIndex,
} from "@starlane-notes/shared-types";
const object = (v: unknown): v is Record<string, unknown> =>
  !!v && typeof v === "object" && !Array.isArray(v);
export function parseServerMovePreview(value: unknown): ServerMovePreview {
  if (
    !object(value) ||
    typeof value.preview_id !== "string" ||
    !value.preview_id ||
    typeof value.fingerprint !== "string" ||
    typeof value.expires_at !== "string" ||
    !Number.isFinite(Date.parse(value.expires_at)) ||
    typeof value.stale !== "boolean" ||
    typeof value.commit_available !== "boolean" ||
    typeof value.commit_unavailable_reason !== "string" ||
    !Array.isArray(value.blockers) ||
    !value.blockers.every((x) => typeof x === "string") ||
    !object(value.inventory) ||
    !object(value.access_consequences)
  )
    throw new Error("Move review response is unavailable. Review again.");
  const access = value.access_consequences;
  if (
    !["space_audience_gained", "space_audience_lost"].every(
      (k) =>
        typeof access[k] === "number" &&
        Number.isSafeInteger(access[k]) &&
        (access[k] as number) >= 0,
    ) ||
    !["public_links", "private_drafts", "permission_model"].every(
      (k) => typeof access[k] === "string",
    )
  )
    throw new Error("Move audience needs a new review.");
  const inv = value.inventory;
  if (
    Array.isArray(inv.pages) &&
    inv.pages.some(
      (page) =>
        !object(page) ||
        (page.human_identifier !== undefined &&
          page.human_identifier !== null &&
          typeof page.human_identifier !== "string") ||
        ["maintenance_user_id", "maintenance_group_id"].some(
          (field) =>
            page[field] !== undefined && page[field] !== null && typeof page[field] !== "string",
        ) ||
        (page.maintenance_user_id != null && page.maintenance_group_id != null) ||
        [
          "identifier_resolution_required",
          "identifier_edit_allowed",
          "maintenance_resolution_required",
          "maintenance_edit_allowed",
        ].some((field) => page[field] !== undefined && typeof page[field] !== "boolean"),
    )
  )
    throw new Error("Move identifier review is unavailable. Review again.");
  if (inv.maintenance_targets !== undefined) {
    const targets = inv.maintenance_targets;
    if (
      !object(targets) ||
      typeof targets.groups_available !== "boolean" ||
      !Array.isArray(targets.users) ||
      !targets.users.every(
        (u) =>
          object(u) &&
          typeof u.id === "string" &&
          !!u.id &&
          typeof u.name === "string" &&
          u.active === true,
      ) ||
      !Array.isArray(targets.groups) ||
      !targets.groups.every(
        (g) =>
          object(g) &&
          typeof g.id === "string" &&
          !!g.id &&
          typeof g.name === "string" &&
          (g.managed_space_id === null || typeof g.managed_space_id === "string"),
      )
    )
      throw new Error("Move maintenance targets need a fresh review.");
  }
  if (
    inv.metadata !== undefined &&
    (!Array.isArray(inv.metadata) ||
      !inv.metadata.every(
        (m) =>
          object(m) &&
          ["tag", "property", "workflow"].includes(String(m.kind)) &&
          object(m.source_definition) &&
          typeof m.source_definition.id === "string" &&
          typeof m.source_definition.name === "string" &&
          typeof m.create_allowed === "boolean" &&
          typeof m.assignment_count === "number" &&
          Number.isSafeInteger(m.assignment_count) &&
          m.assignment_count >= 0 &&
          Array.isArray(m.assignments) &&
          m.assignments.every(
            (a) =>
              object(a) &&
              typeof a.id === "string" &&
              typeof a.document_id === "string" &&
              (a.value === null || typeof a.value === "string"),
          ) &&
          Array.isArray(m.resolution_candidates) &&
          m.resolution_candidates.every(
            (c) =>
              object(c) &&
              typeof c.id === "string" &&
              typeof c.name === "string" &&
              c.compatibility === "requires_explicit_relation_targets" &&
              object(c.choice_details),
          ) &&
          Array.isArray(m.candidates) &&
          m.candidates.every(
            (c) =>
              object(c) &&
              typeof c.id === "string" &&
              typeof c.name === "string" &&
              object(c.choice_details),
          ),
      ))
  )
    throw new Error("Move metadata needs a new review.");
  if (
    inv.relations !== undefined &&
    (!Array.isArray(inv.relations) ||
      !inv.relations.every(
        (e) =>
          object(e) &&
          ["assignment_id", "property_id", "from_document_id", "to_document_id"].every(
            (k) => typeof e[k] === "string",
          ) &&
          typeof e.crosses_subtree === "boolean",
      ))
  )
    throw new Error("Move relationships need a new review.");
  if (
    inv.pages !== undefined &&
    (!Array.isArray(inv.pages) ||
      !inv.pages.every((p) => object(p) && typeof p.id === "string" && typeof p.title === "string"))
  )
    throw new Error("Move pages need a new review.");
  if (inv.folders !== undefined && (!Array.isArray(inv.folders) || !inv.folders.every(object)))
    throw new Error("Move folders need a new review.");
  if (
    ["attachments", "drawings", "history_versions", "workflow_history_events"].some(
      (k) =>
        inv[k] !== undefined &&
        (typeof inv[k] !== "number" || !Number.isSafeInteger(inv[k]) || (inv[k] as number) < 0),
    )
  )
    throw new Error("Move inventory needs a new review.");
  if (
    inv.relation_targets !== undefined &&
    (!Array.isArray(inv.relation_targets) ||
      !inv.relation_targets.every(
        (t) => object(t) && typeof t.id === "string" && typeof t.title === "string",
      ) ||
      typeof inv.relation_targets_complete !== "boolean" ||
      (inv.relation_targets_unavailable_reason !== null &&
        typeof inv.relation_targets_unavailable_reason !== "string"))
  )
    throw new Error("Move relation targets need a new review.");
  if (inv.viewer_grants !== undefined) {
    if (!Array.isArray(inv.viewer_grants)) throw new Error("Move viewer grants need a fresh review.");
    const ids = new Set<string>();
    for (const grant of inv.viewer_grants) {
      if (!object(grant) || !exactKeys(grant, ["grant_id", "document_id", "revision", "role", "scope", "recipient", "preserve_allowed", "remove_allowed"]) ||
        !["grant_id", "document_id", "revision"].every(k => typeof grant[k] === "string" && !!grant[k]) ||
        ids.has(String(grant.grant_id)) || grant.role !== "viewer" || grant.scope !== "page" ||
        !object(grant.recipient) || !exactKeys(grant.recipient, ["available", "display_name"]) || typeof grant.recipient.available !== "boolean" ||
        !(grant.recipient.display_name === null || typeof grant.recipient.display_name === "string") ||
        (!grant.recipient.available && grant.recipient.display_name !== null) ||
        typeof grant.preserve_allowed !== "boolean" || typeof grant.remove_allowed !== "boolean" ||
        !Array.isArray(inv.pages) || !inv.pages.some(p => object(p) && p.id === grant.document_id))
        throw new Error("Move viewer grants need a fresh review.");
      ids.add(String(grant.grant_id));
    }
  }
  return value as unknown as ServerMovePreview;
}
/** Resolve only the exact accepted preview. Hints do not replace server authority. */
export function resolveServerMoveViewerGrants(
  preview: ServerMovePreview,
  choices: Record<string, ServerMoveViewerGrantChoice> = {},
): ServerMoveViewerGrantDecision[] {
  const grants = preview.inventory.viewer_grants ?? [];
  if (Object.keys(choices).some(id => !grants.some(grant => grant.grant_id === id)))
    throw new Error("Viewer grant choice refers to an unavailable grant. Review again.");
  const ids = new Set<string>();
  return grants.map((grant): ServerMoveViewerGrantDecision => {
    if (ids.has(grant.grant_id)) throw new Error("Duplicate viewer grant in move review.");
    ids.add(grant.grant_id);
    const choice = choices[grant.grant_id];
    if (!choice) throw new Error("Choose preserve or remove for every page viewer grant.");
    if (choice.kind === "preserve") {
      if (!grant.preserve_allowed) throw new Error("This viewer grant cannot be preserved in the destination.");
    } else if (choice.kind === "remove" && choice.confirm === true) {
      if (!grant.remove_allowed) throw new Error("You cannot remove this viewer grant.");
    } else throw new Error("Viewer grant removal needs an explicit confirmed choice.");
    return {grant_id: grant.grant_id, revision: grant.revision, choice: choice.kind === "preserve" ? {kind: "preserve"} : {kind: "remove", confirm: true}};
  });
}
/** In-memory original authority; never persist the bearer or re-enroll on retry. */
export interface ReviewedMoveContext { verify: () => void; request: (path: string, method: "POST" | "DELETE", body?: string) => Promise<unknown>; }
export function captureReviewedMoveContext(): ReviewedMoveContext {
  const original = captureOfflineServerContext();
  const selection = capturePageMetadataLoadGuard();
  const verify = () => { selection(); verifyOfflineReplayContext(original, getApiBase(), api.getToken()); };
  return { verify, request: async (path, method, body) => {
    verify();
    if (!(method === "DELETE" ? /^\/moves\/previews\/[^/?#]+$/.test(path) : /^\/spaces\/[^/?#]+\/moves\/preview$/.test(path) || /^\/moves\/previews\/[^/?#]+\/commit$/.test(path))) throw new Error("Invalid reviewed move route");
    if (typeof navigator !== "undefined" && navigator.onLine === false) throw new Error("Move request was not dispatched. Keep the original review until connected.");
    let response: Response; let text: string;
    try {
      response = await fetch(`${original.origin.serverBase}${path}`, { method, headers: { Authorization: `Bearer ${original.bearer}`, ...(body === undefined ? {} : { "Content-Type": "application/json" }) }, body });
      verify(); text = await response.text(); verify();
    } catch { throw new Error("Move outcome is unconfirmed. Retain the original request for review; do not start another move."); }
    if (!response.ok) {
      let value: unknown; try { value = JSON.parse(text); } catch { value = null; }
      const message = object(value) && typeof value.message === "string" ? value.message : object(value) && typeof value.error === "string" ? value.error : `Move request failed (HTTP ${response.status})`;
      throw new ApiError(message, response.status);
    }
    if (response.status !== (method === "DELETE" ? 204 : 200) || method === "DELETE" && text.trim()) throw new Error("Move outcome is unconfirmed. Response did not acknowledge the original request.");
    if (method === "DELETE") return undefined;
    let value: unknown; try { value = JSON.parse(text); } catch { throw new Error("Move outcome is unconfirmed. Retain the original request."); }
    verify(); return value;
  } };
}
export interface PendingReviewedMove { readonly id: string; readonly requestJson: string; readonly context: ReviewedMoveContext; }
export function prepareReviewedMove(context: ReviewedMoveContext, id: string, request: ServerMoveCommit): PendingReviewedMove {
  context.verify();
  if (!id) throw new Error("Original move preview is unavailable");
  return Object.freeze({ id, requestJson: JSON.stringify(request), context });
}
export async function dispatchReviewedMove(pending: PendingReviewedMove): Promise<ServerMoveCommitResult> {
  const value = await pending.context.request(`/moves/previews/${encodeURIComponent(pending.id)}/commit`, "POST", pending.requestJson);
  if (!object(value) || value.committed !== true || typeof value.operation_id !== "string" || !value.operation_id) throw new Error("Move outcome is unconfirmed. Retain the original request.");
  pending.context.verify(); return value as unknown as ServerMoveCommitResult;
}
/** Callback fences are consumed by Sidebar before each wait and state/navigation. */
export async function completeReviewedMove(context: ReviewedMoveContext, current: () => boolean, refresh: () => Promise<void>, finish: () => void): Promise<void> {
  const verify = () => { context.verify(); if (!current()) throw new Error("Original move selection changed. The confirmed move remains committed; reload its destination."); };
  verify(); await refresh(); verify(); finish();
}
/** Only live POST/DELETE transport: no cached GET, offline queue or local authority fallback. */
export const reviewedServerMoves = {
  async preservedIndex(space: string, page: string, cursor?: string) {
    return parseServerMovePreservedIndex(
      await api.fetch<unknown>(
        `/moves/documents/${encodeURIComponent(page)}/preserved/inspect${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ""}`,
        { method: "POST" },
      ),
      space,
      page,
    );
  },
  async preservedEvidence(space: string, page: string, operation: string) {
    return parseServerMovePreservedEvidence(
      await api.fetch<unknown>(
        `/moves/commits/${encodeURIComponent(operation)}/preserved/${encodeURIComponent(page)}/inspect`,
        { method: "POST" },
      ),
      space,
      page,
      operation,
    );
  },
  async preview(source: string, intent: ServerMoveIntent, context = captureReviewedMoveContext()) {
    const request = JSON.stringify(intent);
    return parseServerMovePreview(await context.request(`/spaces/${encodeURIComponent(source)}/moves/preview`, "POST", request));
  },
  async cancel(id: string, context = captureReviewedMoveContext()) {
    await context.request(`/moves/previews/${encodeURIComponent(id)}`, "DELETE");
  },
  async commit(id: string, request: ServerMoveCommit, context = captureReviewedMoveContext()) {
    return dispatchReviewedMove(prepareReviewedMove(context, id, request));
  },
};

const timestamp = (v: unknown): v is string =>
  typeof v === "string" &&
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(v) &&
  Number.isFinite(Date.parse(v));
const nonempty = (v: unknown): v is string => typeof v === "string" && v.length > 0;
const jsonValue = (v: unknown, seen = new Set<object>()): boolean => {
  if (v === null || typeof v === "string" || typeof v === "boolean") return true;
  if (typeof v === "number") return Number.isFinite(v);
  if (typeof v !== "object" || seen.has(v)) return false;
  seen.add(v);
  const valid = (Array.isArray(v) ? v : Object.values(v)).every((item) => jsonValue(item, seen));
  seen.delete(v);
  return valid;
};
const exactKeys = (v: Record<string, unknown>, keys: string[]) =>
  Object.keys(v).length === keys.length && Object.keys(v).every((key) => keys.includes(key));
const unavailable = () => new Error("Preserved metadata is unavailable. Inspect again.");
export function parseServerMovePreservedIndex(
  value: unknown,
  space: string,
  page: string,
): ServerMovePreservedIndex {
  if (
    !object(value) ||
    !exactKeys(value, [
      "schema_version",
      "space_id",
      "document_id",
      "checked_at",
      "operations",
      "next_cursor",
    ]) ||
    value.schema_version !== 1 ||
    value.space_id !== space ||
    value.document_id !== page ||
    !timestamp(value.checked_at) ||
    !Array.isArray(value.operations) ||
    value.operations.length > 20 ||
    !(value.next_cursor === null || nonempty(value.next_cursor))
  )
    throw unavailable();
  const ids = new Set<string>();
  for (const row of value.operations) {
    if (
      !object(row) ||
      !exactKeys(row, ["operation_id", "created_at", "kind"]) ||
      !nonempty(row.operation_id) ||
      ids.has(row.operation_id) ||
      !timestamp(row.created_at) ||
      row.kind !== "immutable_metadata_evidence"
    )
      throw unavailable();
    ids.add(row.operation_id);
  }
  if (
    value.next_cursor !== null &&
    (value.operations.length !== 20 ||
      value.next_cursor !== value.operations[value.operations.length - 1].operation_id)
  )
    throw unavailable();
  return value as unknown as ServerMovePreservedIndex;
}
/** Only server-authorized projected grant evidence; never sealed raw recipient rows. */
export function parseServerMoveViewerGrantEvidence(value: unknown, page: string, grant: string) {
  if (!object(value) || !exactKeys(value, ["grant_id", "document_id", "space_id", "revision", "role", "scope", "recipient"]) ||
    value.grant_id !== grant || value.document_id !== page || !nonempty(value.space_id) || !nonempty(value.revision) ||
    value.role !== "viewer" || value.scope !== "page" || !object(value.recipient) ||
    !exactKeys(value.recipient, ["available", "display_name"]) || typeof value.recipient.available !== "boolean" ||
    !(value.recipient.display_name === null || typeof value.recipient.display_name === "string") ||
    (!value.recipient.available && value.recipient.display_name !== null)) throw unavailable();
  return value;
}
export function parseServerMovePreservedEvidence(
  value: unknown,
  space: string,
  page: string,
  operation: string,
): ServerMovePreservedEvidence {
  if (
    !object(value) ||
    !exactKeys(value, [
      "space_id",
      "document_id",
      "operation_id",
      "kind",
      "editable",
      "restoration_available",
      "entries",
    ]) ||
    value.space_id !== space ||
    value.document_id !== page ||
    value.operation_id !== operation ||
    value.kind !== "immutable_metadata_evidence" ||
    value.editable !== false ||
    value.restoration_available !== false ||
    !Array.isArray(value.entries)
  )
    throw unavailable();
  for (const entry of value.entries) {
    if (
      !object(entry) ||
      !exactKeys(entry, ["family", "source_id", "disposition", "original", "expected_active"]) ||
      !["tag", "property", "workflow", "incident_relation", "identifier", "maintenance", "page_viewer_grant"].includes(
        String(entry.family),
      ) ||
      !nonempty(entry.source_id) ||
      !["map", "create", "preserve", "remove", "rename", "clear"].includes(
        String(entry.disposition),
      ) ||
      !Object.prototype.hasOwnProperty.call(entry, "original") ||
      !Object.prototype.hasOwnProperty.call(entry, "expected_active") ||
      !jsonValue(entry.original) ||
      !jsonValue(entry.expected_active)
    )
      throw unavailable();
    if (entry.family === "page_viewer_grant") {
      if (entry.disposition !== "preserve" && entry.disposition !== "remove") throw unavailable();
      parseServerMoveViewerGrantEvidence(entry.original, page, entry.source_id);
      if (entry.disposition === "remove") {
        if (entry.expected_active !== null) throw unavailable();
      } else parseServerMoveViewerGrantEvidence(entry.expected_active, page, entry.source_id);
    }
  }
  return value as unknown as ServerMovePreservedEvidence;
}

/** Identity/target fence only; the server separately authorizes every inspection. */
export function capturePreservedMetadataContext(
  getCurrent: () => { spaceId: string; documentId: string; accountId: string | null },
) {
  const original = { ...getCurrent() };
  const credential = capturePageMetadataLoadGuard();
  return {
    assertCurrent() {
      credential();
      const now = getCurrent();
      if (
        now.spaceId !== original.spaceId ||
        now.documentId !== original.documentId ||
        now.accountId !== original.accountId
      )
        throw new Error("Account or page changed. Reopen preserved metadata.");
    },
  };
}
