import { api } from "@/lib/api";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { capturePageMetadataLoadGuard } from "@/lib/pageMetadataLoad";

const permissions = [
  "view",
  "comment",
  "create_content",
  "edit_content",
  "move_content",
  "duplicate_content",
  "manage_folders",
  "manage_templates",
  "archive_content",
  "delete_content",
  "manage_members",
  "manage_space",
] as const;
const sourceKinds = [
  "direct_space_role",
  "space_group_role",
  "global_group_role",
  "internal_space_fallback",
  "public_space_fallback",
  "instance_admin",
] as const;
const capabilityNames = [
  "view",
  "comment",
  "edit",
  "move",
  "manage_lock",
  "enable_public_link",
  "disable_public_link",
] as const;
const publicReasons = [
  "not_enabled",
  "deployment_disabled",
  "unpublished",
  "restricted",
  "missing_link",
] as const;
export interface PageAccessInspection {
  schema_version: 1;
  space_id: string;
  page_id: string;
  checked_at: string;
  capabilities: Record<(typeof capabilityNames)[number], boolean>;
  page_permissions?: (typeof permissions)[number][];
  page_permission_sources?: {kind: "direct_page_viewer"; grant_id: string; revision: string; permissions: ["view"]}[];
  space_permission_sources: {
    kind: (typeof sourceKinds)[number];
    permissions: (typeof permissions)[number][];
  }[];
  restrictions: { local: "inherited" | "restricted"; ancestor_restricted: boolean };
  public_link: {
    configured: boolean;
    policy_allows_anonymous: boolean;
    reason: (typeof publicReasons)[number] | null;
  };
  selected_private_draft: {
    scope: "none" | "owned" | "shared_viewer" | "shared_editor";
    view: boolean;
    edit: boolean;
  };
  sharing: { people_grants_available: boolean; guests_available: boolean };
}
function invalid(): never {
  throw new Error("Unable to verify this page's access report. Refresh or reopen Share page.");
}
function object(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return invalid();
  const row = value as Record<string, unknown>;
  if (
    Object.keys(row).length !== keys.length ||
    keys.some((key) => !Object.prototype.hasOwnProperty.call(row, key))
  )
    return invalid();
  return row;
}
function oneOf(value: unknown, choices: readonly string[]): boolean {
  return typeof value === "string" && choices.includes(value);
}
/** Validate the versioned projection, never derive grants from role names or cached page data. */
export function parsePageAccessInspection(
  value: unknown,
  spaceId: string,
  pageId: string,
): PageAccessInspection {
  const candidate = value as Record<string, unknown> | null;
  const additive = candidate !== null && typeof candidate === "object" && !Array.isArray(candidate) && Object.prototype.hasOwnProperty.call(candidate,"page_permissions");
  const row = object(value, [
    "schema_version",
    "space_id",
    "page_id",
    "checked_at",
    "capabilities",
    "space_permission_sources",
    "restrictions",
    "public_link",
    "selected_private_draft",
    "sharing",
    ...(additive ? ["page_permissions", "page_permission_sources"] : []),
  ]);
  if (
    row.schema_version !== 1 ||
    row.space_id !== spaceId ||
    row.page_id !== pageId ||
    !spaceId ||
    !pageId ||
    typeof row.checked_at !== "string" ||
    !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)$/.test(row.checked_at) ||
    !Number.isFinite(Date.parse(row.checked_at))
  )
    return invalid();
  const capabilities = object(row.capabilities, capabilityNames);
  if (
    capabilities.view !== true ||
    capabilityNames.some((name) => typeof capabilities[name] !== "boolean")
  )
    return invalid();
  if (!Array.isArray(row.space_permission_sources)) return invalid();
  const seenKinds = new Set<unknown>();
  for (const entry of row.space_permission_sources) {
    const source = object(entry, ["kind", "permissions"]);
    if (
      !oneOf(source.kind, sourceKinds) ||
      seenKinds.has(source.kind) ||
      !Array.isArray(source.permissions) ||
      !source.permissions.length ||
      new Set(source.permissions).size !== source.permissions.length ||
      source.permissions.some((permission) => !oneOf(permission, permissions))
    )
      return invalid();
    seenKinds.add(source.kind);
  }
  if (additive) {
    if (!Array.isArray(row.page_permissions) || new Set(row.page_permissions).size !== row.page_permissions.length || row.page_permissions.some(p => !oneOf(p,permissions)) || !Array.isArray(row.page_permission_sources) || row.page_permission_sources.length > 1) return invalid();
    for (const entry of row.page_permission_sources) {
      const source = object(entry,["kind","grant_id","revision","permissions"]);
      if (source.kind !== "direct_page_viewer" || typeof source.grant_id !== "string" || !source.grant_id || typeof source.revision !== "string" || !source.revision || !Array.isArray(source.permissions) || source.permissions.length !== 1 || source.permissions[0] !== "view" || !row.page_permissions.includes("view")) return invalid();
    }
  }
  const restrictions = object(row.restrictions, ["local", "ancestor_restricted"]);
  if (
    !oneOf(restrictions.local, ["inherited", "restricted"]) ||
    typeof restrictions.ancestor_restricted !== "boolean"
  )
    return invalid();
  const link = object(row.public_link, ["configured", "policy_allows_anonymous", "reason"]);
  if (
    typeof link.configured !== "boolean" ||
    typeof link.policy_allows_anonymous !== "boolean" ||
    (link.reason !== null && !oneOf(link.reason, publicReasons))
  )
    return invalid();
  if (
    (link.policy_allows_anonymous && (!link.configured || link.reason !== null)) ||
    (!link.policy_allows_anonymous && link.reason === null) ||
    (link.reason === "not_enabled" && link.configured) ||
    (!link.configured && link.reason !== "not_enabled") ||
    (link.policy_allows_anonymous &&
      (restrictions.local !== "inherited" || restrictions.ancestor_restricted))
  )
    return invalid();
  const draft = object(row.selected_private_draft, ["scope", "view", "edit"]);
  if (
    !oneOf(draft.scope, ["none", "owned", "shared_viewer", "shared_editor"]) ||
    typeof draft.view !== "boolean" ||
    typeof draft.edit !== "boolean"
  )
    return invalid();
  if (
    (draft.scope === "none" && (draft.view || draft.edit)) ||
    (draft.scope !== "none" && !draft.view) ||
    (draft.scope === "shared_viewer" && draft.edit)
  )
    return invalid();
  const sharing = object(row.sharing, ["people_grants_available", "guests_available"]);
  if (typeof sharing.people_grants_available !== "boolean" || typeof sharing.guests_available !== "boolean")
    return invalid();
  return row as unknown as PageAccessInspection;
}

