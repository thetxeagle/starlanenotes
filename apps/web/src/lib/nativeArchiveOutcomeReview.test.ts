import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { setCurrentLaunchMode } from "./launchMode";
import { setStorageMode, setSyncProvider } from "./storageProviders";
let token: string | null = "token",
  refresh: string | null = "refresh",
  base = "https://original.invalid",
  native = true;
vi.mock("./api", () => ({
  api: { getToken: () => token, getRefreshToken: () => refresh },
  getApiBase: () => base,
  isTauri: () => native,
}));
import {
  prepareNativeArchiveOutcomeReview,
  type NativeArchiveReviewBinding,
} from "./nativeArchiveOutcomeReview";
const binding: NativeArchiveReviewBinding = {
  context: {
    version: 1,
    serverBase: "local-vault",
    accountId: "original",
    sessionId: "original-session",
    cacheScope: "original-scope",
    storageMode: "local",
    provider: "none",
  },
  revision: "original",
  root: "/original-vault",
};
const op = "12345678-1234-4234-8234-123456789abc",
  other = "12345678-1234-4234-8234-123456789abd";
const report = (operation = op) => ({
  format: 1,
  operation,
  destination: "/destination/" + operation + ".zip",
  state: "unresolved_output_absent",
  journal_state: "publication_unknown",
  journal_sha256: "a".repeat(64),
  original_binding_sha256: "b".repeat(64),
  reviewed: false,
  automatic_retry: false,
  automatic_cleanup: false,
});
let invoke: ReturnType<typeof vi.fn>, disk: Map<string, string>;
beforeEach(() => {
  token = "token";
  refresh = "refresh";
  base = "https://original.invalid";
  native = true;
  disk = new Map([
    ["access_token", "token"],
    ["refresh_token", "refresh"],
    ["starlane_server_cache_v1:session-scope", "original-scope"],
  ]);
  const store = {
    getItem: (k: string) => disk.get(k) ?? null,
    setItem: (k: string, v: string) => disk.set(k, v),
    removeItem: (k: string) => disk.delete(k),
  };
  vi.stubGlobal("localStorage", store);
  const win = new EventTarget();
  Object.assign(win, { localStorage: store });
  vi.stubGlobal("window", win);
  setCurrentLaunchMode("local");
  setStorageMode("local");
  setSyncProvider("none");
  invoke = vi.fn(async (c: string, a: Record<string, unknown>) =>
    c === "list_native_vault_archive_export_outcomes"
      ? {
          format: 1,
          binding: a.originalBinding,
          parent_path: a.parentPath,
          inventory: {
            format: 1,
            operations: [report(), report(other)],
            automatic_retry: false,
            automatic_cleanup: false,
          },
        }
      : {
          format: 1,
          binding: a.originalBinding,
          parent_path: a.parentPath,
          operation: a.operation,
          result: {
            format: 1,
            operation: a.operation,
            reviewed: true,
            retry_authority: false,
            cleanup_authority: false,
          },
          retry_authority: false,
          cleanup_authority: false,
          serve_authority: false,
        },
  );
  Object.assign(win, { __TAURI_INTERNALS__: { invoke } });
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
it("retains exact immutable report peroperation without caller mutation or crossoperation erasure", async () => {
  const adapter = prepareNativeArchiveOutcomeReview(binding, "/destination");
  const rows = await adapter.pending();
  rows[0].journal_sha256 = "c".repeat(64);
  const before = JSON.stringify([...disk]);
  await adapter.acknowledgeNamedDestination(op, report().destination);
  expect(invoke.mock.calls[invoke.mock.calls.length - 1][1].originalReport.journal_sha256).toBe(
    "a".repeat(64),
  );
  await adapter.acknowledgeNamedDestination(other, report(other).destination);
  expect(invoke.mock.calls.map(([name]) => name)).toEqual([
    "list_native_vault_archive_export_outcomes",
    "acknowledge_native_vault_archive_export_review",
    "acknowledge_native_vault_archive_export_review",
  ]);
  expect(JSON.stringify([...disk])).toBe(before);
});
it("requires an inspected exact operation and destination before any review dispatch", async () => {
  const a = prepareNativeArchiveOutcomeReview(binding, "/destination");
  await expect(a.acknowledgeNamedDestination(op, report().destination)).rejects.toThrow(
    "Inspect original",
  );
  await a.pending();
  const n = invoke.mock.calls.length;
  await expect(a.acknowledgeNamedDestination(op, "/other.zip")).rejects.toThrow(
    "original archive destination",
  );
  expect(invoke.mock.calls).toHaveLength(n);
});
it("session change during inventory refuses retained evidence admission", async () => {
  const a = prepareNativeArchiveOutcomeReview(binding, "/destination");
  const original = invoke.getMockImplementation()! as (
    c: string,
    args: Record<string, unknown>,
  ) => Promise<Record<string, unknown>>;
  invoke.mockImplementation(async (c, args) => {
    const r = await original(c, args);
    disk.set("access_token", "foreign");
    return r;
  });
  await expect(a.pending()).rejects.toThrow("session/workspace changed");
  await expect(a.acknowledgeNamedDestination(op, report().destination)).rejects.toThrow(
    "session/workspace changed",
  );
  expect(invoke).toHaveBeenCalledTimes(1);
});
it("late session change after native review reports unknown and does not clear another operation", async () => {
  const a = prepareNativeArchiveOutcomeReview(binding, "/destination");
  await a.pending();
  const original = invoke.getMockImplementation()! as (
    c: string,
    args: Record<string, unknown>,
  ) => Promise<Record<string, unknown>>;
  invoke.mockImplementation(async (c, args) => {
    const r = await original(c, args);
    token = "foreign";
    return r;
  });
  await expect(a.acknowledgeNamedDestination(op, report().destination)).rejects.toThrow(
    "outcome unknown",
  );
  expect(disk.get("access_token")).toBe("token");
  token = "token";
  invoke.mockImplementation(original);
  await a.acknowledgeNamedDestination(other, report(other).destination);
});
it("provider, refresh, base, scope or launch changes refuse before review dispatch", async () => {
  for (const mode of ["provider", "refresh", "base", "scope", "launch"]) {
    setCurrentLaunchMode("local");
    setStorageMode("local");
    setSyncProvider("none");
    refresh = "refresh";
    base = "https://original.invalid";
    disk.set("starlane_server_cache_v1:session-scope", "original-scope");
    const a = prepareNativeArchiveOutcomeReview(binding, "/destination");
    await a.pending();
    const n = invoke.mock.calls.length;
    if (mode === "provider") setSyncProvider("google-drive");
    if (mode === "refresh") refresh = "foreign";
    if (mode === "base") base = "https://foreign.invalid";
    if (mode === "scope") disk.set("starlane_server_cache_v1:session-scope", "foreign");
    if (mode === "launch") setCurrentLaunchMode("server");
    await expect(a.acknowledgeNamedDestination(op, report().destination)).rejects.toThrow(
      "changed",
    );
    expect(invoke.mock.calls).toHaveLength(n);
  }
});
it("unavailable shell and malformed inventory refuse without export, serve or cleanup calls", async () => {
  native = false;
  expect(() => prepareNativeArchiveOutcomeReview(binding, "/destination")).toThrow("desktop local");
  native = true;
  const a = prepareNativeArchiveOutcomeReview(binding, "/destination");
  invoke.mockResolvedValue({
    format: 1,
    binding,
    parent_path: "/destination",
    inventory: {
      format: 1,
      operations: [report(), report()],
      automatic_retry: false,
      automatic_cleanup: false,
    },
  });
  await expect(a.pending()).rejects.toThrow("evidence invalid");
  expect(
    invoke.mock.calls.every(([name]) => name === "list_native_vault_archive_export_outcomes"),
  ).toBe(true);
});
it("malformed acknowledgment stays unknown and original reports remain available for inspection", async () => {
  const a = prepareNativeArchiveOutcomeReview(binding, "/destination");
  await a.pending();
  invoke.mockResolvedValue({ format: 1, retry_authority: true });
  await expect(a.acknowledgeNamedDestination(op, report().destination)).rejects.toThrow("unknown");
});

it("accepts actual native semantic binding with reordered JSON keys", async () => {
  const a = prepareNativeArchiveOutcomeReview(binding, "/destination");
  const original = invoke.getMockImplementation()! as (
    c: string,
    args: Record<string, unknown>,
  ) => Promise<Record<string, unknown>>;
  invoke.mockImplementation(async (c, args) => {
    const r = await original(c, args);
    r.binding = {
      root: binding.root,
      revision: binding.revision,
      context: Object.fromEntries(Object.entries(binding.context).reverse()),
    };
    return r;
  });
  await a.pending();
  await a.acknowledgeNamedDestination(op, report().destination);
});
