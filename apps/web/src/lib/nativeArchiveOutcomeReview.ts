import { api, getApiBase, isTauri } from "./api";
import { getCurrentLaunchMode } from "./launchMode";
import { getStorageSelection } from "./storageProviders";
export interface NativeArchiveReviewBinding {
  context: {
    version: 1;
    serverBase: string;
    accountId: string;
    sessionId: string;
    cacheScope: string;
    storageMode: "local";
    provider: "none" | "icloud-drive" | "google-drive";
  };
  revision: string;
  root: string;
}
export interface NativeArchiveOutcomeReport extends Record<string, unknown> {
  format: 1;
  operation: string;
  destination: string;
  state: string;
  journal_state: string;
  journal_sha256: string;
  original_binding_sha256: string;
  reviewed: boolean;
  automatic_retry: false;
  automatic_cleanup: false;
}
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
const canonical = (v: unknown) =>
  JSON.stringify(v, (_key, value: unknown) =>
    value && typeof value === "object" && !Array.isArray(value)
      ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)))
      : value,
  );
const equal = (a: unknown, b: unknown) => canonical(a) === canonical(b);
const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
const hash = /^[a-f0-9]{64}$/;
const states = new Set([
  "incomplete",
  "sealed_unpublished",
  "unresolved_output_absent",
  "published_output_missing",
  "published_verified",
  "conflict",
]);
function bounded(value: unknown, limit: number): void {
  if (new TextEncoder().encode(JSON.stringify(value)).length > limit)
    throw new Error("Native archive review response exceeds bound");
}
async function invoke(command: string, args: Record<string, unknown>): Promise<unknown> {
  const native = (
    window as unknown as {
      __TAURI_INTERNALS__?: { invoke: (c: string, a: Record<string, unknown>) => Promise<unknown> };
    }
  ).__TAURI_INTERNALS__;
  if (!native?.invoke)
    throw new Error("Native archive review unavailable; complete export remains disabled");
  return native.invoke(command, args);
}
/** Provenance is original archive evidence, not a new server authorization claim. */
export function prepareNativeArchiveOutcomeReview(
  binding: NativeArchiveReviewBinding,
  parentPath: string,
) {
  const original = clone(binding),
    parent = parentPath;
  if (
    !isTauri() ||
    getCurrentLaunchMode() !== "local" ||
    !parent.startsWith("/") ||
    parent === "/" ||
    parent.endsWith("/") ||
    parent
      .split("/")
      .slice(1)
      .some((p) => !p || p === "." || p === "..") ||
    !original ||
    Object.keys(original).length !== 3 ||
    !original.root.startsWith("/") ||
    !original.revision ||
    original.revision.length > 256 ||
    !original.context ||
    Object.keys(original.context).length !== 7 ||
    original.context.version !== 1 ||
    original.context.storageMode !== "local" ||
    !["none", "icloud-drive", "google-drive"].includes(original.context.provider) ||
    !["serverBase", "accountId", "sessionId", "cacheScope"].every(
      (k) =>
        typeof original.context[k as keyof typeof original.context] === "string" &&
        String(original.context[k as keyof typeof original.context]).length > 0,
    )
  )
    throw new Error("Original desktop local archive review required");
  bounded(original, 65536);
  const selection = JSON.stringify(getStorageSelection()),
    base = getApiBase(),
    token = api.getToken(),
    refresh = api.getRefreshToken(),
    diskToken = localStorage.getItem("access_token"),
    diskRefresh = localStorage.getItem("refresh_token"),
    scope = localStorage.getItem("starlane_server_cache_v1:session-scope");
  const verify = () => {
    const current = getStorageSelection();
    if (
      !isTauri() ||
      getCurrentLaunchMode() !== "local" ||
      current.mode !== "local" ||
      current.provider !== original.context.provider ||
      JSON.stringify(current) !== selection ||
      getApiBase() !== base ||
      api.getToken() !== token ||
      api.getRefreshToken() !== refresh ||
      localStorage.getItem("access_token") !== diskToken ||
      localStorage.getItem("refresh_token") !== diskRefresh ||
      localStorage.getItem("starlane_server_cache_v1:session-scope") !== scope
    )
      throw new Error("Original archive review session/workspace changed; retain native evidence");
  };
  verify();
  const reports = new Map<string, string>();
  const args = () => ({
    provider: original.context.provider,
    originalBinding: clone(original),
    parentPath: parent,
    current: clone(original.context),
  });
  return {
    async pending(): Promise<NativeArchiveOutcomeReport[]> {
      verify();
      const response = await invoke("list_native_vault_archive_export_outcomes", args());
      verify();
      bounded(response, 8 * 1024 * 1024);
      const r = response as {
        format?: unknown;
        binding?: unknown;
        parent_path?: unknown;
        inventory?: {
          format?: unknown;
          operations?: NativeArchiveOutcomeReport[];
          automatic_retry?: unknown;
          automatic_cleanup?: unknown;
        };
      };
      if (
        r?.format !== 1 ||
        !equal(r.binding, original) ||
        r.parent_path !== parent ||
        r.inventory?.format !== 1 ||
        r.inventory.automatic_retry !== false ||
        r.inventory.automatic_cleanup !== false ||
        !Array.isArray(r.inventory.operations) ||
        r.inventory.operations.length > 128
      )
        throw new Error("Original native archive inventory differs");
      const seen = new Set<string>();
      for (const report of r.inventory.operations) {
        bounded(report, 65536);
        if (
          report?.format !== 1 ||
          !uuid.test(report.operation) ||
          seen.has(report.operation) ||
          !report.destination.startsWith(parent + "/") ||
          report.destination.slice(parent.length + 1).includes("/") ||
          !states.has(report.state) ||
          !hash.test(report.journal_sha256) ||
          !hash.test(report.original_binding_sha256) ||
          typeof report.reviewed !== "boolean" ||
          report.automatic_retry !== false ||
          report.automatic_cleanup !== false
        )
          throw new Error("Original named archive evidence invalid");
        seen.add(report.operation);
      }
      verify();
      for (const report of r.inventory.operations)
        reports.set(report.operation, JSON.stringify(report));
      return clone(r.inventory.operations);
    },
    /** Call only from explicit review of this operation's exact named destination. */
    async acknowledgeNamedDestination(operation: string, destination: string): Promise<void> {
      verify();
      const raw = reports.get(operation);
      if (!raw || !uuid.test(operation))
        throw new Error("Inspect original native operation before recording review");
      const report = JSON.parse(raw) as NativeArchiveOutcomeReport;
      if (report.destination !== destination)
        throw new Error("Review must name the original archive destination");
      bounded(report, 65536);
      verify();
      try {
        const response = await invoke("acknowledge_native_vault_archive_export_review", {
          ...args(),
          operation,
          originalReport: clone(report),
        });
        verify();
        bounded(response, 65536);
        const r = response as {
          format?: unknown;
          binding?: unknown;
          parent_path?: unknown;
          operation?: unknown;
          result?: {
            format?: unknown;
            operation?: unknown;
            reviewed?: unknown;
            retry_authority?: unknown;
            cleanup_authority?: unknown;
          };
          retry_authority?: unknown;
          cleanup_authority?: unknown;
          serve_authority?: unknown;
        };
        if (
          r?.format !== 1 ||
          !equal(r.binding, original) ||
          r.parent_path !== parent ||
          r.operation !== operation ||
          r.result?.format !== 1 ||
          r.result.operation !== operation ||
          r.result.reviewed !== true ||
          r.result.retry_authority !== false ||
          r.result.cleanup_authority !== false ||
          r.retry_authority !== false ||
          r.cleanup_authority !== false ||
          r.serve_authority !== false
        )
          throw new Error("Original review acknowledgment differs");
      } catch {
        throw new Error(
          "Archive review acknowledgment outcome unknown; inspect the original named destination again. Native evidence retained",
        );
      }
    },
  };
}
