import {
  parseApplicationRestoreHistory,
  type ApplicationRestoreHistory,
} from "./applicationRestoreHistory";
import { api, getApiBase } from "./api";
import { getCurrentLaunchMode } from "./launchMode";

export interface ReadonlyKnownUsage {
  status: "observed" | "unavailable";
  scope: "restored_sqlite_historical_logical_and_reserved_v1";
  observed_at: string | null;
  logical_retained_floor_bytes: string | null;
  original_pending_maximum_bytes: string | null;
  conservative_known_sum_bytes: string | null;
  reference_semantics: "not_established";
  write_admission: "unavailable";
}
export interface AttachmentCapacityObservation {
  quota_status: "configured" | "unlimited" | "unavailable";
  quota_bytes: string | null;
  accounting_status: "available" | "unavailable";
  logical_retained_bytes: string | null;
  charged_retained_bytes: string | null;
  reserved_maximum_bytes: string | null;
  charged_total_bytes: string | null;
  scope: "validated_attachment_charge_and_reservations";
  readonly_known_usage?: ReadonlyKnownUsage;
}
export interface StorageHealthReport {
  observed_at: string;
  database: {
    engine: string;
    connection_status: string;
    migration_history: {
      status: string;
      expected_count: number;
      applied_count: number | null;
      scope: string;
    };
  };
  files: { backend: string; location: string; read_status: string; integrity_status: string };
  recorded_attachment_usage: {
    status: string;
    attachments: string | null;
    bytes: string | null;
    scope: string;
  };
  limits: { max_upload_bytes: string | null; quota_status: string };
  attachment_capacity?: AttachmentCapacityObservation;
  backup: { status: string; completed_at: string | null };
  verified_restore: { status: string; completed_at: string | null };
  application_restore_history?: ApplicationRestoreHistory;
}
const failure = () =>
  new Error("Storage status is unavailable. Check your connection and sign-in, then retry.");
const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown): value is string => typeof value === "string" && value.length > 0;
const count = (value: unknown) =>
  value === null || (typeof value === "string" && /^(0|[1-9]\d*)$/.test(value));
