import { beforeEach, describe, expect, it, vi } from "vitest";
const context = vi.hoisted(() => ({
  base: "/api/v1",
  token: "original",
  refresh: "refresh",
  mode: "server",
}));
vi.mock("./api", () => ({
  api: { getToken: () => context.token, getRefreshToken: () => context.refresh },
  getApiBase: () => context.base,
}));
vi.mock("./launchMode", () => ({ getCurrentLaunchMode: () => context.mode }));
import {
  parseStorageHealth,
  readStorageHealth,
  readStorageHealthObservation,
  storageQuantity,
  storageStatus,
} from "./adminStorageHealth";
function report() {
  return {
    observed_at: "2026-10-10T13:00:00Z",
    database: {
      engine: "sqlite",
      connection_status: "available",
      migration_history: {
        status: "matching",
        expected_count: 49,
        applied_count: 49,
        scope: "applied_history_only",
      },
    },
    files: {
      backend: "local",
      location: "/data",
      read_status: "available",
      integrity_status: "not_checked",
    },
    recorded_attachment_usage: {
      status: "available",
      attachments: "1",
      bytes: "9007199254740993",
      scope: "attachment_database_records_only",
    },
    limits: { max_upload_bytes: "1048576", quota_status: "not_available" },
    backup: { status: "unknown", completed_at: null },
    verified_restore: { status: "unknown", completed_at: null },
  };
}
beforeEach(() => {
  Object.assign(context, {
    base: "/api/v1",
    token: "original",
    refresh: "refresh",
    mode: "server",
  });
  vi.stubGlobal("localStorage", { getItem: () => context.token });
});
describe("fresh administrator storage observation", () => {
  it("never labels unknown, uninspected integrity or unavailable quota as success", () => {
    const value = parseStorageHealth(report());
    expect(storageStatus(value.backup.status)).toBe("Unknown");
    expect(storageStatus(value.verified_restore.status)).toBe("Unknown");
    expect(storageStatus(value.files.integrity_status)).toBe("Not checked");
    expect(storageStatus(value.limits.quota_status)).toBe("Not available");
    expect(storageStatus("future_status")).toBe("Unknown");
    expect(storageStatus("__proto__")).toBe("Unknown");
    expect(storageStatus("constructor")).toBe("Unknown");
  });
  it("keeps large recorded byte counts exact without floating-point rounding", () => {
    expect(storageQuantity("9007199254740993", true).replace(/,/g, "")).toBe(
      "9007199254740993 bytes",
    );
    expect(storageQuantity(null)).toBe("Unavailable");
  });
  it("refuses a claimed completion without its real timestamp and malformed counts", () => {
    const value = report();
    value.backup.status = "recorded_completed";
    expect(() => parseStorageHealth(value)).toThrow(/unavailable/);
    value.backup.status = "unknown";
    value.recorded_attachment_usage.bytes = "-1";
    expect(() => parseStorageHealth(value)).toThrow(/unavailable/);
  });
  it("refuses unfamiliar scopes instead of presenting them as recorded attachment usage", () => {
    const value = report();
    value.recorded_attachment_usage.scope = "physical_files";
    expect(() => parseStorageHealth(value)).toThrow(/unavailable/);
    value.recorded_attachment_usage.scope = "attachment_database_records_only";
    value.database.migration_history.scope = "full_integrity";
    expect(() => parseStorageHealth(value)).toThrow(/unavailable/);
    value.database.migration_history.scope = "applied_history_only";
    expect(() =>
      parseStorageHealth({
        ...value,
        recorded_attachment_usage: { ...value.recorded_attachment_usage, bytes: null },
      }),
    ).toThrow(/unavailable/);
  });
  it("uses an authenticated no-store GET and only the returned current report", async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(report())));
    vi.stubGlobal("fetch", fetcher);
    const signal = new AbortController().signal;
    expect((await readStorageHealth(signal)).files.integrity_status).toBe("not_checked");
    expect(fetcher).toHaveBeenCalledWith("/api/v1/admin/storage-health", {
      headers: { Authorization: "Bearer original" },
      cache: "no-store",
      signal,
    });
  });
  it.each(["token", "base", "refresh"] as const)(
    "discards response after original %s changes during the network wait",
    async (key) => {
      vi.stubGlobal(
        "fetch",
        vi.fn(async () => {
          context[key] = "changed";
          return new Response(JSON.stringify(report()));
        }),
      );
      await expect(readStorageHealth(new AbortController().signal)).rejects.toThrow(/unavailable/);
    },
  );
  it("discards response after context changes while JSON is being read", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        json: async () => {
          context.mode = "local";
          return report();
        },
      })),
    );
    await expect(readStorageHealth(new AbortController().signal)).rejects.toThrow(/unavailable/);
  });
  it("fails offline or refused reads without cached success or raw server error text", async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValue(new Response("private backend details", { status: 403 }));
    vi.stubGlobal("fetch", fetcher);
    await expect(readStorageHealth(new AbortController().signal)).rejects.toThrow(
      /Storage status is unavailable/,
    );
    fetcher.mockRejectedValueOnce(new Error("offline"));
    await expect(readStorageHealth(new AbortController().signal)).rejects.toThrow("offline");
    expect(fetcher).toHaveBeenCalledTimes(2);
  });
  it("does not request server health from standalone mode", async () => {
    context.mode = "local";
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    await expect(readStorageHealth(new AbortController().signal)).rejects.toThrow(/unavailable/);
    expect(fetcher).not.toHaveBeenCalled();
  });
});

