const semantics = "historical_completed_isolated_application_test_at_recorded_time" as const;
export const applicationHistoryScope = "restored-pg-ledger1-ordinary-application-v1" as const;
export const sqliteApplicationHistoryScope = "restored-sqlite-ledger1-ordinary-application-v1" as const;
export type ApplicationRestoreHistory =
  | { status: "unavailable"; semantics: typeof semantics; full_supported_restore_verified: false }
  | {
      status: "completed_isolated_test";
      semantics: typeof semantics;
      full_supported_restore_verified: false;
      completed_at: string;
      scope: typeof applicationHistoryScope | typeof sqliteApplicationHistoryScope;
      operation: string;
      pages: number;
      drafts: number;
      registered_assets: number;
      legacy_assets: number;
      exact_binary_bytes: string;
      sampled_humans: number;
    };
const failure = () => new Error("Historical application-test observation is unavailable.");
const counts = ["pages", "drafts", "registered_assets", "legacy_assets", "sampled_humans"] as const;
const fields = ["completed_at", "scope", "operation", ...counts, "exact_binary_bytes"] as const;
export function parseApplicationRestoreHistory(value: unknown): ApplicationRestoreHistory {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw failure();
  const item = value as Record<string, unknown>;
  if (item.semantics !== semantics || item.full_supported_restore_verified !== false)
    throw failure();
  if (item.status === "unavailable") {
    if (fields.some((key) => item[key] != null)) throw failure();
    return { status: "unavailable", semantics, full_supported_restore_verified: false };
  }
  if (
    item.status !== "completed_isolated_test" ||
    (item.scope !== applicationHistoryScope && item.scope !== sqliteApplicationHistoryScope) ||
    typeof item.completed_at !== "string" ||
    !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)$/.test(item.completed_at) ||
    !Number.isFinite(Date.parse(item.completed_at)) ||
    typeof item.operation !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(item.operation)
  )
    throw failure();
  for (const key of counts)
    if (
      typeof item[key] !== "number" ||
      !Number.isSafeInteger(item[key]) ||
      (item[key] as number) < 0 ||
      (item[key] as number) > 256
    )
      throw failure();
  if (
    (item.pages as number) < 1 ||
    (item.sampled_humans as number) < 1 ||
    (item.sampled_humans as number) > 8 ||
    (item.registered_assets as number) + (item.legacy_assets as number) > 256 ||
    typeof item.exact_binary_bytes !== "string" ||
    !/^(0|[1-9]\d*)$/.test(item.exact_binary_bytes) ||
    item.exact_binary_bytes.length > 10 ||
    BigInt(item.exact_binary_bytes) > 1073741824n
  )
    throw failure();
  return item as unknown as ApplicationRestoreHistory;
}