/** POST intentionally avoids GET cache fallback. This report is a point-in-time display, not authorization. */
export async function inspectDocumentAccess(
  spaceId: string,
  pageId: string,
): Promise<PageAccessInspection> {
  const assertCurrent = capturePageMetadataLoadGuard();
  if (getCurrentLaunchMode() !== "server")
    throw new Error("Access inspection is available when connected to a server.");
  assertCurrent();
  const response = await api.fetch<unknown>(
    `/spaces/${encodeURIComponent(spaceId)}/documents/${encodeURIComponent(pageId)}/access/inspect`,
    { method: "POST" },
  );
  assertCurrent();
  return parsePageAccessInspection(response, spaceId, pageId);
}

/** An authenticated application route; never an anonymous/public-link URL. */
export function buildPageShareLink(
  spaceId: string,
  pageId: string,
  context: { native: boolean; apiBase: string; webOrigin: string },
): string {
  const route = buildDocumentRoutePath(spaceId, pageId);
  let base: URL;
  try {
    base = new URL(context.native ? context.apiBase : context.webOrigin);
  } catch {
    throw new Error("The connected server's page link is unavailable.");
  }
  if (
    !["http:", "https:"].includes(base.protocol) ||
    base.username ||
    base.password ||
    (context.native && !base.pathname.replace(/\/+$/, "").endsWith("/api/v1"))
  )
    throw new Error("The connected server's page link is unavailable.");
  const prefix = context.native
    ? base.pathname.replace(/\/+$/, "").slice(0, -"/api/v1".length)
    : "";
  return `${base.origin}${prefix}${route}`;
}

/** Keep a reused dialog bound to the page whose report was requested. */
export function capturePageShareTargetGuard(
  readTarget: () => { spaceId: string; pageId: string },
): () => void {
  const original = { ...readTarget() };
  return () => {
    const current = readTarget();
    if (current.spaceId !== original.spaceId || current.pageId !== original.pageId)
      throw new Error("Page changed. Reopen Share page for the current page.");
  };
}
