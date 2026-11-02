import { beforeEach, describe, expect, it, vi } from "vitest";

const exportManifest = vi.fn();
const createTransfer = vi.fn();
const apiFetch = vi.fn();
const portableRows = new Map<string, unknown>();
const listPortableTransfers = vi.fn(async () =>
  [...portableRows.values()].map((value) => structuredClone(value)),
);
const savePortableTransfer = vi.fn(async (record: { operation_id: string }) => {
  const retained = structuredClone(record);
  portableRows.set(record.operation_id, retained);
  return structuredClone(retained);
});

vi.mock("@/lib/api", () => ({ api: { fetch: apiFetch }, getApiBase: () => "/api", isTauri: () => false }));
vi.mock("@/lib/storageProviders", () => ({ getSyncProvider: () => "none" }));
vi.mock("@/lib/nativeArchiveCurrentContext", () => ({ captureNativeArchiveLocalSession: vi.fn() }));
vi.mock("@/lib/dataClient", () => ({
  dataClient: {
    exportQueuedSyncReplayManifest: exportManifest,
    createReviewedDraftTransfer: createTransfer,
    listPortableReviewedDraftTransfers: listPortableTransfers,
    savePortableReviewedDraftTransfer: savePortableTransfer,
  },
  publishedReplayFingerprint: async (value: unknown) => {
    const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify(value)));
    return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, "0")).join("");
  },
}));

function installIndexedDb() {
  const rows = new Map<string, unknown>();
  const names = { contains: () => true };
  const database = {
    objectStoreNames: names,
    createObjectStore: vi.fn(),
    close: vi.fn(),
    transaction: (_name: string, _mode: string) => {
      const transaction = {
        error: null,
        oncomplete: null as null | (() => void),
        onerror: null as null | (() => void),
        onabort: null as null | (() => void),
        objectStore: () => ({
          put: (record: { operation_id: string }) => { rows.set(record.operation_id, structuredClone(record)); queueMicrotask(() => transaction.oncomplete?.()); },
          get: (id: string) => {
            const request = { result: undefined as unknown, onsuccess: null as null | (() => void), onerror: null as null | (() => void), error: null };
            queueMicrotask(() => { request.result = structuredClone(rows.get(id)); request.onsuccess?.(); });
            return request;
          },
          getAll: () => {
            const request = { result: [] as unknown[], onsuccess: null as null | (() => void), onerror: null as null | (() => void), error: null };
            queueMicrotask(() => { request.result = [...rows.values()].map((value) => structuredClone(value)); request.onsuccess?.(); });
            return request;
          },
        }),
      };
      return transaction;
    },
  };
  vi.stubGlobal("indexedDB", { open: () => {
    const request = { result: database, onsuccess: null as null | (() => void), onerror: null as null | (() => void), onupgradeneeded: null as null | (() => void), error: null };
    queueMicrotask(() => request.onsuccess?.());
    return request;
  } });
  return rows;
}