describe("conservative attachment capacity", () => {
  const capacity = () => ({
    quota_status: "configured",
    quota_bytes: "9223372036854775807",
    accounting_status: "available",
    logical_retained_bytes: "3",
    charged_retained_bytes: "9007199254740993",
    reserved_maximum_bytes: "7",
    charged_total_bytes: "9007199254741000",
    scope: "validated_attachment_charge_and_reservations",
  });
  it("preserves exact conservative charge, pending maxima and independent unlimited configuration", () => {
    const value = { ...report(), attachment_capacity: capacity() };
    expect(parseStorageHealth(value).attachment_capacity?.charged_total_bytes).toBe(
      "9007199254741000",
    );
    value.attachment_capacity.quota_status = "unlimited";
    Object.assign(value.attachment_capacity, { quota_bytes: null });
    expect(parseStorageHealth(value).attachment_capacity?.quota_status).toBe("unlimited");
  });
  it("does not turn unavailable proof into zero and accepts old server reports without capacity", () => {
    const value = {
      ...report(),
      attachment_capacity: {
        ...capacity(),
        accounting_status: "unavailable",
        logical_retained_bytes: null,
        charged_retained_bytes: null,
        reserved_maximum_bytes: null,
        charged_total_bytes: null,
      },
    };
    expect(parseStorageHealth(value).attachment_capacity?.charged_total_bytes).toBeNull();
    expect(parseStorageHealth(report()).attachment_capacity).toBeUndefined();
  });
  it("refuses missing, undercharged, inconsistent, stale and overflowing accounting evidence", () => {
    for (const change of [
      { charged_retained_bytes: "2" },
      { charged_total_bytes: "0" },
      { logical_retained_bytes: null },
      { charged_retained_bytes: "-1" },
      { charged_total_bytes: "9223372036854775808" },
      { quota_bytes: null },
      { scope: "raw_disk_usage" },
      { accounting_status: "unavailable" },
    ]) {
      expect(() =>
        parseStorageHealth({ ...report(), attachment_capacity: { ...capacity(), ...change } }),
      ).toThrow(/unavailable/);
    }
  });
});

describe("bounded original-session health response", () => {
  it("refuses a body beyond one MiB and invalid UTF-8 before parsing", async () => {
    for (const body of [new Uint8Array(1024 * 1024 + 1), new Uint8Array([255])]) {
      vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(body)));
      await expect(readStorageHealth(new AbortController().signal)).rejects.toThrow();
    }
  });
  it.each(["token", "base", "refresh"] as const)(
    "cancels after original %s changes during body streaming",
    async (key) => {
      let first = true,
        cancelled = false;
      const stream = new ReadableStream<Uint8Array>({
        pull(controller) {
          if (first) {
            first = false;
            controller.enqueue(new TextEncoder().encode('{"observed_at":'));
            return;
          }
          context[key] = "changed-during-body";
          return new Promise<void>(() => undefined);
        },
        cancel() {
          cancelled = true;
        },
      });
      const fetcher = vi.fn().mockResolvedValue(new Response(stream));
      vi.stubGlobal("fetch", fetcher);
      await expect(readStorageHealth(new AbortController().signal)).rejects.toThrow(/unavailable/);
      expect(cancelled).toBe(true);
      expect(fetcher).toHaveBeenCalledTimes(1);
    },
  );
  it("does not dispatch an already aborted observation", async () => {
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    const original = new AbortController();
    original.abort();
    await expect(readStorageHealth(original.signal)).rejects.toThrow(/unavailable/);
    expect(fetcher).not.toHaveBeenCalled();
  });
});

