export const DELETE_OPERATION_HEADER = "x-starlane-delete-operation";
export interface OriginalDeleteTarget { created_at: string; deleted_at: string | null }
export const canonicalDeleteOperation = (v: unknown): v is string => typeof v === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v);
export function deleteRoute(method: string, path: string): boolean { return method === "DELETE" && /^\/spaces\/[^/?#]+\/documents\/[^/?#]+\/trash$/.test(path); }
export function originalDeleteTarget(body: unknown): OriginalDeleteTarget | null {
  if (!body || typeof body !== "object" || Array.isArray(body)) return null;
  const target = (body as Record<string, unknown>).original_target;
  if (!target || typeof target !== "object" || Array.isArray(target)) return null;
  const row = target as Record<string, unknown>;
  return Object.keys(row).length === 2 && typeof row.created_at === "string" && !!row.created_at.trim() && row.created_at.length <= 4096 && (row.deleted_at === null || typeof row.deleted_at === "string" && !!row.deleted_at.trim() && row.deleted_at.length <= 4096) ? { created_at: row.created_at, deleted_at: row.deleted_at as string | null } : null;
}
export function requirePermanentDeleteTarget(target: { created_at?: string; deleted_at?: string | null }): OriginalDeleteTarget {
  const witness = originalDeleteTarget({original_target: target});
  if (!witness || witness.deleted_at === null) throw new Error("Original page and Trash identity unavailable. Reload Trash and review this page before permanent deletion.");
  return witness;
}