describe("reviewed draft transfer retention", () => {
  let indexedRows: Map<string, unknown>;

  beforeEach(() => {
    vi.clearAllMocks();
    portableRows.clear();
    indexedRows = installIndexedDb();
    vi.stubGlobal("window", { location: { origin: "https://notes.example.test" } });
    vi.stubGlobal("crypto", { ...crypto, randomUUID: () => "11111111-1111-4111-8111-111111111111", subtle: crypto.subtle });
  });

  it("retains the exact hydrated manifest before dispatch and confirms a receipt binding", async () => {
    const payload = { draft_id: "draft-local", data_url: "data:text/plain;base64,SGVsbG8=", local_draft_source: { schema_version: 1, space_id: "local-space", document_id: "local-doc", draft_id: "draft-local", draft_created_at: "2026-10-19T20:00:00Z" } };
    exportManifest.mockResolvedValue({ schema_version: 7, exported_at: "now", provider: "none", replay_contract: {}, replay_readiness: {}, operation_count: 1, operation_summary: {}, recovery_checkpoint_count: 0, recovery_checkpoints: [], operations: [{ id: "syncop-1", entity_type: "asset", entity_id: "asset-1", action: "create", queued_at: "now", attempts: 0, payload_status: "available", payload }] });
    apiFetch.mockResolvedValue({ id: "account-1" });
    const module = await import("./reviewedDraftTransfer");
    const prepared = await module.prepareReviewedDraftTransfer({
      draft: { id: "draft-local", document_id: "local-doc", space_id: "local-space", owner_id: "local", title: "Exact", content: "![a](asset-1)", base_updated_at: "base", created_at: "2026-10-19T20:00:00Z", updated_at: "now" },
      rootIdentity: await module.currentLocalTransferRootIdentity(), serverAccountId: "account-1", serverSpaceId: "server-space", serverSpaceName: "Server", serverDocumentId: "server-doc", serverDocumentTitle: "Target", serverSource: { kind: "published_page", page_updated_at: "now", source_digest: "a".repeat(64) }, selectedOperationIds: ["syncop-1"],
    });
    expect(prepared.state).toBe("pending");
    expect(prepared.exact_manifest.operations[0].payload).toEqual(payload);
    createTransfer.mockResolvedValue({ schema_version: 1, operation_id: prepared.operation_id, request_fingerprint: "b".repeat(64), server_actor_id: "account-1", server_space_id: "server-space", server_document_id: "server-doc", target_draft_id: "server-draft", target_draft_created_at: "created", target_source_digest: "c".repeat(64), local_source: prepared.request.local_source, selected_operation_fingerprints: prepared.request.selected_operation_fingerprints, created_at: "created" });
    const confirmed = await module.confirmReviewedDraftTransfer(prepared);
    expect(confirmed.state).toBe("confirmed");
    expect(confirmed.binding?.bindings[0]).toMatchObject({ operation_id: "syncop-1", target_draft_id: "server-draft" });
    expect(createTransfer).toHaveBeenCalledWith("server-space", "server-doc", prepared.request);
  });

  it("restores the device journal from the portable vault copy without changing the original request", async () => {
    const module = await import("./reviewedDraftTransfer");
    const record = {
      schema_version: 1 as const,
      operation_id: "transfer-portable",
      state: "pending" as const,
      server_base: "/api",
      server_account_id: "account-1",
      server_space_id: "server-space",
      server_space_name: "Server",
      server_document_id: "server-doc",
      server_document_title: "Target",
      request: {
        schema_version: 1 as const,
        operation_id: "transfer-portable",
        local_source: { provider: "none" as const, root_identity: "root", space_id: "local-space", document_id: "local-doc", draft_id: "local-draft", draft_created_at: "created", snapshot_digest: "a".repeat(64) },
        server_source: { kind: "published_page" as const, page_updated_at: "server-updated", source_digest: "b".repeat(64) },
        reviewed_draft: { title: "Portable", icon: null, cover_image: null, content: "![asset](asset-1)" },
        selected_operation_fingerprints: ["c".repeat(64)],
      },
      exact_manifest: {
        schema_version: 7 as const,
        exported_at: "exported",
        provider: "none" as const,
        replay_contract: {},
        replay_readiness: {},
        operation_count: 1,
        operation_summary: {},
        recovery_checkpoint_count: 0,
        recovery_checkpoints: [],
        operations: [{ id: "syncop-1", entity_type: "asset", entity_id: "asset-1", action: "create", queued_at: "queued", attempts: 0, payload_status: "available" as const, payload: { data_url: "data:text/plain;base64,SGVsbG8=" } }],
      },
      prepared_at: "2026-10-19T20:00:00Z",
      updated_at: "2026-10-19T20:00:00Z",
    };
    portableRows.set(record.operation_id, structuredClone(record));

    const recovered = await module.readReviewedDraftTransfer(record.operation_id);
    const listed = await module.listReviewedDraftTransfers();

    expect(recovered).toEqual(record);
    expect(listed).toEqual([record]);
    expect(indexedRows.get(record.operation_id)).toEqual(record);
    expect(listed[0].exact_manifest.operations[0].payload).toEqual({ data_url: "data:text/plain;base64,SGVsbG8=" });
  });

  it("reconciles portable and device journals after native JSON key canonicalization", async () => {
    const module = await import("./reviewedDraftTransfer");
    const record = {
      schema_version: 1,
      operation_id: "transfer-canonicalized",
      state: "unknown",
      server_base: "/api",
      server_account_id: "account-1",
      server_space_id: "server-space",
      server_space_name: "Server",
      server_document_id: "server-doc",
      server_document_title: "Target",
      request: {
        schema_version: 1,
        operation_id: "transfer-canonicalized",
        local_source: { provider: "none", root_identity: "root", space_id: "space", document_id: "doc" },
      },
      exact_manifest: { schema_version: 7, operations: [] },
      prepared_at: "2026-10-20T20:00:00Z",
      updated_at: "2026-10-20T20:01:00Z",
    };
    indexedRows.set(record.operation_id, structuredClone(record));
    portableRows.set(record.operation_id, {
      ...structuredClone(record),
      request: {
        operation_id: record.operation_id,
        schema_version: 1,
        local_source: { document_id: "doc", space_id: "space", root_identity: "root", provider: "none" },
      },
    });

    await expect(module.listReviewedDraftTransfers()).resolves.toEqual([record]);
  });

  it("refuses conflicting portable and device copies of the original request", async () => {
    const module = await import("./reviewedDraftTransfer");
    const base = {
      schema_version: 1,
      operation_id: "transfer-conflict",
      state: "pending",
      server_base: "/api",
      server_account_id: "account-1",
      server_space_id: "server-space",
      server_space_name: "Server",
      server_document_id: "server-doc",
      server_document_title: "Target",
      request: { schema_version: 1, operation_id: "transfer-conflict" },
      exact_manifest: { schema_version: 7, operations: [] },
      prepared_at: "2026-10-19T20:00:00Z",
      updated_at: "2026-10-19T20:00:00Z",
    };
    indexedRows.set(base.operation_id, structuredClone(base));
    portableRows.set(base.operation_id, { ...structuredClone(base), request: { ...base.request, operation_id: "changed" } });

    await expect(module.readReviewedDraftTransfer(base.operation_id)).rejects.toThrow(
      "journals conflict with the original request",
    );
  });
});