describe("scoped historical logical and reserved observation", () => {
  const known = () => ({
    status: "observed",
    scope: "restored_sqlite_historical_logical_and_reserved_v1",
    observed_at: "2026-10-15T13:00:00Z",
    logical_retained_floor_bytes: "84",
    original_pending_maximum_bytes: "1048576",
    conservative_known_sum_bytes: "1048660",
    reference_semantics: "not_established",
    write_admission: "unavailable",
  });
  const scoped = (value: unknown = known()) => ({
    ...report(),
    attachment_capacity: {
      quota_status: "configured",
      quota_bytes: "9223372036854775807",
      accounting_status: "unavailable",
      logical_retained_bytes: null,
      charged_retained_bytes: null,
      reserved_maximum_bytes: null,
      charged_total_bytes: null,
      scope: "validated_attachment_charge_and_reservations",
      readonly_known_usage: value,
    },
  });
  it("keeps the known sum distinct from unavailable generic charge and independent quota", () => {
    const result = parseStorageHealth(scoped());
    expect(result.attachment_capacity?.readonly_known_usage?.conservative_known_sum_bytes).toBe(
      "1048660",
    );
    expect(result.attachment_capacity?.charged_total_bytes).toBeNull();
    expect(result.attachment_capacity?.accounting_status).toBe("unavailable");
    expect(result.attachment_capacity?.quota_status).toBe("configured");
    expect(result.verified_restore.status).toBe("unknown");
    expect(parseStorageHealth(report()).attachment_capacity).toBeUndefined();
  });
  it("accepts a proved zero and exact signed-i64 upper boundary without Number rounding", () => {
    for (const amount of ["0", "9007199254740993", "9223372036854775807"]) {
      const value = {
        ...known(),
        logical_retained_floor_bytes: amount,
        original_pending_maximum_bytes: "0",
        conservative_known_sum_bytes: amount,
      };
      const parsed = parseStorageHealth(scoped(value)).attachment_capacity?.readonly_known_usage;
      expect(parsed?.conservative_known_sum_bytes).toBe(amount);
      expect(
        storageQuantity(parsed?.conservative_known_sum_bytes ?? null, true).replace(/,/g, ""),
      ).toBe(`${amount} bytes`);
    }
  });
  it("keeps an unavailable scoped result null instead of substituting zero", () => {
    const value = {
      ...known(),
      status: "unavailable",
      observed_at: null,
      logical_retained_floor_bytes: null,
      original_pending_maximum_bytes: null,
      conservative_known_sum_bytes: null,
    };
    const parsed = parseStorageHealth(scoped(value)).attachment_capacity?.readonly_known_usage;
    expect(parsed?.status).toBe("unavailable");
    expect(parsed?.conservative_known_sum_bytes).toBeNull();
  });
  it.each([
    null,
    {},
    { ...known(), scope: "physical_usage" },
    { ...known(), status: "complete" },
    { ...known(), observed_at: "not-a-time" },
    { ...known(), observed_at: "0" },
    { ...known(), observed_at: "2026-10-15" },
    { ...known(), observed_at: null },
    { ...known(), reference_semantics: "established" },
    { ...known(), write_admission: "available" },
    { ...known(), writer_lease: true },
    { ...known(), conservative_known_sum_bytes: "1048659" },
    { ...known(), logical_retained_floor_bytes: "-1" },
    { ...known(), logical_retained_floor_bytes: "084" },
    { ...known(), logical_retained_floor_bytes: 84 },
    { ...known(), original_pending_maximum_bytes: null },
    { ...known(), conservative_known_sum_bytes: "9223372036854775808" },
    {
      ...known(),
      logical_retained_floor_bytes: "9223372036854775807",
      original_pending_maximum_bytes: "1",
    },
    { ...known(), status: "unavailable" },
    {
      ...known(),
      status: "unavailable",
      observed_at: null,
      logical_retained_floor_bytes: null,
      original_pending_maximum_bytes: null,
    },
  ])(
    "refuses malformed, incomplete, contradictory or authority-looking scoped facts %#",
    (value) => {
      expect(() => parseStorageHealth(scoped(value))).toThrow(/unavailable/);
    },
  );
  it("refuses scoped facts alongside otherwise valid available generic accounting", () => {
    const value = scoped();
    const capacity = {
      ...value.attachment_capacity,
      accounting_status: "available",
      logical_retained_bytes: "3",
      charged_retained_bytes: "3",
      reserved_maximum_bytes: "7",
      charged_total_bytes: "10",
    };
    expect(() => parseStorageHealth({ ...value, attachment_capacity: capacity })).toThrow(
      /unavailable/,
    );
  });
  it("discards a scoped success when the original credential changes during the network wait", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        context.token = "different-session";
        return new Response(JSON.stringify(scoped()));
      }),
    );
    await expect(readStorageHealth(new AbortController().signal)).rejects.toThrow(/unavailable/);
  });
});


describe("completed storage observation context", () => {
  it.each(["base", "token", "refresh", "mode"] as const)(
    "invalidates completed facts after the original %s changes",
    async (field) => {
      vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(report()))));
      const observation = await readStorageHealthObservation(new AbortController().signal);
      expect(observation.isCurrent()).toBe(true);
      context[field] = field === "mode" ? "local" : "different-context";
      expect(observation.isCurrent()).toBe(false);
    },
  );
  it("invalidates completed facts when browser credential storage disagrees", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(report()))));
    const observation = await readStorageHealthObservation(new AbortController().signal);
    vi.stubGlobal("localStorage", { getItem: () => "other-browser-session" });
    expect(observation.isCurrent()).toBe(false);
  });
  it("retires a completed observation when its owning view aborts", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(report()))));
    const controller = new AbortController();
    const observation = await readStorageHealthObservation(controller.signal);
    expect(observation.isCurrent()).toBe(true);
    controller.abort();
    expect(observation.isCurrent()).toBe(false);
  });
});