const integer = (value: unknown) =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
const timestamp = (value: unknown) => text(value) && Number.isFinite(Date.parse(value));
const readonlyAmountKeys = [
  "logical_retained_floor_bytes",
  "original_pending_maximum_bytes",
  "conservative_known_sum_bytes",
] as const;
const readonlyKnownKeys = [
  "status",
  "scope",
  "observed_at",
  ...readonlyAmountKeys,
  "reference_semantics",
  "write_admission",
];
function parseReadonlyKnownUsage(value: unknown): void {
  if (
    !record(value) ||
    Object.keys(value).length !== readonlyKnownKeys.length ||
    Object.keys(value).some((key) => !readonlyKnownKeys.includes(key)) ||
    value.scope !== "restored_sqlite_historical_logical_and_reserved_v1" ||
    value.reference_semantics !== "not_established" ||
    value.write_admission !== "unavailable"
  )
    throw failure();
  if (value.status === "unavailable") {
    if (value.observed_at !== null || readonlyAmountKeys.some((key) => value[key] !== null))
      throw failure();
    return;
  }
  if (
    value.status !== "observed" ||
    !timestamp(value.observed_at) ||
    !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)$/.test(String(value.observed_at))
  )
    throw failure();
  const maximum = 9223372036854775807n;
  for (const key of readonlyAmountKeys) {
    const amount = value[key];
    if (
      typeof amount !== "string" ||
      amount.length > 19 ||
      !/^(0|[1-9]\d*)$/.test(amount) ||
      BigInt(amount) > maximum
    )
      throw failure();
  }
  const known =
    BigInt(String(value.logical_retained_floor_bytes)) +
    BigInt(String(value.original_pending_maximum_bytes));
  if (known > maximum || known !== BigInt(String(value.conservative_known_sum_bytes)))
    throw failure();
}
export function parseStorageHealth(value: unknown): StorageHealthReport {
  if (!record(value) || !timestamp(value.observed_at)) throw failure();
  const {
    database: db,
    files,
    recorded_attachment_usage: usage,
    limits,
    backup,
    verified_restore: restore,
  } = value;
  if (
    !record(db) ||
    !text(db.engine) ||
    !text(db.connection_status) ||
    !record(db.migration_history)
  )
    throw failure();
  const migration = db.migration_history;
  if (
    !text(migration.status) ||
    migration.scope !== "applied_history_only" ||
    !integer(migration.expected_count) ||
    !(migration.applied_count === null || integer(migration.applied_count))
  )
    throw failure();
  if (
    !record(files) ||
    !text(files.backend) ||
    typeof files.location !== "string" ||
    !text(files.read_status) ||
    !text(files.integrity_status)
  )
    throw failure();
  if (
    !record(usage) ||
    !text(usage.status) ||
    usage.scope !== "attachment_database_records_only" ||
    !count(usage.attachments) ||
    !count(usage.bytes) ||
    (usage.status === "available" && (usage.attachments === null || usage.bytes === null))
  )
    throw failure();
  if (!record(limits) || !count(limits.max_upload_bytes) || !text(limits.quota_status))
    throw failure();
  for (const recovery of [backup, restore]) {
    if (
      !record(recovery) ||
      !text(recovery.status) ||
      !(recovery.completed_at === null || timestamp(recovery.completed_at))
    )
      throw failure();
    if (recovery.status === "recorded_completed" && recovery.completed_at === null) throw failure();
  }
  if (value.attachment_capacity !== undefined) {
    const capacity = value.attachment_capacity;
    if (
      !record(capacity) ||
      capacity.scope !== "validated_attachment_charge_and_reservations" ||
      !["configured", "unlimited", "unavailable"].includes(String(capacity.quota_status)) ||
      !["available", "unavailable"].includes(String(capacity.accounting_status))
    )
      throw failure();
    const quantities = [
      "logical_retained_bytes",
      "charged_retained_bytes",
      "reserved_maximum_bytes",
      "charged_total_bytes",
    ] as const;
    for (const key of ["quota_bytes", ...quantities]) {
      const amount = capacity[key];
      if (
        !count(amount) ||
        (amount !== null &&
          (String(amount).length > 19 || BigInt(String(amount)) > 9223372036854775807n))
      )
        throw failure();
    }
    if ((capacity.quota_status === "configured") !== (capacity.quota_bytes !== null))
      throw failure();
    if (
      quantities.some(
        (key) => (capacity.accounting_status === "available") !== (capacity[key] !== null),
      )
    )
      throw failure();
    if (capacity.readonly_known_usage !== undefined) {
      if (
        capacity.accounting_status !== "unavailable" ||
        quantities.some((key) => capacity[key] !== null)
      )
        throw failure();
      parseReadonlyKnownUsage(capacity.readonly_known_usage);
    }
    if (capacity.accounting_status === "available") {
      const logical = BigInt(String(capacity.logical_retained_bytes));
      const charged = BigInt(String(capacity.charged_retained_bytes));
      if (
        charged < logical ||
        BigInt(String(capacity.charged_total_bytes)) !==
          charged + BigInt(String(capacity.reserved_maximum_bytes))
      )
        throw failure();
    }
  }
  if (value.application_restore_history !== undefined)
    parseApplicationRestoreHistory(value.application_restore_history);
  return value as unknown as StorageHealthReport;
}
/** Fresh read only: no offline cache, token refresh, mutation queue or vault mirror. */
export interface StorageHealthObservation {
  report: StorageHealthReport;
  isCurrent: () => boolean;
}
export async function readStorageHealth(signal: AbortSignal): Promise<StorageHealthReport> {
  return (await readStorageHealthObservation(signal)).report;
}
/** Retain the original read context so a completed observation can be cleared. */
export async function readStorageHealthObservation(signal: AbortSignal): Promise<StorageHealthObservation> {
  const base = getApiBase(),
    token = api.getToken(),
    refresh = api.getRefreshToken();
  const same = () =>
    !signal.aborted &&
    getCurrentLaunchMode() === "server" &&
    getApiBase() === base &&
    api.getToken() === token &&
    localStorage.getItem("access_token") === token &&
    api.getRefreshToken() === refresh;
  if (!token || !same()) throw failure();
  const response = await fetch(`${base}/admin/storage-health`, {
    headers: { Authorization: `Bearer ${token}` },
    cache: "no-store",
    signal,
  });
  if (!response.ok || !same()) throw failure();
  const reader = response.body?.getReader();
  if (!reader) throw failure();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let bytes = 0,
    raw = "";
  try {
    while (true) {
      const part = await reader.read();
      if (!same()) throw failure();
      if (part.done) break;
      bytes += part.value.byteLength;
      if (bytes > 1024 * 1024) throw failure();
      raw += decoder.decode(part.value, { stream: true });
    }
    raw += decoder.decode();
  } catch (error) {
    void reader.cancel().catch(() => undefined);
    throw error;
  } finally {
    reader.releaseLock();
  }
  const value: unknown = JSON.parse(raw);
  if (!same()) throw failure();
  return { report: parseStorageHealth(value), isCurrent: same };
}
export function storageStatus(value: string): string {
  const label = (
    {
      available: "Available",
      unavailable: "Unavailable",
      matching: "Matching",
      mismatch: "Mismatch",
      not_checked: "Not checked",
      not_available: "Not available",
      configured: "Configured",
      unlimited: "Unlimited",
      unknown: "Unknown",
      recorded_completed: "Recorded completion",
    } as Record<string, string>
  )[value];
  return typeof label === "string" ? label : "Unknown";
}
export function storageQuantity(value: string | null, bytes = false): string {
  if (value === null) return "Unavailable";
  return `${BigInt(value).toLocaleString()}${bytes ? " bytes" : ""}`;
}
