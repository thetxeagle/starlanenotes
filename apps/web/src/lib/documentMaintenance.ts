export type DocumentMaintenance =
  | { kind: "unassigned" }
  | { kind: "user"; user_id: string }
  | { kind: "group"; group_id: string };
export interface DocumentMaintenanceSource {
  page_updated_at: string;
  published_at: string | null;
  maintenance_user_id: string | null;
  maintenance_group_id: string | null;
}
export interface DocumentMaintenanceIntent {
  id: string;
  space_id: string;
  maintenance: DocumentMaintenance;
  source: DocumentMaintenanceSource;
}
export interface DocumentMaintenanceTargets {
  users: { id: string; name: string; active: true }[];
  groups: { id: string; name: string; managed_space_id: string | null }[];
  groups_available: boolean;
}
export interface DocumentMaintenanceCatalog {
  server_base: string;
  account_id: string;
  space_id: string;
  targets: DocumentMaintenanceTargets;
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("Maintenance metadata must be an object");
  return value as Record<string, unknown>;
}
function exact(value: Record<string, unknown>, keys: string[]): void {
  if (Object.keys(value).length !== keys.length || keys.some((key) => !(key in value)))
    throw new Error("Maintenance metadata requires exact intended fields");
}
function id(value: unknown): string {
  if (typeof value !== "string" || !value || !value.trim())
    throw new Error("Maintenance identity must be a nonempty exact ID");
  return value;
}
function nullableId(value: unknown): string | null {
  return value === null ? null : id(value);
}
export function normalizeDocumentMaintenance(value: unknown): DocumentMaintenance {
  const v = object(value);
  if (v.kind === "unassigned") {
    exact(v, ["kind"]);
    return { kind: "unassigned" };
  }
  if (v.kind === "user") {
    exact(v, ["kind", "user_id"]);
    return { kind: "user", user_id: id(v.user_id) };
  }
  if (v.kind === "group") {
    exact(v, ["kind", "group_id"]);
    return { kind: "group", group_id: id(v.group_id) };
  }
  throw new Error("Maintenance assignment must be unassigned, person or team");
}
export function maintenanceFields(maintenance: DocumentMaintenance): {
  maintenance_user_id: string | null;
  maintenance_group_id: string | null;
} {
  const v = normalizeDocumentMaintenance(maintenance);
  return {
    maintenance_user_id: v.kind === "user" ? v.user_id : null,
    maintenance_group_id: v.kind === "group" ? v.group_id : null,
  };
}
export function normalizeDocumentMaintenanceFields<T extends object>(value: T): T {
  const fields = value as T & { maintenance_user_id?: unknown; maintenance_group_id?: unknown };
  const user =
    fields.maintenance_user_id === undefined ? undefined : nullableId(fields.maintenance_user_id);
  const group =
    fields.maintenance_group_id === undefined ? undefined : nullableId(fields.maintenance_group_id);
  if (user && group) throw new Error("A page cannot have both a maintenance person and team");
  return {
    ...value,
    ...(user !== undefined ? { maintenance_user_id: user } : {}),
    ...(group !== undefined ? { maintenance_group_id: group } : {}),
  };
}
export function parseDocumentMaintenanceIntent(value: unknown): DocumentMaintenanceIntent {
  const v = object(value);
  exact(v, ["id", "space_id", "maintenance", "source"]);
  const s = object(v.source);
  exact(s, ["page_updated_at", "published_at", "maintenance_user_id", "maintenance_group_id"]);
  if (
    typeof s.page_updated_at !== "string" ||
    !Number.isFinite(Date.parse(s.page_updated_at)) ||
    !(
      s.published_at === null ||
      (typeof s.published_at === "string" && Number.isFinite(Date.parse(s.published_at)))
    )
  )
    throw new Error("Maintenance source requires explicit valid timestamps");
  const pair = normalizeDocumentMaintenanceFields({
    maintenance_user_id: nullableId(s.maintenance_user_id),
    maintenance_group_id: nullableId(s.maintenance_group_id),
  });
  return {
    id: id(v.id),
    space_id: id(v.space_id),
    maintenance: normalizeDocumentMaintenance(v.maintenance),
    source: { page_updated_at: s.page_updated_at, published_at: s.published_at, ...pair },
  };
}
export function normalizeDocumentMaintenanceTargets(value: unknown): DocumentMaintenanceTargets {
  const v = object(value);
  exact(v, ["users", "groups", "groups_available"]);
  if (
    !Array.isArray(v.users) ||
    !Array.isArray(v.groups) ||
    typeof v.groups_available !== "boolean"
  )
    throw new Error("Maintenance target catalog is invalid");
  const users = v.users.map((raw) => {
    const r = object(raw);
    exact(r, ["id", "name", "active"]);
    if (r.active !== true || typeof r.name !== "string")
      throw new Error("Maintenance person is invalid");
    return { id: id(r.id), name: r.name, active: true as const };
  });
  const groups = v.groups.map((raw) => {
    const r = object(raw);
    exact(r, ["id", "name", "managed_space_id"]);
    if (typeof r.name !== "string") throw new Error("Maintenance team is invalid");
    return { id: id(r.id), name: r.name, managed_space_id: nullableId(r.managed_space_id) };
  });
  if (
    new Set(users.map((r) => r.id)).size !== users.length ||
    new Set(groups.map((r) => r.id)).size !== groups.length ||
    (!v.groups_available && groups.length)
  )
    throw new Error("Maintenance target catalog is ambiguous");
  return { users, groups, groups_available: v.groups_available };
}
export function normalizeDocumentMaintenanceCatalogs(value: unknown): DocumentMaintenanceCatalog[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) throw new Error("Maintenance target catalogs must be an array");
  const records = value.map((raw) => {
    const r = object(raw);
    exact(r, ["server_base", "account_id", "space_id", "targets"]);
    return {
      server_base: id(r.server_base),
      account_id: id(r.account_id),
      space_id: id(r.space_id),
      targets: normalizeDocumentMaintenanceTargets(r.targets),
    };
  });
  if (
    new Set(records.map((r) => JSON.stringify([r.server_base, r.account_id, r.space_id]))).size !==
    records.length
  )
    throw new Error("Maintenance target catalog provenance is ambiguous");
  return records;
}
