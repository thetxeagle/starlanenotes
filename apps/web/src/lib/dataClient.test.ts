import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createLocalVaultAdapter, runLocalVaultStartupCheck, type VaultSnapshotMutationBuilder } from "./localVault";
import { api, ApiError } from "./api";
import { cacheServerResponse, readCachedServerResponse, serverCacheScopeForSession, clearServerCacheSessionScope, listOfflineServerAttachments } from "./offlineServerCache";
import { __resetLocalDataForTests, dataClient, publishedReplayFingerprint } from "./dataClient";
import { setCurrentLaunchMode } from "./launchMode";
import {
  buildProviderRemoteImportApplyPlan,
  buildProviderRemoteImportDryRun,
  buildProviderRemoteMutationEnvelope,
  buildProviderRemoteMutationPlan,
  buildProviderRemoteSyncManifest,
} from "./providerAdapters";
import { writeProviderSession } from "./providerSession";
import { writeSharedSpaceDownloadCheckpoints } from "./sharedSpaceDownloads";
import { setSyncProvider } from "./storageProviders";
import { DEFAULT_NEW_DOCUMENT_CONTENT } from "./documentContent";
import { serializeTemplateMarker } from "./templateMarkers";
import type { ReviewedDraftTransferRecord } from "./reviewedDraftTransfer";

function identifierSessionToken(accountId: string): string {
  return `header.${btoa(JSON.stringify({ sub: accountId }))}.test-signature`;
}

function draftSaveSessionToken(accountId: string): string {
  return `e30.${btoa(JSON.stringify({ sub: accountId, sid: `draft-session-${accountId}`, token_type: "access", exp: 9999999999 }))}.fixture`;
}

function installNativeImportJournalTestDouble() {
  window.localStorage.setItem("starlane_storage_mode", "local");
  const stores = new Map<string, Map<string, unknown>>();
  const database = {
    close: vi.fn(),
    onversionchange: null as null | (() => void),
    objectStoreNames: { contains: (name: string) => stores.has(name) },
    createObjectStore(name: string) { stores.set(name, new Map()); },
    transaction(_names: string[]) {
      let finished = false;
      const tx = {
        oncomplete: null as null | (() => void),
        onabort: null as null | (() => void),
        onerror: null as null | (() => void),
        objectStore(name: string) {
          const rows = stores.get(name)!;
          return {
            add(value: { operationId: string }) { if (rows.has(value.operationId)) throw new Error("duplicate"); rows.set(value.operationId, structuredClone(value)); },
            put(value: { operationId: string }) { rows.set(value.operationId, structuredClone(value)); },
            delete(operationId: string) { rows.delete(operationId); },
            get(operationId: string) { return { result: structuredClone(rows.get(operationId)), onerror: null as null | (() => void) }; },
          };
        },
      };
      queueMicrotask(() => { if (!finished) { finished = true; tx.oncomplete?.(); } });
      return tx;
    },
  };
  vi.stubGlobal("indexedDB", {
    open: () => {
      const request = { result: database, onupgradeneeded: null as null | (() => void), onsuccess: null as null | (() => void), onerror: null, onblocked: null };
      queueMicrotask(() => { request.onupgradeneeded?.(); request.onsuccess?.(); });
      return request;
    },
  });
  const prototype = Object.getPrototypeOf(createLocalVaultAdapter("none")) as { admitArchiveCanonicalSnapshot?: (snapshot: unknown, verify: () => void) => Promise<void>; saveSnapshot(snapshot: unknown): Promise<void> };
  Object.defineProperty(prototype, "admitArchiveCanonicalSnapshot", { configurable: true, value: async function (this: typeof prototype, snapshot: unknown, verify: () => void) {
    verify();
    const current = JSON.parse(window.localStorage.getItem("starlane_local_vault_v1_none") ?? "{}") as { drafts?: Array<{ id: string; created_at?: string }> };
    const incoming = snapshot as { drafts?: Array<{ id: string; created_at?: string }> };
    if (incoming.drafts?.some(draft => current.drafts?.find(row => row.id === draft.id)?.created_at !== draft.created_at)) throw new Error("Canonical draft incarnation changed");
    await this.saveSnapshot(snapshot);
    verify();
  } });
  return Object.assign(() => { delete prototype.admitArchiveCanonicalSnapshot; }, { stores });
}

function nativePathImportProtocol(before: Awaited<ReturnType<typeof dataClient.exportLocalVaultSnapshot>>, pause?: Promise<void>) {
  let fullRequest: Record<string, unknown> | null = null;
  let committedResult: unknown = null;
  const sourceProof = { schema_version: 1, source_path: "/selected/native.txt", stamp: { device: 1, inode: 2, uid: 1000, mode: 33152, links: 1, length: 6, ctime: 1, ctime_nsec: 0, mtime: 1, mtime_nsec: 0 }, binary_sha256: "b".repeat(64) };
  return vi.fn(async (command: string, args: Record<string, unknown>) => {
    if (command === "describe_current_native_vault_archive") return { format: 1, binding: { context: args.context, root: "/vault", revision: "before" }, initialized: true, authority: "local_archive_provenance", export_authority: false, retry_authority: false, serve_authority: false };
    if (command === "inspect_original_native_local_import_path_source") return { name: "native.txt", mime_type: "text/plain", byte_length: 6, source_selection: sourceProof };
    if (command === "prepare_original_native_local_import_path") {
      fullRequest = { ...(args.request as Record<string, unknown>), path_source: sourceProof };
      return { format: 1, operation_id: fullRequest.operation_id, producer_token: "a".repeat(64), prepared: {}, request: fullRequest };
    }
    if (command === "run_original_native_local_import_path") {
      if (pause) await pause;
      if (!fullRequest) throw new Error("missing prepared request");
      const operationId = String(fullRequest.operation_id), createdAt = "2026-10-18T02:00:00Z";
      const vaultPath = `/vault/assets/local-imports/asset-import-${operationId}.bin`;
      const asset = { id: `asset-import-${operationId}`, name: "native.txt", mime_type: "text/plain", size: 6, url: `asset://localhost/${encodeURIComponent(vaultPath)}`, vault_path: vaultPath, source: "tauri", space_id: fullRequest.space_id, document_id: fullRequest.document_id, created_at: createdAt };
      const queued = { id: `syncop-import-${operationId}`, entity_type: "asset", entity_id: asset.id, action: "create", queued_at: createdAt, attempts: 0, payload: { ...asset, draft_id: fullRequest.draft_id, local_draft_source: { schema_version: 1, space_id: fullRequest.space_id, document_id: fullRequest.document_id, draft_id: fullRequest.draft_id, draft_created_at: fullRequest.draft_created_at } } };
      const snapshot = structuredClone(before); snapshot.assets.push(asset as never); snapshot.sync_state ??= { state: "idle", updated_at: createdAt, queued_operations: [] }; snapshot.sync_state.queued_operations.push(queued); snapshot.canonical_revision = "after"; snapshot.mirror_pending = true;
      const receipt = { format: 1, operation_id: operationId, request_sha256: "c".repeat(64), request: fullRequest, creator: {}, seal_json: JSON.stringify({ format: 1, operation_id: operationId, binary_sha256: sourceProof.binary_sha256, asset }), seal_file: {}, creator_directory_stamp: {}, canonical_revision: "after", canonical_committed: true, asset, queued_operation: queued };
      committedResult = { format: 1, operation_id: operationId, canonical_committed: true, asset, canonical_snapshot: snapshot, receipt_json: JSON.stringify(receipt) };
      return committedResult;
    }
    if (command === "inspect_original_native_local_import") {
      if (!committedResult) throw new Error("missing committed result");
      return structuredClone(committedResult);
    }
    if (command === "pack_native_replay_asset_payload") return "data:text/plain;base64,QQBCDQou";
    throw new Error(`Unsupported test shell command: ${command}`);
  });
}

function nativeBlobImportProtocol(before: Awaited<ReturnType<typeof dataClient.exportLocalVaultSnapshot>>) {
  let fullRequest: Record<string, unknown> | null = null;
  return vi.fn(async (command: string, args: Record<string, unknown>) => {
    if (command === "describe_current_native_vault_archive") return { format: 1, binding: { context: args.context, root: "/vault", revision: "before" }, initialized: true, authority: "local_archive_provenance", export_authority: false, retry_authority: false, serve_authority: false };
    if (command === "prepare_original_native_local_import") {
      fullRequest = structuredClone(args.request as Record<string, unknown>);
      return { format: 1, operation_id: fullRequest.operation_id, producer_token: "a".repeat(64) };
    }
    if (command === "append_original_native_local_import") {
      if (!fullRequest) throw new Error("missing prepared request");
      expect(args).toMatchObject({ producerToken: "a".repeat(64), index: 0, bytes: [65, 0, 66, 13, 10, 46] });
      return { format: 1, operation_id: fullRequest.operation_id, producer_token: "b".repeat(64), index: 0, received: 6 };
    }
    if (command === "finish_original_native_local_import") {
      if (!fullRequest) throw new Error("missing prepared request");
      expect(args.producerToken).toBe("b".repeat(64));
      const operationId = String(fullRequest.operation_id), createdAt = "2026-10-18T02:00:00Z";
      const vaultPath = `/vault/assets/local-imports/asset-import-${operationId}.bin`;
      const asset = { id: `asset-import-${operationId}`, name: fullRequest.name, mime_type: fullRequest.mime_type, size: fullRequest.byte_length, url: `asset://localhost/${encodeURIComponent(vaultPath)}`, vault_path: vaultPath, source: "tauri", space_id: fullRequest.space_id, document_id: fullRequest.document_id, created_at: createdAt };
      const queued = { id: `syncop-import-${operationId}`, entity_type: "asset", entity_id: asset.id, action: "create", queued_at: createdAt, attempts: 0, payload: { ...asset, draft_id: fullRequest.draft_id, local_draft_source: { schema_version: 1, space_id: fullRequest.space_id, document_id: fullRequest.document_id, draft_id: fullRequest.draft_id, draft_created_at: fullRequest.draft_created_at } } };
      const snapshot = structuredClone(before); snapshot.assets.push(asset as never); snapshot.sync_state ??= { state: "idle", updated_at: createdAt, queued_operations: [] }; snapshot.sync_state.queued_operations.push(queued); snapshot.canonical_revision = "after"; snapshot.mirror_pending = true;
      const receipt = { format: 1, operation_id: operationId, request_sha256: "c".repeat(64), request: fullRequest, creator: {}, seal_json: JSON.stringify({ format: 1, operation_id: operationId, binary_sha256: "d".repeat(64), asset }), seal_file: {}, creator_directory_stamp: {}, canonical_revision: "after", canonical_committed: true, asset, queued_operation: queued };
      return { format: 1, operation_id: operationId, canonical_committed: true, asset, canonical_snapshot: snapshot, receipt_json: JSON.stringify(receipt) };
    }
    if (command === "pack_native_replay_asset_payload") return "data:text/plain;base64,QQBCDQou";
    throw new Error(`Unsupported test shell command: ${command}`);
  });
}

describe("dataClient local mode", () => {
  const storage = new Map<string, string>();

  beforeEach(async () => {
    storage.clear();
    const localStorageMock = {
      get length() { return storage.size; },
      key: (index: number) => [...storage.keys()][index] ?? null,
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
      removeItem: (key: string) => {
        storage.delete(key);
      },
    };
    (globalThis as { window?: unknown }).window = {
      localStorage: localStorageMock,
    };
    (globalThis as { localStorage?: unknown }).localStorage = localStorageMock;
    setCurrentLaunchMode("local");
    setSyncProvider("none");
    await __resetLocalDataForTests();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    api.setToken(null);
    delete (globalThis as { window?: unknown }).window;
    delete (globalThis as { localStorage?: unknown }).localStorage;
    delete (globalThis as { fetch?: unknown }).fetch;
  });

  it("commits identifier with exact source and queue while retaining draft bodies and snapshot roundtrip", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Identifier", human_identifier: " ENG-1 ", content: "Exact private body" });
    const before = await dataClient.exportLocalVaultSnapshot();
    const source = before.documents.find((doc) => doc.id === page.id)!;
    const result = await dataClient.setDocumentIdentifier("local-space", page.id, " ENG-2 ");
    expect(result.disposition).toBe("applied");
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(after.drafts).toEqual(before.drafts);
    expect(after.documents.find((doc) => doc.id === page.id)).toMatchObject({ human_identifier: "ENG-2", content: source.content });
    const operation = after.sync_state!.queued_operations.find((op) => op.id === result.operation_id)!;
    expect(operation).toMatchObject({ action: "update_identifier", entity_id: page.id, payload: { id: page.id, space_id: "local-space", human_identifier: "ENG-2", source: { page_updated_at: source.updated_at, published_at: source.published_at ?? null, human_identifier: "ENG-1" } } });
    expect(await dataClient.getDocument("local-space", page.id)).toMatchObject({ human_identifier: "ENG-2" });
    const reopened = await createLocalVaultAdapter("none").loadSnapshot();
    expect(reopened.documents.find((doc) => doc.id === page.id)?.human_identifier).toBe("ENG-2");
    expect((await dataClient.searchDocuments("", 25, { identifier: "eng-2" })).map((doc) => doc.document_id)).toContain(page.id);
    expect((await dataClient.searchDocuments("", 25, { identifier: "eng-1" })).map((doc) => doc.document_id)).not.toContain(page.id);
  });

  it("preserves exact reviewed draft transfer evidence through portable vault export and import", async () => {
    const record = {
      schema_version: 1,
      operation_id: "transfer-portable-roundtrip",
      state: "pending",
      server_base: "/api",
      server_account_id: "account-1",
      server_space_id: "server-space",
      server_space_name: "Server",
      server_document_id: "server-doc",
      server_document_title: "Target",
      request: {
        schema_version: 1,
        operation_id: "transfer-portable-roundtrip",
        local_source: { provider: "none", root_identity: "root", space_id: "local-space", document_id: "local-doc", draft_id: "local-draft", draft_created_at: "created", snapshot_digest: "a".repeat(64) },
        server_source: { kind: "published_page", page_updated_at: "server-updated", source_digest: "b".repeat(64) },
        reviewed_draft: { title: "Portable", icon: null, cover_image: null, content: "![asset](asset-1)" },
        selected_operation_fingerprints: ["c".repeat(64)],
      },
      exact_manifest: {
        schema_version: 7,
        exported_at: "exported",
        provider: "none",
        replay_contract: {},
        replay_readiness: {},
        operation_count: 1,
        operation_summary: {},
        recovery_checkpoint_count: 0,
        recovery_checkpoints: [],
        operations: [{ id: "syncop-1", entity_type: "asset", entity_id: "asset-1", action: "create", queued_at: "queued", attempts: 0, payload_status: "available", payload: { data_url: "data:text/plain;base64,SGVsbG8=" } }],
      },
      prepared_at: "2026-10-19T20:00:00Z",
      updated_at: "2026-10-19T20:00:00Z",
    } as unknown as ReviewedDraftTransferRecord;

    await dataClient.savePortableReviewedDraftTransfer(record);
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.reviewed_draft_transfer_records).toEqual([record]);

    await dataClient.importLocalVaultSnapshot(structuredClone(exported));

    expect(await dataClient.listPortableReviewedDraftTransfers()).toEqual([record]);
    expect((await dataClient.exportLocalVaultSnapshot()).reviewed_draft_transfer_records?.[0].exact_manifest.operations[0].payload).toEqual({ data_url: "data:text/plain;base64,SGVsbG8=" });
  });

  it("accepts a reviewed draft transfer receipt after native JSON key canonicalization", async () => {
    const request = {
      schema_version: 1 as const,
      operation_id: "11111111-1111-4111-8111-111111111111",
      local_source: {
        document_id: "local-document",
        draft_created_at: "2026-10-20T09:51:13.518Z",
        draft_id: "local-draft",
        provider: "none",
        root_identity: "native-root:" + "a".repeat(64),
        snapshot_digest: "b".repeat(64),
        space_id: "local-space",
      },
      server_source: { kind: "draft" as const, draft_id: "source-draft", draft_created_at: "source-created", source_digest: "c".repeat(64) },
      reviewed_draft: { title: "Transferred", icon: null, cover_image: null, content: "Exact reviewed bytes" },
      selected_operation_fingerprints: ["d".repeat(64)],
    };
    const receipt = {
      schema_version: 1 as const,
      operation_id: request.operation_id,
      request_fingerprint: "e".repeat(64),
      server_actor_id: "server-actor",
      server_space_id: "server-space",
      server_document_id: "server-document",
      target_draft_id: "target-draft",
      target_draft_created_at: "target-created",
      target_source_digest: "f".repeat(64),
      local_source: {
        provider: "none",
        root_identity: "native-root:" + "a".repeat(64),
        space_id: "local-space",
        document_id: "local-document",
        draft_id: "local-draft",
        draft_created_at: "2026-10-20T09:51:13.518Z",
        snapshot_digest: "b".repeat(64),
      },
      selected_operation_fingerprints: ["d".repeat(64)],
      created_at: "receipt-created",
    };
    vi.spyOn(api, "fetch").mockResolvedValue(receipt);

    await expect(dataClient.createReviewedDraftTransfer("server-space", "server-document", request)).resolves.toEqual(receipt);
  });

  it("rejects case-insensitive reserved archive/trash identifiers and failed persistence without publishing caches or queue", async () => {
    const reserved = await dataClient.createDocument("local-space", { title: "Reserved", human_identifier: "ENG-1" });
    await dataClient.archiveDocument("local-space", reserved.id);
    const page = await dataClient.createDocument("local-space", { title: "Target" });
    const before = await dataClient.exportLocalVaultSnapshot();
    await expect(dataClient.setDocumentIdentifier("local-space", page.id, "eng-1")).rejects.toThrow("already reserved");
    expect(await dataClient.exportLocalVaultSnapshot()).toMatchObject({ documents: before.documents, drafts: before.drafts, sync_state: { queued_operations: before.sync_state!.queued_operations } });
    const original = window.localStorage.setItem;
    window.localStorage.setItem = () => { throw new Error("QuotaExceededError"); };
    try {
      await expect(dataClient.setDocumentIdentifier("local-space", page.id, "ENG-2")).rejects.toThrow("QuotaExceededError");
      expect((await dataClient.getDocument("local-space", page.id)).human_identifier).toBeUndefined();
    } finally { window.localStorage.setItem = original; }
    expect(await dataClient.exportLocalVaultSnapshot()).toMatchObject({ documents: before.documents, drafts: before.drafts, sync_state: { queued_operations: before.sync_state!.queued_operations } });
  });


  it("reports identifier edit capability from canonical locked and archived local state", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Capability" });
    expect((await dataClient.getDocument("local-space", page.id)).identifier_edit_allowed).toBe(true);
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    snapshot.documents.find((doc) => doc.id === page.id)!.locked = true;
    await dataClient.importLocalVaultSnapshot(snapshot);
    expect((await dataClient.getDocument("local-space", page.id)).identifier_edit_allowed).toBe(false);
    snapshot.documents.find((doc) => doc.id === page.id)!.locked = false;
    snapshot.documents.find((doc) => doc.id === page.id)!.archived_at = "2026-10-08T00:00:00Z";
    await dataClient.importLocalVaultSnapshot(snapshot);
    expect((await dataClient.getDocument("local-space", page.id)).identifier_edit_allowed).toBe(false);
    await expect(dataClient.setDocumentIdentifier("local-space", page.id, "ENG-1")).rejects.toThrow(/unavailable/);
  });

  it("rejects conflicting full snapshot replacement before any durable mutation", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Original", human_identifier: "ENG-1" });
    const before = await dataClient.exportLocalVaultSnapshot();
    const incoming = structuredClone(before);
    incoming.documents.push({ ...incoming.documents.find((doc) => doc.id === page.id)!, id: "duplicate", human_identifier: "eng-1", deleted_at: "2026-10-08T00:00:00Z" });
    await expect(dataClient.importLocalVaultSnapshot(incoming)).rejects.toThrow("already reserved");
    expect(await dataClient.exportLocalVaultSnapshot()).toMatchObject({ documents: before.documents, drafts: before.drafts, sync_state: { queued_operations: before.sync_state!.queued_operations } });
  });

  it.each(["before-callback", "during-ack"])("retains unrelated cached body/base during identifier commit (%s)", async (boundary) => {
    const page = await dataClient.createDocument("local-space", { title: "Identifier", content: "Initial" });
    const draft = await dataClient.getDocumentDraft("local-space", page.id, page.draft_id!);
    let release!: () => void, reached!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const started = new Promise<void>((resolve) => { reached = resolve; });
    const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(createLocalVaultAdapter("none"));
    if (boundary === "before-callback") {
      const original = prototype.supportsAtomicSnapshotCommit;
      vi.spyOn(prototype, "supportsAtomicSnapshotCommit").mockImplementationOnce(async function (this: ReturnType<typeof createLocalVaultAdapter>) {
        const supported = await original.call(this); reached(); await gate; return supported;
      });
    } else {
      const original = prototype.commitSnapshotMutation;
      vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (this: ReturnType<typeof createLocalVaultAdapter>, mutate: VaultSnapshotMutationBuilder) {
        const saved = await original.call(this, mutate); reached(); await gate; return saved;
      });
    }
    const identifier = dataClient.setDocumentIdentifier("local-space", page.id, "ENG-1");
    await started;
    const body = dataClient.updateDocumentContent("local-space", page.id, "Concurrent body");
    const staged = { content: "Concurrent draft", base_updated_at: "Concurrent base" };
    const draftBody = dataClient.updateDocumentDraft("local-space", page.id, draft.id, staged);
    try {
      await vi.waitFor(async () => {
        expect((await dataClient.getDocumentContent("local-space", page.id)).content).toContain("Concurrent body");
        expect(await dataClient.getDocumentDraft("local-space", page.id, draft.id)).toMatchObject({ content: "Concurrent draft", base_updated_at: "Concurrent base" });
      });
    } finally { release(); }
    await Promise.all([identifier, body, draftBody]);
    const saved = await createLocalVaultAdapter("none").loadSnapshot();
    expect(saved.documents.find((doc) => doc.id === page.id)).toMatchObject({ human_identifier: "ENG-1", content: "Concurrent body" });
    expect(saved.drafts?.find((item) => item.id === draft.id)).toMatchObject({ content: "Concurrent draft", base_updated_at: "Concurrent base" });
    expect(saved.sync_state!.queued_operations.some((op) => op.action === "update_identifier")).toBe(true);
    expect(saved.sync_state!.queued_operations.some((op) => op.action === "update_content")).toBe(true);
  });

  it("retains server offline identifier and exact REST retry identity without fabricating a body or republishing historical state", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Downloaded", content: "Actual saved body" });
    const before = await dataClient.exportLocalVaultSnapshot();
    const original = before.documents.find((doc) => doc.id === page.id)!;
    const source = { page_updated_at: original.updated_at, published_at: original.published_at ?? null, human_identifier: original.human_identifier ?? null };
    setCurrentLaunchMode("server"); api.setToken(identifierSessionToken("account-a"));
    let lost = true;
    let accountId = "account-a";
    const bodies: unknown[] = [];
    vi.spyOn(api, "fetch").mockImplementation(async (path, options) => {
      if (path === "/auth/me") return { id: accountId };
      if (path === `/spaces/local-space/documents/${page.id}`) return { ...original };
      if (path === `/spaces/local-space/documents/${page.id}/identifier`) {
        bodies.push(options?.body);
        if (lost) throw new Error("Lost acknowledgement");
        return { operation_id: (options!.body as { operation_id: string }).operation_id, human_identifier: "ENG-1", page_updated_at: "2026-10-08T12:00:00Z", published_at: null, historical: true };
      }
      throw new Error(`Unexpected request ${path}`);
    });
    const first = await dataClient.setDocumentIdentifier("local-space", page.id, "ENG-1", source);
    expect(first.disposition).toBe("queued");
    const pending = await createLocalVaultAdapter("none").loadSnapshot();
    const queued = pending.sync_state!.queued_operations.find((op) => op.id === first.operation_id)!;
    expect(queued).toMatchObject({ replay_provider: "none", server_context: { account_id: "account-a", server_base: "/api/v1" }, payload: { source } });
    expect(pending.documents.find((doc) => doc.id === page.id)).toEqual(original);
    expect(pending.drafts).toEqual(before.drafts);
    expect(await dataClient.getPendingDocumentIdentifier("local-space", page.id)).toMatchObject({ operation_id: first.operation_id, human_identifier: "ENG-1", source });
    // Native serde JSON sorting must not create a new retry operation or drop its binding.
    const canonical = createLocalVaultAdapter("none");
    await canonical.commitSnapshotMutation((snapshot) => {
      const operation = snapshot.sync_state!.queued_operations.find((item) => item.id === first.operation_id)!;
      operation.payload = { source: { human_identifier: source.human_identifier, published_at: source.published_at, page_updated_at: source.page_updated_at }, human_identifier: "ENG-1", space_id: "local-space", id: page.id };
      operation.server_context = { session_scope: operation.server_context!.session_scope, account_id: "account-a", server_base: "/api/v1" };
      return { snapshot, result: undefined };
    });
    accountId = "different-account"; api.setToken(identifierSessionToken(accountId));
    expect(await dataClient.getPendingDocumentIdentifier("local-space", page.id)).toBeNull();
    setCurrentLaunchMode("local");
    await expect(dataClient.exportQueuedSyncReplayManifest([first.operation_id])).rejects.toThrow("different server or account");
    accountId = "account-a"; api.setToken(identifierSessionToken(accountId)); setCurrentLaunchMode("server");
    lost = false;
    const second = await dataClient.setDocumentIdentifier("local-space", page.id, "ENG-1", source);
    expect(second).toEqual({ disposition: "historical", operation_id: first.operation_id });
    expect(bodies[1]).toEqual(bodies[0]);
    const after = await createLocalVaultAdapter("none").loadSnapshot();
    expect(after.sync_state!.queued_operations.some((op) => op.id === first.operation_id)).toBe(false);
    expect(await dataClient.getPendingDocumentIdentifier("local-space", page.id)).toBeNull();
    expect(after.documents.find((doc) => doc.id === page.id)?.updated_at).toBe(original.updated_at);
  });

  it("applies dedicated remote identifier without echo and retains stale/malformed operations", async () => {
    await dataClient.clearQueuedSyncOperations();
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    const source = snapshot.documents.find((doc) => doc.id === "local-welcome")!;
    const envelope = buildProviderRemoteMutationEnvelope(buildProviderRemoteMutationPlan("google-drive", "Google Drive", "push", { status: "enabled", root_strategy: "drive-appdata-or-user-selected-folder", auth_model: "oauth2-offline-access", required_scopes: ["drive.file"], write_model: "local-sqlite-first-then-provider-api-push", conflict_model: "provider-revision-plus-local-replay-queue-validation" }, { provider: "google-drive", connected: true, account_label: "Google account", scopes: ["drive.file"], sync_mode: "manual" }), "2026-10-08T12:00:00Z", "identifier-envelope");
    const payload = { id: source.id, space_id: source.space_id, human_identifier: "ENG-1", source: { page_updated_at: source.updated_at, published_at: source.published_at ?? null, human_identifier: source.human_identifier ?? null } };
    const manifest = (id: string, payload: unknown) => buildProviderRemoteSyncManifest(envelope, { schema_version: 1, operation_count: 1, operations: [{ id, entity_type: "document", entity_id: source.id, action: "update_identifier", payload_status: "available", payload }] });
    expect((await dataClient.applyProviderRemoteImportManifest(manifest("identifier-import", payload))).applied_operation_count).toBe(1);
    const saved = await dataClient.exportLocalVaultSnapshot();
    expect(saved.documents.find((doc) => doc.id === source.id)?.human_identifier).toBe("ENG-1");
    expect(saved.drafts).toEqual(snapshot.drafts);
    expect(await dataClient.listQueuedSyncOperations()).toEqual([]);
    for (const bad of [{ ...payload, human_identifier: "ENG-2" }, { ...payload, human_identifier: 42 }, { ...payload, source: { ...payload.source, published_at: undefined } }]) {
      expect((await dataClient.applyProviderRemoteImportManifest(manifest("identifier-bad", bad))).applied_operation_count).toBe(0);
      expect((await dataClient.exportLocalVaultSnapshot()).documents).toEqual(saved.documents);
      expect(await dataClient.listQueuedSyncOperations()).toEqual([]);
    }
  });

  it("retains original identifier provider rather than relabeling a queued operation", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Provider" });
    const queued = await dataClient.setDocumentIdentifier("local-space", page.id, "ENG-1");
    const before = await createLocalVaultAdapter("none").loadSnapshot();
    setSyncProvider("google-drive");
    await expect(dataClient.exportQueuedSyncReplayManifest([queued.operation_id])).rejects.toThrow("different sync provider");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
  });

  it.each(["identifier", "publication", "unchanged"])("keeps foreign downloaded rows unchanged and only projects exact confirmed cache source (%s)", async (drift) => {
    const page = await dataClient.createDocument("local-space", { title: "Foreign download", human_identifier: "ENG-OLD" });
    const original = (await dataClient.exportLocalVaultSnapshot()).documents.find((doc) => doc.id === page.id)!;
    setCurrentLaunchMode("server"); api.setToken(identifierSessionToken("different-account"));
    const path = `/spaces/local-space/documents/${page.id}`;
    const scope = serverCacheScopeForSession(api.getToken(), api.getRefreshToken());
    cacheServerResponse("/api/v1", path, original, scope);
    const source = { page_updated_at: original.updated_at, published_at: original.published_at ?? null, human_identifier: "ENG-OLD" };
    let reject = true;
    vi.spyOn(api, "fetch").mockImplementation(async (request, options) => {
      if (request === "/auth/me") return { id: "different-account" };
      if (request === path) return { ...original };
      if (request === `${path}/identifier`) {
        if (reject) throw new ApiError("Identifier is reserved", 409);
        // A concurrent cache response may retain the timestamp while changing publication or identifier.
        cacheServerResponse("/api/v1", path, { ...original, ...(drift === "identifier" ? { human_identifier: "ENG-NEWER" } : drift === "publication" ? { published_at: "2026-10-08T10:00:00Z" } : {}) }, scope);
        return { operation_id: (options!.body as { operation_id: string }).operation_id, human_identifier: "ENG-NEXT", page_updated_at: "2026-10-08T12:00:00Z", published_at: null, historical: false };
      }
      throw new Error(request);
    });
    const pending = await dataClient.setDocumentIdentifier("local-space", page.id, "ENG-NEXT", source);
    expect(pending).toMatchObject({ disposition: "queued", pending_error: "Identifier is reserved" });
    expect(readCachedServerResponse("/api/v1", path, scope)).toEqual(original);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents.find((doc) => doc.id === page.id)).toEqual(original);
    reject = false;
    expect((await dataClient.setDocumentIdentifier("local-space", page.id, "ENG-NEXT", source)).disposition).toBe("applied");
    expect(readCachedServerResponse<{ human_identifier: string; published_at: string | null }>("/api/v1", path, scope)?.human_identifier).toBe(drift === "identifier" ? "ENG-NEWER" : drift === "publication" ? "ENG-OLD" : "ENG-NEXT");
    if (drift === "publication") expect(readCachedServerResponse<{ published_at: string }>("/api/v1", path, scope)?.published_at).toBe("2026-10-08T10:00:00Z");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents.find((doc) => doc.id === page.id)).toEqual(original);
  });

  it("rejects account changes during initial identity lookup before any identifier queue or PATCH", async () => {
    const before = await dataClient.exportLocalVaultSnapshot();
    const sourcePage = before.documents.find((page) => page.id === "local-welcome")!;
    const source = { page_updated_at: sourcePage.updated_at, published_at: sourcePage.published_at ?? null, human_identifier: null };
    setCurrentLaunchMode("server"); api.setToken(identifierSessionToken("account-a"));
    const fetch = vi.spyOn(api, "fetch").mockImplementation(async (path) => {
      if (path !== "/auth/me") throw new Error("Page/PATCH must not run after account changes");
      api.setToken(identifierSessionToken("account-b"));
      return { id: "account-a" };
    });
    await expect(dataClient.setDocumentIdentifier("local-space", "local-welcome", "ENG-1", source)).rejects.toThrow("account changed");
    expect(fetch).toHaveBeenCalledTimes(1);
    const after = await createLocalVaultAdapter("none").loadSnapshot();
    expect(after.documents).toEqual(before.documents);
    expect(after.drafts).toEqual(before.drafts);
    expect(after.sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
  });

  it("rejects a preexisting different credential with cached old account identity before queuing or PATCH", async () => {
    const before = await dataClient.exportLocalVaultSnapshot();
    const page = before.documents.find((doc) => doc.id === "local-welcome")!;
    setCurrentLaunchMode("server"); api.setToken(identifierSessionToken("account-b"));
    const fetch = vi.spyOn(api, "fetch").mockImplementation(async (path) => {
      if (path !== "/auth/me") throw new Error("No page lookup or mutation may follow mismatched identity");
      return { id: "cached-account-a" };
    });
    await expect(dataClient.setDocumentIdentifier("local-space", page.id, "ENG-1", { page_updated_at: page.updated_at, published_at: page.published_at ?? null, human_identifier: null })).rejects.toThrow("Current server account");
    await expect(dataClient.getPendingDocumentIdentifier("local-space", page.id)).rejects.toThrow("account changed");
    expect(fetch).toHaveBeenCalledTimes(2);
    const after = await createLocalVaultAdapter("none").loadSnapshot();
    expect(after.documents).toEqual(before.documents);
    expect(after.sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
  });

  it("queues explicit placement mask/source and null clears atomically for published and private subtrees", async () => {
    for (const privatePage of [false, true]) {
      const folder = await dataClient.createFolder("local-space", `Move ${privatePage}`);
      const page = await dataClient.createDocument("local-space", { title: "Root", folder_id: folder.id });
      const child = await dataClient.createDocument("local-space", { title: "Child", folder_id: folder.id, parent_id: page.id });
      if (!privatePage) {
        const draft = await dataClient.getDocumentDraft("local-space", page.id, page.draft_id!);
        await dataClient.publishDocumentDraft("local-space", page.id, draft.id);
      }
      const before = await dataClient.exportLocalVaultSnapshot();
      const source = before.documents.find((doc) => doc.id === page.id)!;
      await dataClient.moveDocument("local-space", page.id, { target_space_id: "local-space", folder_id: null, parent_id: null });
      const saved = await dataClient.exportLocalVaultSnapshot();
      const operation = (await dataClient.listQueuedSyncOperations()).filter((op) => op.entity_id === page.id && op.action === "update").slice(-1)[0]!;
      expect(operation.payload).toMatchObject({ folder_id: null, parent_id: null, metadata_intent: { version: 1, fields: ["folder_id", "parent_id"], source: { page_updated_at: source.updated_at, published_at: source.published_at ?? null } } });
      expect(operation.payload).not.toHaveProperty("title");expect(operation.payload).not.toHaveProperty("draft_id");
      expect(saved.documents.find((doc) => doc.id === page.id)?.folder_id).toBeUndefined();
      expect(saved.documents.find((doc) => doc.id === child.id)).toMatchObject({ parent_id: page.id });
      expect(saved.documents.find((doc) => doc.id === child.id)?.folder_id).toBeUndefined();
      expect(saved.drafts).toEqual(before.drafts);
      const reopened = await createLocalVaultAdapter("none").loadSnapshot();
      expect(reopened.sync_state?.queued_operations.find((op) => op.id === operation.id)?.payload).toEqual(operation.payload);
    }
  });

  it("preserves metadata/draft/cache/queue on failed atomic persistence and captures exact caller draft context", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Private" });
    const draft = await dataClient.getDocumentDraft("local-space", page.id, page.draft_id!);
    const before = await dataClient.exportLocalVaultSnapshot();
    const disk = new Map(storage);
    const original = window.localStorage.setItem;
    window.localStorage.setItem = () => { throw new Error("QuotaExceededError"); };
    try {
      await expect(dataClient.updateDocument("local-space", page.id, { title: "Failed", icon: "" }, { id: draft.id, updated_at: draft.updated_at, base_updated_at: draft.base_updated_at })).rejects.toThrow("QuotaExceededError");
      expect(await dataClient.getDocument("local-space", page.id)).toMatchObject({ title: "Private" });
      expect(await dataClient.getDocumentDraft("local-space", page.id, draft.id)).toMatchObject({ title: "Private" });
      expect(storage).toEqual(disk);
    } finally { window.localStorage.setItem = original; }
    const afterFailure = await dataClient.exportLocalVaultSnapshot();
    expect({ ...afterFailure, exported_at: undefined, sync_state: undefined }).toEqual({ ...before, exported_at: undefined, sync_state: undefined });
    expect(await dataClient.listQueuedSyncOperations()).toEqual(before.sync_state?.queued_operations);
    vi.useFakeTimers();
    vi.setSystemTime(new Date(draft.updated_at).getTime() + 1000);
    try {
    await dataClient.updateDocument("local-space", page.id, { title: "Saved", icon: "" }, { id: draft.id, updated_at: draft.updated_at, base_updated_at: draft.base_updated_at });
    } finally { vi.useRealTimers(); }
    const op = (await dataClient.listQueuedSyncOperations()).filter((op) => op.action === "update").slice(-1)[0]!;
    expect(op.payload).toMatchObject({ title: "Saved", icon: null, metadata_intent: { fields: ["icon", "title"], draft: { mode: "exact", id: draft.id, updated_at: draft.updated_at, base_updated_at: draft.base_updated_at } } });
    await expect(dataClient.updateDocument("local-space", page.id, { title: "Wrong" }, { id: draft.id, updated_at: draft.updated_at, base_updated_at: draft.base_updated_at })).rejects.toThrow("Selected draft changed");
  });

  it.each(["before-callback", "during-ack"])("retains staged document and draft body/base while metadata commits (%s)", async (boundary) => {
    const page = await dataClient.createDocument("local-space", { title: "Before", content: "Initial document" });
    const draft = await dataClient.getDocumentDraft("local-space", page.id, page.draft_id!);
    let release!: () => void, reached!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const started = new Promise<void>((resolve) => { reached = resolve; });
    const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(createLocalVaultAdapter("none"));
    if (boundary === "before-callback") {
      const original = prototype.supportsAtomicSnapshotCommit;
      vi.spyOn(prototype, "supportsAtomicSnapshotCommit").mockImplementationOnce(async function (this: ReturnType<typeof createLocalVaultAdapter>) {
        const supported = await original.call(this); reached(); await gate; return supported;
      });
    } else {
      const original = prototype.commitSnapshotMutation;
      vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (this: ReturnType<typeof createLocalVaultAdapter>, mutate: VaultSnapshotMutationBuilder) {
        const saved = await original.call(this, mutate); reached(); await gate; return saved;
      });
    }
    const metadata = dataClient.updateDocument("local-space", page.id, { title: "Metadata title" });
    await started;
    const body = dataClient.updateDocumentContent("local-space", page.id, "Concurrent document body");
    // Existing cache-first draft writer accepts the staged record fields. Exercise
    // base preservation without changing its API/runtime in this metadata slice.
    const staged = { content: "Concurrent draft body", base_updated_at: "Concurrent base" };
    const draftBody = dataClient.updateDocumentDraft("local-space", page.id, draft.id, staged);
    try {
      await vi.waitFor(async () => {
        expect((await dataClient.getDocumentContent("local-space", page.id)).content).toContain("Concurrent document body");
        expect(await dataClient.getDocumentDraft("local-space", page.id, draft.id)).toMatchObject(staged);
      });
    } finally { release(); }
    await Promise.all([metadata, body, draftBody]);
    const saved = await createLocalVaultAdapter("none").loadSnapshot();
    expect(saved.documents.find((item) => item.id === page.id)).toMatchObject({ title: "Metadata title", content: "Concurrent document body" });
    expect(saved.drafts?.find((item) => item.id === draft.id)).toMatchObject({ title: "Metadata title", ...staged });
    const queue = saved.sync_state?.queued_operations ?? [];
    expect(queue.some((op) => op.entity_id === page.id && op.action === "update_content")).toBe(true);
    expect(queue.some((op) => op.entity_id === page.id && op.action === "update" && (op.payload as { title?: string }).title === "Metadata title")).toBe(true);
    expect(await dataClient.getDocumentDraft("local-space", page.id, draft.id)).toMatchObject({ title: "Metadata title", ...staged });
  });

  it("uses latest committed source under serialization and retains independent queued metadata edits", async () => {
    const folder = await dataClient.createFolder("local-space", "Destination");
    const before = await dataClient.exportLocalVaultSnapshot();
    const move = dataClient.moveDocument("local-space", "local-welcome", { target_space_id: "local-space", folder_id: folder.id, parent_id: null });
    const rename = dataClient.updateDocument("local-space", "local-welcome", { title: "Concurrent" });
    await Promise.all([move, rename]);
    const operations = (await dataClient.listQueuedSyncOperations()).filter((op) => op.entity_id === "local-welcome" && op.action === "update");
    const first = operations[0].payload as { updated_at: string; metadata_intent: { source: { page_updated_at: string } } };
    const second = operations[1].payload as { metadata_intent: { source: { page_updated_at: string } } };
    expect(first.metadata_intent.source.page_updated_at).toBe(before.documents.find((doc) => doc.id === "local-welcome")!.updated_at);
    expect(second.metadata_intent.source.page_updated_at).toBe(first.updated_at);
    expect((await dataClient.getDocument("local-space", "local-welcome"))).toMatchObject({ title: "Concurrent", folder_id: folder.id });
  });
  it("applies explicit remote placement without queue echo and retains stale/exact-draft operations unchanged", async () => {
    await dataClient.clearQueuedSyncOperations();
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    const source = snapshot.documents.find((doc) => doc.id === "local-welcome")!;
    const envelope = buildProviderRemoteMutationEnvelope(buildProviderRemoteMutationPlan("google-drive", "Google Drive", "push", { status: "enabled", root_strategy: "drive-appdata-or-user-selected-folder", auth_model: "oauth2-offline-access", required_scopes: ["drive.file"], write_model: "local-sqlite-first-then-provider-api-push", conflict_model: "provider-revision-plus-local-replay-queue-validation" }, { provider: "google-drive", connected: true, account_label: "Google account", scopes: ["drive.file"], sync_mode: "manual" }), "2026-10-08T12:00:00Z", "metadata-remote-envelope");
    const payload: Record<string, unknown> = { id: source.id, space_id: source.space_id, folder_id: null, parent_id: null, updated_at: "2026-10-08T12:00:00Z", metadata_intent: { version: 1, fields: ["folder_id", "parent_id"], source: { page_updated_at: source.updated_at, published_at: source.published_at ?? null } } };
    const manifest = (id: string, payload: Record<string, unknown>) => buildProviderRemoteSyncManifest(envelope, { schema_version: 1, operation_count: 1, operations: [{ id, entity_type: "document", entity_id: source.id, action: "update", payload_status: "available", payload }] });
    const first = await dataClient.applyProviderRemoteImportManifest(manifest("metadata-first", payload));
    expect(first.applied_operation_count).toBe(1);
    expect(await dataClient.listQueuedSyncOperations()).toEqual([]);
    const before = await dataClient.exportLocalVaultSnapshot();
    const bad = { ...payload, title: "Must not save", metadata_intent: { version: 1, fields: ["title"], source: { page_updated_at: "1900-01-01T00:00:00Z", published_at: source.published_at ?? null }, draft: { mode: "exact", id: "missing", updated_at: "old", base_updated_at: "old" } } };
    const failed = await dataClient.applyProviderRemoteImportManifest(manifest("metadata-stale", bad));
    expect(failed.applied_operation_count).toBe(0);
    expect((await dataClient.exportLocalVaultSnapshot()).documents).toEqual(before.documents);
    expect(await dataClient.listQueuedSyncOperations()).toEqual([]);
    const selectedMissing = { ...bad, metadata_intent: { ...(bad.metadata_intent as object), source: { page_updated_at: before.documents.find((doc) => doc.id === source.id)!.updated_at, published_at: source.published_at ?? null } } };
    const missing = await dataClient.applyProviderRemoteImportManifest(manifest("metadata-missing-draft", selectedMissing));
    expect(missing.applied_operation_count).toBe(0);
    expect(missing.results[0].message).toContain("Selected draft");
    expect((await dataClient.exportLocalVaultSnapshot()).documents).toEqual(before.documents);
  });

  it("keeps property values, schemas, replay queue and disk unchanged after persistence failure", async () => {
    const schema = await dataClient.createPropertySchema("local-space", {
      name: "Durable",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: schema.id, value: "Before" },
    ]);
    const before = await dataClient.exportLocalVaultSnapshot();
    const queued = await dataClient.listQueuedSyncOperations();
    const disk = new Map(storage);
    const storageMock = window.localStorage;
    const setItem = storageMock.setItem;
    storageMock.setItem = () => {
      throw new Error("QuotaExceededError");
    };
    try {
      await expect(
        dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
          { property_id: schema.id, value: "Failed" },
        ]),
      ).rejects.toThrow("QuotaExceededError");
      await expect(
        dataClient.updatePropertySchema("local-space", schema.id, {
          name: "Failed rename",
          property_type: "text",
        }),
      ).rejects.toThrow("QuotaExceededError");
      await expect(
        dataClient.createPropertySchema("local-space", {
          name: "Failed creation",
          property_type: "text",
        }),
      ).rejects.toThrow("QuotaExceededError");
      await expect(dataClient.deletePropertySchema("local-space", schema.id)).rejects.toThrow(
        "QuotaExceededError",
      );
    } finally {
      storageMock.setItem = setItem;
    }
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(after.property_values).toEqual(before.property_values);
    expect(after.property_schemas).toEqual(before.property_schemas);
    expect(await dataClient.listQueuedSyncOperations()).toEqual(queued);
    expect(storage).toEqual(disk);
    expect(
      (await dataClient.listDocumentProperties("local-space", "local-welcome")).find(
        (value) => value.property_id === schema.id,
      )?.value,
    ).toBe("Before");
  });

  it("retains concurrent property writes and validates schema changes against the latest committed values", async () => {
    const left = await dataClient.createPropertySchema("local-space", {
      name: "Left durable",
      property_type: "text",
    });
    const right = await dataClient.createPropertySchema("local-space", {
      name: "Right durable",
      property_type: "text",
    });
    const beforeQueue = (await dataClient.listQueuedSyncOperations()).length;
    await Promise.all([
      dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
        { property_id: left.id, value: "Left" },
      ]),
      dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
        { property_id: right.id, value: "Right" },
      ]),
    ]);
    const values = await dataClient.listDocumentProperties("local-space", "local-welcome");
    expect(values.find((value) => value.property_id === left.id)?.value).toBe("Left");
    expect(values.find((value) => value.property_id === right.id)?.value).toBe("Right");
    expect((await dataClient.listQueuedSyncOperations()).length).toBe(beforeQueue + 2);
    const results = await Promise.allSettled([
      dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
        { property_id: left.id, value: "Newest" },
      ]),
      dataClient.updatePropertySchema("local-space", left.id, {
        name: "Left durable",
        property_type: "number",
      }),
    ]);
    expect(results[0].status).toBe("fulfilled");
    expect(results[1].status).toBe("rejected");
    expect(
      (await dataClient.listDocumentProperties("local-space", "local-welcome")).find(
        (value) => value.property_id === left.id,
      )?.value,
    ).toBe("Newest");
  });

  it("creates template field properties atomically and rejects foreign bindings before page creation", async () => {
    const schema = await dataClient.createPropertySchema("local-space", {
      name: "Chef",
      property_type: "text",
    });
    const saved = await dataClient.createTemplate("local-space", {
      name: "Recipe",
      content: "Recipe",
      properties: { Chef: "Default chef", __starlane_template: { schema_version: 1, fields: [] } },
    });
    const created = await dataClient.createDocument("local-space", {
      title: "Dinner",
      template_id: saved.id,
      source_id: "field-page",
      property_values: [{ property_id: schema.id, value: "Eagle" }],
    });
    expect(
      (await dataClient.listDocumentProperties("local-space", created.id)).find(
        (item) => item.property_id === schema.id,
      )?.value,
    ).toBe("Eagle");
    await expect(
      dataClient.createDocument("local-space", { title: "Dinner", source_id: "field-page" }),
    ).rejects.toThrow("already exists");
    const before = await dataClient.listDocuments("local-space");
    await expect(
      dataClient.createDocument("local-space", {
        title: "Invalid",
        property_values: [{ property_id: "foreign", value: "unsafe" }],
      }),
    ).rejects.toThrow("not in this space");
    expect(await dataClient.listDocuments("local-space")).toHaveLength(before.length);
  });

  it("prevalidates typed batches and preserves all values and replay queue on rejection", async () => {
    const text = await dataClient.createPropertySchema("local-space", {
      name: "Note",
      property_type: "text",
    });
    const number = await dataClient.createPropertySchema("local-space", {
      name: "Amount",
      property_type: "number",
    });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: text.id, value: "Original" },
    ]);
    const before = await dataClient.exportLocalVaultSnapshot();
    const queue = await dataClient.listQueuedSyncOperations();
    await expect(
      dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
        { property_id: text.id, value: "Changed" },
        { property_id: number.id, value: "NaN" },
      ]),
    ).rejects.toThrow("does not match number");
    expect((await dataClient.exportLocalVaultSnapshot()).property_values).toEqual(
      before.property_values,
    );
    expect(await dataClient.listQueuedSyncOperations()).toEqual(queue);
    await expect(
      dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
        { property_id: "foreign", value: null },
      ]),
    ).rejects.toThrow("not in this space");
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: number.id, value: ".5" },
    ]);
    expect(
      (await dataClient.listDocumentProperties("local-space", "local-welcome")).find(
        (value) => value.property_id === number.id,
      )?.value,
    ).toBe(".5");
  });

  it("rejects incompatible select options and bad template defaults without creating a page", async () => {
    const schema = await dataClient.createPropertySchema("local-space", {
      name: "Choice",
      property_type: "select",
      options: { options: ["A", "B"] },
    });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: schema.id, value: "A" },
    ]);
    await expect(
      dataClient.updatePropertySchema("local-space", schema.id, {
        name: "Choice",
        property_type: "select",
        options: ["B"],
      }),
    ).rejects.toThrow("does not match select");
    const template = await dataClient.createTemplate("local-space", {
      name: "Bad",
      content: "Body",
      properties: { Choice: "C" },
    });
    const before = await dataClient.exportLocalVaultSnapshot();
    await expect(
      dataClient.createDocument("local-space", { title: "Bad page", template_id: template.id }),
    ).rejects.toThrow("does not match select");
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(after.documents).toEqual(before.documents);
    expect(after.drafts).toEqual(before.drafts);
    expect(after.doc_tags).toEqual(before.doc_tags);
    // Explicit valid field values override invalid defaults before validation.
    await expect(
      dataClient.createDocument("local-space", {
        title: "Good page",
        template_id: template.id,
        property_values: [{ property_id: schema.id, value: "B" }],
      }),
    ).resolves.toHaveProperty("id");
  });

  it("checks local relation scope/lifecycle and active standalone user identity", async () => {
    const relation = await dataClient.createPropertySchema("local-space", {
      name: "Related",
      property_type: "relation",
    });
    const user = await dataClient.createPropertySchema("local-space", {
      name: "Responsible",
      property_type: "user",
    });
    const target = await dataClient.createDocument("local-space", { title: "Own draft" });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: relation.id, value: target.id },
      { property_id: user.id, value: "local-user" },
    ]);
    await dataClient.archiveDocument("local-space", target.id);
    await expect(
      dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
        { property_id: relation.id, value: target.id },
      ]),
    ).rejects.toThrow("unavailable");
    await expect(
      dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
        { property_id: user.id, value: "missing-user" },
      ]),
    ).rejects.toThrow("active account");
    const other = await dataClient.createSpace("Other");
    const foreign = await dataClient.createDocument(other.id, { title: "Foreign" });
    await expect(
      dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
        { property_id: relation.id, value: foreign.id },
      ]),
    ).rejects.toThrow("unavailable");
  });

  // Archive counterparts use the native-first fixture in nativeJointArchiveImport.test.ts.
  it.each(["snapshot"])("gates nonempty %s replay imports on the actual lossless native descriptor before mutation", async (transport) => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-10-09T12:00:00Z"));
    const before = await dataClient.exportLocalVaultSnapshot();
    const operation = { id: "old-op", entity_type: "document", entity_id: "local-welcome", action: "update_content", queued_at: "old-time", payload: null };
    const snapshot = { ...before, sync_state: { state: "idle", updated_at: "old", queued_operations: [operation] } };
    const valid = { schema_version: 1, provider: "none", store_kind: "sqlite", supports_atomic_snapshot_commit: true, supports_operation_queue: true, supports_lossless_replay_queue: true };
    try {
      for (const descriptor of [new Error("Unknown describe command"), null, [], {}, { ...valid, supports_lossless_replay_queue: undefined }, { ...valid, supports_lossless_replay_queue: "true" }, { ...valid, schema_version: "1" }, { ...valid, schema_version: 2 }, { ...valid, provider: "google-drive" }, { ...valid, store_kind: "snapshot-json" }, { ...valid, supports_atomic_snapshot_commit: false }, { ...valid, supports_operation_queue: false }]) {
        const invoke = vi.fn(async (command: string) => {
          if (command === "prepare_vault_archive_import") return { handle: "prepared", snapshot_json: JSON.stringify(snapshot) };
          if (command === "describe_native_local_database") {
            if (descriptor instanceof Error) throw descriptor;
            return descriptor;
          }
          if (command === "cancel_vault_archive_import") return undefined;
          throw new Error(`Unexpected mutation: ${command}`);
        });
        (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
        const run = transport === "archive" ? dataClient.importLocalVaultArchive("/selected.zip") : dataClient.importLocalVaultSnapshot(snapshot);
        await expect(run).rejects.toThrow("Update the desktop shell");
        expect(invoke.mock.calls.map(([command]) => command)).toEqual(transport === "archive" ? ["prepare_vault_archive_import", "describe_native_local_database", "cancel_vault_archive_import"] : ["describe_native_local_database"]);
        delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
        expect(await dataClient.exportLocalVaultSnapshot()).toEqual(before);
      }
      const invoke = vi.fn(async (command: string) => {
        if (command === "prepare_vault_archive_import") return { handle: "prepared", snapshot_json: JSON.stringify(snapshot) };
        if (command === "describe_native_local_database") return valid;
        if (command === "restore_validated_vault_archive_import") return { handle: "prepared", snapshot_json: JSON.stringify(snapshot), asset_updates: [] };
        if (command === "cancel_vault_archive_import") return undefined;
        throw new Error(`Unexpected command: ${command}`);
      });
      (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
      await (transport === "archive" ? dataClient.importLocalVaultArchive("/selected.zip") : dataClient.importLocalVaultSnapshot(snapshot));
      delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      expect((await dataClient.exportLocalVaultSnapshot()).sync_state?.queued_operations).toEqual([operation]);
    } finally {
      delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      vi.useRealTimers();
    }
  });

  it("keeps empty legacy native snapshot imports compatible without a lossless descriptor", async () => {
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    snapshot.sync_state = { state: "idle", updated_at: "old", queued_operations: [] };
    const text = JSON.stringify(snapshot);
    const invoke = vi.fn(async (command: string) => {
      if (command === "prepare_vault_archive_import") return { handle: "prepared", snapshot_json: text };
      if (command === "restore_validated_vault_archive_import") return { handle: "prepared", snapshot_json: text, asset_updates: [] };
      if (command === "cancel_vault_archive_import") return undefined;
      throw new Error(`Old shell command unavailable: ${command}`);
    });
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    try {
      await dataClient.importLocalVaultSnapshot(snapshot);
      expect(invoke).not.toHaveBeenCalled();
    } finally {
      delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it.each(["snapshot-provider", "snapshot-mode"])("rejects %s context drift during lossless capability IPC before restore/save/cache", async (kind) => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-10-09T12:00:00Z"));
    const before = await dataClient.exportLocalVaultSnapshot();
    const snapshot = { ...before, sync_state: { state: "idle", updated_at: "old", queued_operations: [{ id: "op", entity_type: "document", entity_id: "local-welcome", action: "update_content", queued_at: "old" }] } };
    const invoke = vi.fn(async (command: string) => {
      if (command === "prepare_vault_archive_import") return { handle: "prepared", snapshot_json: JSON.stringify(snapshot) };
      if (command === "describe_native_local_database") {
        if (kind.endsWith("provider")) setSyncProvider("google-drive"); else setCurrentLaunchMode("server");
        return { schema_version: 1, provider: "none", store_kind: "sqlite", supports_atomic_snapshot_commit: true, supports_operation_queue: true, supports_lossless_replay_queue: true };
      }
      if (command === "cancel_vault_archive_import") return undefined;
      throw new Error(`Unexpected canonical mutation: ${command}`);
    });
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    try {
      await expect(kind.startsWith("archive") ? dataClient.importLocalVaultArchive("/selected.zip") : dataClient.importLocalVaultSnapshot(snapshot)).rejects.toThrow("context changed");
      expect(invoke.mock.calls.map(([command]) => command)).toEqual(kind.startsWith("archive") ? ["prepare_vault_archive_import", "describe_native_local_database", "cancel_vault_archive_import"] : ["describe_native_local_database"]);
      delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      setCurrentLaunchMode("local"); setSyncProvider("none");
      expect(await dataClient.exportLocalVaultSnapshot()).toEqual(before);
    } finally {
      delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      setCurrentLaunchMode("local"); setSyncProvider("none"); vi.useRealTimers();
    }
  });

  // ZIP raw queue, full typed properties, exact asset plans and old-shell denial
  // remain covered in nativeJointArchiveImport.test.ts before plan/commit/cache.

  it.each([undefined, null, "", 42])("requires raw imported property value IDs before either import mutation (%s)", async (id) => {
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    const imported = { ...snapshot, property_schemas: [{ id: "text", space_id: "local-space", name: "Text", property_type: "text", sort_order: 0 }], property_values: [{ id, document_id: "local-welcome", property_id: "text", value: "retained" }] };
    await expect(dataClient.importLocalVaultSnapshot(imported)).rejects.toThrow("value id");
    const request = vi.spyOn(api, "fetch").mockRejectedValue(new Error("Unexpected remote request"));
    setCurrentLaunchMode("server");
    await expect(dataClient.importVaultSnapshotToServer(imported)).rejects.toThrow("value id");
    expect(request).not.toHaveBeenCalled();
  });

  const rawImportCases = (() => {
    const schema = { id: "raw-schema", space_id: "local-space", name: "Raw", property_type: "text", sort_order: 0 };
    const cases: { name: string; fields: Record<string, unknown> }[] = [];
    for (const field of ["property_schemas", "property_values"]) {
      for (const [label, value] of [["null", null], ["undefined", undefined], ["object", {}], ["string", "not-an-array"], ["number", 1]] as const)
        cases.push({ name: `${field} ${label}`, fields: { [field]: value } });
    }
    for (const [label, value] of [["null", null], ["undefined", undefined], ["string", "1"], ["fraction", 1.5], ["below i32", -2147483649], ["above i32", 2147483648], ["NaN", NaN], ["Infinity", Infinity]] as const)
      cases.push({ name: `order ${label}`, fields: { property_schemas: [{ ...schema, sort_order: value }] } });
    for (const [label, value] of [["null", null], ["undefined", undefined], ["empty", ""], ["number", 1], ["object", {}]] as const)
      cases.push({ name: `type ${label}`, fields: { property_schemas: [{ ...schema, property_type: value }] } });
    for (const [label, value] of [["undefined", undefined], ["null", null], ["empty", ""], ["number", 1]] as const)
      cases.push({ name: `id ${label}`, fields: { property_schemas: [{ ...schema, id: value }] } });
    const missingId = { ...schema } as Partial<typeof schema>;
    delete missingId.id;
    cases.push({ name: "missing id", fields: { property_schemas: [missingId] } });
    const missingType = { ...schema } as Partial<typeof schema>;
    delete missingType.property_type;
    cases.push({ name: "missing type", fields: { property_schemas: [missingType] } });
    cases.push({ name: "null name", fields: { property_schemas: [{ ...schema, name: null }] } });
    cases.push({ name: "duplicate id", fields: { property_schemas: [schema, { ...schema, name: "Duplicate" }] } });
    cases.push({ name: "duplicate select options", fields: { property_schemas: [{ ...schema, property_type: "select", options: ["one", "one"] }] } });
    return cases;
  })();

  it.each(rawImportCases)("rejects raw property admission $name in both public importers", async ({ fields }) => {
    // Snapshot reads refresh diagnostic sync timestamps. Freeze only Date so
    // full-state equality stays meaningful without excluding any queue fields.
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-10-08T12:00:00Z"));
    try {
      await dataClient.createPropertySchema("local-space", { name: "Retained", property_type: "text" });
      const before = await dataClient.exportLocalVaultSnapshot();
      const beforeCache = await dataClient.listPropertySchemas("local-space");
      const request = vi.spyOn(api, "fetch").mockRejectedValue(new Error("Unexpected remote request"));
      const imported = { ...before, property_values: [], ...fields };
      setCurrentLaunchMode("local");
      await expect(dataClient.importLocalVaultSnapshot(imported)).rejects.toThrow();
      expect(await dataClient.exportLocalVaultSnapshot()).toEqual(before);
      expect(await dataClient.listPropertySchemas("local-space")).toEqual(beforeCache);
      setCurrentLaunchMode("server");
      await expect(dataClient.importVaultSnapshotToServer(imported)).rejects.toThrow();
      expect(request).not.toHaveBeenCalled();
      setCurrentLaunchMode("local");
      expect(await dataClient.exportLocalVaultSnapshot()).toEqual(before);
    } finally {
      vi.useRealTimers();
    }
  });

  it("preserves omitted legacy collections/order and exact valid recorded schema metadata", async () => {
    const before = await dataClient.exportLocalVaultSnapshot();
    const legacy = { ...before };
    delete legacy.property_schemas;
    delete legacy.property_values;
    await dataClient.importLocalVaultSnapshot(legacy);
    expect((await dataClient.exportLocalVaultSnapshot()).property_schemas).toEqual([]);
    const schemas = [
      { id: "legacy:schema/raw", space_id: "local-space", name: " Raw name ", property_type: "select", options: { options: [" one ", "two"] } },
      { id: "lower", space_id: "local-space", name: "Lower", property_type: "text", sort_order: -2147483648 },
      { id: "upper", space_id: "local-space", name: "Upper", property_type: "text", sort_order: 2147483647 },
    ];
    await dataClient.importLocalVaultSnapshot({ ...legacy, property_schemas: schemas });
    expect((await dataClient.exportLocalVaultSnapshot()).property_schemas).toEqual(schemas);
    expect(Object.prototype.hasOwnProperty.call((await dataClient.exportLocalVaultSnapshot()).property_schemas![0], "sort_order")).toBe(false);
  });

  it("retains valid raw schema options and relative server reorder after admission", async () => {
    const before = await dataClient.exportLocalVaultSnapshot();
    const imported = { ...before, folders: [], documents: [], drafts: [], templates: [], assets: [], property_values: [], property_schemas: [
      { id: "upper", space_id: "local-space", name: " Upper ", property_type: "text", sort_order: 2147483647 },
      { id: "lower", space_id: "local-space", name: "Lower", property_type: "select", options: { options: [" one ", "two"] }, sort_order: -2147483648 },
    ] };
    const space = vi.spyOn(dataClient, "createSpace").mockResolvedValue({ ...before.spaces[0], id: "destination", slug: "destination" });
    const create = vi.spyOn(dataClient, "createPropertySchema").mockImplementation(async (_space, input) => ({ id: input.source_id!, name: input.name, property_type: input.property_type!, options: input.options ?? null, sort_order: 0 }));
    const reorder = vi.spyOn(dataClient, "reorderPropertySchemas").mockResolvedValue(undefined);
    setCurrentLaunchMode("server");
    await dataClient.importVaultSnapshotToServer({ schema_version: 1, spaces: [], folders: [], documents: [] });
    await dataClient.importVaultSnapshotToServer(imported);
    expect(space).toHaveBeenCalledOnce();
    expect(create.mock.calls.map(([, input]) => input.source_id)).toEqual(["lower", "upper"]);
    expect(create.mock.calls[0][1].options).toEqual(imported.property_schemas[1].options);
    expect(reorder).toHaveBeenCalledWith("destination", { property_ids: ["lower", "upper"] });
  });

  it("rejects malformed imported property batches before replacing the vault", async () => {
    const schema = await dataClient.createPropertySchema("local-space", {
      name: "Count",
      property_type: "number",
    });
    const before = await dataClient.exportLocalVaultSnapshot();
    const imported = {
      ...before,
      property_values: [
        { id: "bad", document_id: "local-welcome", property_id: schema.id, value: "Infinity" },
      ],
    };
    await expect(dataClient.importLocalVaultSnapshot(imported)).rejects.toThrow(
      "does not match number",
    );
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(after.documents).toEqual(before.documents);
    expect(after.property_values).toEqual(before.property_values);
    expect(after.property_schemas).toEqual(before.property_schemas);
    expect(after.sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
  });

  it("blocks a malformed provider value batch without changing prior assignments", async () => {
    const text = await dataClient.createPropertySchema("local-space", {
      name: "Text",
      property_type: "text",
    });
    const number = await dataClient.createPropertySchema("local-space", {
      name: "Number",
      property_type: "number",
    });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: text.id, value: "Original" },
    ]);
    await dataClient.clearQueuedSyncOperations();
    const before = (await dataClient.exportLocalVaultSnapshot()).property_values;
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-10-07T12:00:00Z",
      "invalid-properties",
    );
    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "invalid",
            entity_type: "document_property_values",
            entity_id: "local-welcome",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "local-welcome",
              space_id: "local-space",
              values: [
                { property_id: text.id, value: "Changed" },
                { property_id: number.id, value: "NaN" },
              ],
            },
          },
        ],
      }),
    );
    expect(result.failed_operation_count).toBe(1);
    expect((await dataClient.exportLocalVaultSnapshot()).property_values).toEqual(before);
    expect(await dataClient.listQueuedSyncOperations()).toEqual([]);
    const validReplay = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 1,
      operations: [
        {
          id: "legacy-replay",
          entity_type: "document_property_values",
          entity_id: "local-welcome",
          action: "set",
          payload_status: "available",
          payload: {
            document_id: "local-welcome",
            space_id: "local-space",
            values: [{ id: "legacy:value-number", property_id: number.id, value: "01" }],
          },
        },
      ],
    });
    expect(
      (await dataClient.applyProviderRemoteImportManifest(validReplay)).applied_operation_count,
    ).toBe(1);
    expect((await dataClient.exportLocalVaultSnapshot()).property_values).toEqual(
      expect.arrayContaining([expect.objectContaining({ id: "legacy:value-number", value: "01" })]),
    );
  });

  it("preserves draft ownership in imported snapshots and denies inaccessible draft relations", async () => {
    const target = await dataClient.createDocument("local-space", { title: "Draft" });
    const relation = await dataClient.createPropertySchema("local-space", {
      name: "Related",
      property_type: "relation",
    });
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    const drafts = (snapshot.drafts ?? []).map((draft) =>
      draft.document_id === target.id ? { ...draft, owner_id: "other-user" } : draft,
    );
    await dataClient.importLocalVaultSnapshot({ ...snapshot, drafts });
    expect((await dataClient.exportLocalVaultSnapshot()).drafts).toEqual(drafts);
    await expect(
      dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
        { property_id: relation.id, value: target.id },
      ]),
    ).rejects.toThrow("unavailable");
  });

  it("preflights malformed property imports before creating remote entities", async () => {
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    const invalid = {
      ...snapshot,
      property_schemas: [
        {
          id: "number",
          space_id: "local-space",
          name: "Number",
          property_type: "number",
          sort_order: 1,
        },
      ],
      property_values: [
        { id: "bad", document_id: "local-welcome", property_id: "number", value: "NaN" },
      ],
    };
    setCurrentLaunchMode("server");
    let requests = 0;
    (globalThis as { fetch?: unknown }).fetch = async () => {
      requests++;
      return new Response("{}", { status: 200 });
    };
    await expect(dataClient.importVaultSnapshotToServer(invalid)).rejects.toThrow(
      "does not match number",
    );
    expect(requests).toBe(0);
  });

  it("preflights import identities, timestamps and active subjects before any batch write", async () => {
    const one = await dataClient.createPropertySchema("local-space", {
      name: "One",
      property_type: "text",
    });
    const two = await dataClient.createPropertySchema("local-space", {
      name: "Two",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: one.id, value: "Original", source_id: "existing-value" },
    ]);
    const before = await dataClient.exportLocalVaultSnapshot();
    for (const metadata of [
      { source_id: "invalid/id" },
      { created_at: "yesterday" },
      { source_id: "existing-value" },
    ]) {
      await expect(
        dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
          { property_id: one.id, value: "Changed" },
          { property_id: two.id, value: "Bad", ...metadata },
        ]),
      ).rejects.toThrow();
      expect((await dataClient.exportLocalVaultSnapshot()).property_values).toEqual(
        before.property_values,
      );
    }
    await expect(
      dataClient.createPropertySchema("local-space", {
        name: "Collision",
        property_type: "text",
        source_id: one.id,
      }),
    ).rejects.toThrow("already exists");
    const page = await dataClient.createDocument("local-space", { title: "Inactive" });
    await dataClient.archiveDocument("local-space", page.id);
    await expect(
      dataClient.setDocumentPropertyValues("local-space", page.id, [
        { property_id: one.id, value: "No" },
      ]),
    ).rejects.toThrow("archived or deleted");
    const archived = await dataClient.exportLocalVaultSnapshot();
    await dataClient.importLocalVaultSnapshot(archived);
    await expect(
      dataClient.setDocumentPropertyValues("local-space", page.id, [
        { property_id: one.id, value: "No" },
      ]),
    ).rejects.toThrow("archived or deleted");
  });

  it("blocks unmapped user properties before migration and preserves the local snapshot", async () => {
    const owner = await dataClient.createPropertySchema("local-space", {
      name: "Owner",
      property_type: "user",
    });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: owner.id, value: "local-user" },
    ]);
    const before = await dataClient.exportLocalVaultSnapshot();
    setCurrentLaunchMode("server");
    let requests = 0;
    (globalThis as { fetch?: unknown }).fetch = async () => {
      requests++;
      return new Response("{}", { status: 200 });
    };
    await expect(dataClient.importVaultSnapshotToServer(before)).rejects.toThrow(
      "Choose an active server account mapping",
    );
    expect(requests).toBe(0);
    setCurrentLaunchMode("local");
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(after.documents).toEqual(before.documents);
    expect(after.property_values).toEqual(before.property_values);
    expect(after.property_schemas).toEqual(before.property_schemas);
    expect(after.sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
  });

  it("recovers a committed server page after an ambiguous create failure using the same operation identity", async () => {
    setCurrentLaunchMode("server");
    const requests: string[] = [];
    (globalThis as { fetch?: unknown }).fetch = (async (url: string, init?: RequestInit) => {
      requests.push(`${init?.method ?? "GET"} ${url}`);
      if (init?.method === "POST") {
        expect(JSON.parse(String(init.body)).source_id).toBe("operation-1");
        return new Response(JSON.stringify({ error: "Connection lost after commit" }), {
          status: 503,
        });
      }
      return new Response(
        JSON.stringify({ id: "operation-1", title: "Dinner", draft_id: "draft-1" }),
        { headers: { "Content-Type": "application/json" } },
      );
    }) as typeof fetch;
    await expect(
      dataClient.createDocument("space-1", { title: "Dinner", source_id: "operation-1" }),
    ).resolves.toEqual({ id: "operation-1", draft_id: "draft-1" });
    expect(requests.filter((request) => request.startsWith("POST"))).toHaveLength(1);
    expect(
      requests.some((request) => request.endsWith("/spaces/space-1/documents/operation-1")),
    ).toBe(true);
  });

  it("seeds a local space and welcome doc", async () => {
    const spaces = await dataClient.listSpaces();
    expect(spaces.length).toBe(1);
    expect(spaces[0].id).toBe("local-space");
    expect(spaces[0].name).toBe("Personal Space");
    expect(spaces[0]).toMatchObject({
      owner_username: "local-user",
      owner_display_name: "Local User",
      created_by: "local-user",
    });

    await expect(dataClient.getSpace("local-space")).resolves.toMatchObject({
      owner_username: "local-user",
      owner_display_name: "Local User",
      created_by: "local-user",
    });

    const docs = await dataClient.listDocuments("local-space");
    expect(docs.some((d) => d.id === "local-welcome")).toBe(true);
  });

  it("keeps an unpublished local page discoverable through its private draft", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Durable draft",
      content: "Saved without publishing",
    });

    await expect(dataClient.listDocuments("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: created.id,
          title: "Durable draft",
          published_at: undefined,
          draft_id: created.draft_id,
        }),
      ]),
    );
  });

  it("matches local stable backlinks by exact ID and keeps legacy title-only links", async () => {
    const firstTarget = await dataClient.createDocument("local-space", {
      title: "Launch Plan",
      content: "First target",
    });
    const secondTarget = await dataClient.createDocument("local-space", {
      title: "Launch Plan",
      content: "Second target",
    });
    const stableLinkToSecond = await dataClient.createDocument("local-space", {
      title: "Stable link to second",
      content: `[[A renamed label|${secondTarget.id}]]`,
    });
    const firstIdPrefix = await dataClient.createDocument("local-space", {
      title: "First ID prefix",
      content: `[[Launch Plan|prefix-${firstTarget.id}]]`,
    });
    const secondIdSuffix = await dataClient.createDocument("local-space", {
      title: "Second ID suffix",
      content: `[[Launch Plan|${secondTarget.id}-suffix]]`,
    });
    const legacyTitleLink = await dataClient.createDocument("local-space", {
      title: "Legacy title link",
      content: "[[ launch PLAN ]]",
    });

    const firstBacklinks = await dataClient.listDocumentBacklinks("local-space", firstTarget.id);
    const secondBacklinks = await dataClient.listDocumentBacklinks("local-space", secondTarget.id);

    expect(firstBacklinks.map(({ document_id }) => document_id)).toEqual([legacyTitleLink.id]);
    expect(secondBacklinks.map(({ document_id }) => document_id).sort()).toEqual(
      [legacyTitleLink.id, stableLinkToSecond.id].sort(),
    );
    expect(firstBacklinks.some(({ document_id }) => document_id === firstIdPrefix.id)).toBe(false);
    expect(secondBacklinks.some(({ document_id }) => document_id === secondIdSuffix.id)).toBe(
      false,
    );
  });

  it("keeps TOTP and passkey server-account actions unavailable in local mode", async () => {
    await expect(dataClient.setupTotp()).rejects.toThrow(
      "Two-factor authentication is unavailable in local mode.",
    );
    await expect(dataClient.verifyTotp("123456")).rejects.toThrow(
      "Two-factor authentication is unavailable in local mode.",
    );
    await expect(dataClient.disableTotp("123456")).rejects.toThrow(
      "Two-factor authentication is unavailable in local mode.",
    );

    await expect(dataClient.listPasskeys()).resolves.toEqual([]);
    await expect(dataClient.startPasskeyRegistration("Phone")).rejects.toThrow(
      "Passkeys are unavailable in local mode.",
    );
    await expect(dataClient.finishPasskeyRegistration("challenge-1", {}, "Phone")).rejects.toThrow(
      "Passkeys are unavailable in local mode.",
    );
    await expect(dataClient.deletePasskey("passkey-1")).rejects.toThrow(
      "Passkeys are unavailable in local mode.",
    );
    await expect(dataClient.startPasskeyLogin("eagle@example.test")).rejects.toThrow(
      "Passkey sign-in is unavailable in local mode.",
    );
    await expect(dataClient.finishPasskeyLogin("challenge-1", {})).rejects.toThrow(
      "Passkey sign-in is unavailable in local mode.",
    );
  });

  it("saves and resolves document-owned PNG previews by filename", async () => {
    const previewUrl =
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

    await dataClient.saveDocumentDataAsset({
      name: "preview.png",
      mime_type: "image/png",
      data_url: previewUrl,
      space_id: "local-space",
      document_id: "local-welcome",
    });

    await expect(
      dataClient.resolveDocumentAssetUrl("preview.png", {
        space_id: "local-space",
        document_id: "local-welcome",
      }),
    ).resolves.toBe(previewUrl);
  });

  it("loads server PNG previews through the authenticated attachment path", async () => {
    setCurrentLaunchMode("server");
    api.setToken("preview-token");
    const requests: Array<{ url: string; authorization?: string }> = [];
    (globalThis as { fetch?: unknown }).fetch = (async (url: string, init?: RequestInit) => {
      const headers = new Headers(init?.headers);
      requests.push({
        url,
        authorization: headers.get("Authorization") ?? undefined,
      });
      if (url.endsWith("/spaces/space-1/documents/doc-1/attachments")) {
        return new Response(
          JSON.stringify([
            {
              id: "attachment-1",
              filename: "attachment-1.png",
              original_name: "architecture.png",
              mime_type: "image/png",
              size_bytes: 4,
              url: "/api/v1/files/spaces/space-1/attachments/attachment-1.png",
              created_at: "2026-07-25T00:00:00Z",
            },
          ]),
          { headers: { "Content-Type": "application/json" } },
        );
      }
      return new Response(new Uint8Array([137, 80, 78, 71]), {
        headers: { "Content-Type": "image/png" },
      });
    }) as typeof fetch;

    await expect(
      dataClient.resolveDocumentAssetUrl("architecture.png", {
        space_id: "space-1",
        document_id: "doc-1",
      }),
    ).resolves.toBe("data:image/png;base64,iVBORw==");
    expect(requests[requests.length - 1]?.authorization).toBe("Bearer preview-token");
  });

  it("does not replace an unavailable source-bound draft or retry its bytes", async () => {
    setCurrentLaunchMode("server"); api.setToken(identifierSessionToken("a"));
    const request = vi.fn(async (_url: string) => new Response(JSON.stringify({ message: "Draft not found" }), { status: 404 }));
    (globalThis as { fetch?: unknown }).fetch = request;
    await expect(dataClient.updateDocumentDraft("s", "a", "old", { content: "Retained edit" }, "a".repeat(64))).rejects.toMatchObject({ reason: "outcome_unknown" });
    expect(request).toHaveBeenCalledTimes(1); expect(request.mock.calls[0]?.[0]).toContain("/drafts/old");
  });

  it("retains offline server attachment bytes without writing the current local vault", async () => {
    setCurrentLaunchMode("server");
    api.setToken(`e30.${btoa(JSON.stringify({ sub: "asset-owner", sid: "asset-session", token_type: "access" }))}.fixture`);
    (globalThis as { fetch?: unknown }).fetch = (async () => {
      throw new TypeError("Server is unreachable");
    }) as typeof fetch;

    const nativeInvoke = vi.fn();
    (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = { invoke: nativeInvoke };
    const mirror = vi.spyOn(dataClient, "saveBrowserAsset");
    const saved = await dataClient.saveDocumentDataAsset({
      name: "offline-diagram.png",
      mime_type: "image/png",
      data_url: "data:image/png;base64,ZmFrZQ==",
      space_id: "space-1",
      document_id: "doc-1",
    });

    expect(saved).toMatchObject({
      name: "offline-diagram.png",
      document_id: "doc-1",
      space_id: "space-1",
    });
    expect(mirror).not.toHaveBeenCalled(); expect(nativeInvoke).not.toHaveBeenCalled();
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    expect(listOfflineServerAttachments()[0]).toMatchObject({ dataUrl: "data:image/png;base64,ZmFrZQ==", replayState: "unknown" });
    setCurrentLaunchMode("local");
    await expect(dataClient.getLocalVaultStats()).resolves.toMatchObject({ assets: 0 });
  });

  it("updates local personal-space identity and description", async () => {
    const updated = await dataClient.updateSpace("local-space", {
      name: "Field Notes",
      description: "A local-first field notebook.",
    });
    expect(updated.name).toBe("Field Notes");
    expect(updated.description).toBe("A local-first field notebook.");
    expect(updated.member_role).toBe("owner");

    const spaces = await dataClient.listSpaces();
    expect(spaces[0].name).toBe("Field Notes");
    expect(spaces[0].description).toBe("A local-first field notebook.");
  });

  it("creates and updates local documents", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Draft",
      content: "hello",
    });

    await dataClient.updateDocument("local-space", created.id, { title: "Draft 2" });
    await dataClient.updateDocumentContent("local-space", created.id, "# Body");

    const doc = await dataClient.getDocument("local-space", created.id);
    const content = await dataClient.getDocumentContent("local-space", created.id);
    expect(doc.title).toBe("Draft 2");
    expect(content.content).toBe("# Body");
  });

  it("creates new local documents with an open writing canvas", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Sectioned draft",
    });

    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: DEFAULT_NEW_DOCUMENT_CONTENT,
    });
    expect(DEFAULT_NEW_DOCUMENT_CONTENT).toBe("");
  });

  it.each(["archive", "trash", "restore", "unarchive"] as const)("keeps lifecycle %s cache, durable bytes and replay unchanged after failed storage", async action => {
    const page = await dataClient.createDocument("local-space", { title: "Retained", content: "Raw\r\nbody" });
    if (action === "restore") await dataClient.deleteDocument("local-space", page.id);
    if (action === "unarchive") await dataClient.archiveDocument("local-space", page.id);
    const before = await dataClient.exportLocalVaultSnapshot(), disk = new Map(storage);
    const original = window.localStorage.setItem;
    window.localStorage.setItem = () => { throw new Error("Lifecycle quota"); };
    try {
      const method = { archive: "archiveDocument", trash: "deleteDocument", restore: "restoreDocument", unarchive: "unarchiveDocument" }[action] as "archiveDocument" | "deleteDocument" | "restoreDocument" | "unarchiveDocument";
      await expect(dataClient[method]("local-space", page.id)).rejects.toThrow("Lifecycle quota");
    } finally { window.localStorage.setItem = original; }
    expect(storage).toEqual(disk);
    // Read the live cache before export can rehydrate it from durable storage.
    expect((await dataClient.listArchivedDocuments("local-space")).some(row => row.id === page.id)).toBe(action === "unarchive");
    expect((await dataClient.listTrashedDocuments("local-space")).some(row => row.id === page.id)).toBe(action === "restore");
    expect((await dataClient.exportLocalVaultSnapshot()).documents).toEqual(before.documents);
    expect((await dataClient.listQueuedSyncOperations())).toEqual(before.sync_state?.queued_operations);
  });

  it("patches only the lifecycle target using latest durable body and placement, preserving children and private records", async () => {
    const parent = await dataClient.createDocument("local-space", { title: "Parent" });
    const page = await dataClient.createDocument("local-space", { title: "Target", parent_id: parent.id });
    await dataClient.createDocument("local-space", { title: "Child", parent_id: page.id });
    const adapter = createLocalVaultAdapter("none");
    await adapter.commitSnapshotMutation(snapshot => {
      const target = snapshot.documents.find(row => row.id === page.id)!;
      target.content = "Latest durable body\r\n<!-- exact -->"; target.title = "Durable title"; target.parent_id = undefined;
      return { snapshot };
    });
    const before = await adapter.loadSnapshot();
    await dataClient.archiveDocument("local-space", page.id);
    const after = await createLocalVaultAdapter("none").loadSnapshot();
    expect(after.documents.find(row => row.id === page.id)).toMatchObject({ title: "Durable title", content: "Latest durable body\r\n<!-- exact -->", archived_at: expect.any(String) });
    expect(after.documents.find(row => row.id === page.id)?.parent_id).toBeUndefined();
    expect(after.documents.filter(row => row.id !== page.id)).toEqual(before.documents.filter(row => row.id !== page.id));
    expect(after.drafts).toEqual(before.drafts); expect(after.assets).toEqual(before.assets);
  });

  it("retains legacy optional birth fields while merging newer durable body and placement", async () => {
    const seed = await dataClient.exportLocalVaultSnapshot();
    seed.documents.push({ id: "legacy-lifecycle", space_id: "local-space", title: "Legacy", content: "Original", updated_at: "legacy-revision", published_at: "legacy-publication" });
    await __resetLocalDataForTests();
    await createLocalVaultAdapter("none").saveSnapshot(seed);
    await dataClient.listDocuments("local-space");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents.find(doc => doc.id === "legacy-lifecycle")?.created_at).toBeUndefined();
    await createLocalVaultAdapter("none").commitSnapshotMutation(snapshot => {
      const page = snapshot.documents.find(doc => doc.id === "legacy-lifecycle")!;
      page.content = "New durable legacy\r\n"; page.updated_at = "new-revision";
      return { snapshot };
    });
    await dataClient.archiveDocument("local-space", "legacy-lifecycle");
    const page = (await createLocalVaultAdapter("none").loadSnapshot()).documents.find(doc => doc.id === "legacy-lifecycle")!;
    expect(page).toMatchObject({ content: "New durable legacy\r\n", archived_at: expect.any(String) });
    expect(page.created_at).toBeUndefined();
  });

  it.each(["restoreDocument", "unarchiveDocument"] as const)("refuses %s on an active page without a new operation, matching current server transition prerequisites", async method => {
    const page = await dataClient.createDocument("local-space", { title: "Active" });
    const disk = new Map(storage);
    await expect(dataClient[method]("local-space", page.id)).rejects.toThrow("unavailable for this transition");
    expect(storage).toEqual(disk);
  });

  it("clears archive on Trash so archive→Trash→restore returns the target to active without touching children", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Lifecycle" });
    const child = await dataClient.createDocument("local-space", { title: "Child", parent_id: page.id });
    await dataClient.archiveDocument("local-space", page.id);
    await dataClient.deleteDocument("local-space", page.id);
    expect((await dataClient.listTrashedDocuments("local-space")).find(row => row.id === page.id)?.archived_at).toBeUndefined();
    await dataClient.restoreDocument("local-space", page.id);
    expect((await dataClient.listArchivedDocuments("local-space")).some(row => row.id === page.id)).toBe(false);
    expect((await dataClient.listDocuments("local-space")).some(row => row.id === page.id)).toBe(true);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents.find(row => row.id === child.id)?.parent_id).toBe(page.id);
  });

  it("retains exactly one lifecycle operation after lost ACK without publishing tentative cache", async () => {
    const page = await dataClient.createDocument("local-space", { title: "ACK" });
    await dataClient.clearQueuedSyncOperations();
    const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(createLocalVaultAdapter("none"));
    const commit = prototype.commitSnapshotMutation;
    vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (this: ReturnType<typeof createLocalVaultAdapter>, mutate: VaultSnapshotMutationBuilder) { await commit.call(this, mutate); throw new Error("Lifecycle ACK lost"); });
    await expect(dataClient.archiveDocument("local-space", page.id)).rejects.toThrow("Lifecycle ACK lost");
    expect((await dataClient.listArchivedDocuments("local-space")).some(row => row.id === page.id)).toBe(false);
    const cold = await createLocalVaultAdapter("none").loadSnapshot();
    expect(cold.documents.find(row => row.id === page.id)?.archived_at).toBeTruthy();
    expect(cold.sync_state?.queued_operations.filter(op => op.action === "archive")).toHaveLength(1);
    const operation = cold.sync_state!.queued_operations.find(op => op.action === "archive")!;
    await expect(dataClient.archiveDocument("local-space", page.id)).rejects.toThrow(/lifecycle.*changed/i);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).sync_state?.queued_operations).toEqual([operation]);
  });

  it.each(["replacement", "lifecycle"])("refuses a changed durable lifecycle source (%s) without changing the original operation queue", async change => {
    const page = await dataClient.createDocument("local-space", { title: "Original source" });
    const adapter = createLocalVaultAdapter("none");
    await adapter.commitSnapshotMutation(snapshot => {
      const row = snapshot.documents.find(doc => doc.id === page.id)!;
      if (change === "replacement") row.created_at = "replacement"; else row.deleted_at = "later-trash";
      return { snapshot };
    });
    const before = await adapter.loadSnapshot(), disk = new Map(storage);
    await expect(dataClient.archiveDocument("local-space", page.id)).rejects.toThrow(/source|lifecycle.*changed/i);
    expect(storage).toEqual(disk);
    const after = await createLocalVaultAdapter("none").loadSnapshot();
    expect(after.documents).toEqual(before.documents); expect(after.drafts).toEqual(before.drafts);
    expect(after.sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
    expect((await dataClient.listArchivedDocuments("local-space")).some(row => row.id === page.id)).toBe(false);
  });

  it("keeps the original committed lifecycle and one queue ID after selection changes before ACK without publishing its cache", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Late selection" });
    await dataClient.clearQueuedSyncOperations();
    const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(createLocalVaultAdapter("none"));
    const commit = prototype.commitSnapshotMutation;
    vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (this: ReturnType<typeof createLocalVaultAdapter>, mutate: VaultSnapshotMutationBuilder) {
      const saved = await commit.call(this, mutate);
      setSyncProvider("google-drive");
      return saved;
    });
    await expect(dataClient.archiveDocument("local-space", page.id)).rejects.toThrow("context changed");
    setSyncProvider("none");
    expect((await dataClient.listArchivedDocuments("local-space")).some(row => row.id === page.id)).toBe(false);
    const cold = await createLocalVaultAdapter("none").loadSnapshot();
    expect(cold.documents.find(row => row.id === page.id)?.archived_at).toBeTruthy();
    expect(cold.sync_state?.queued_operations).toEqual([expect.objectContaining({ id: expect.any(String), entity_id: page.id, action: "archive", attempts: 0 })]);
    expect((await createLocalVaultAdapter("google-drive").loadSnapshot()).documents.some(row => row.id === page.id)).toBe(false);
  });

  it("refuses an already mismatched provider at lifecycle entry and resumes only after original provider alignment", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Original provider page" });
    await dataClient.clearQueuedSyncOperations();
    const original = await createLocalVaultAdapter("none").loadSnapshot();
    const other = await createLocalVaultAdapter("google-drive").loadSnapshot();
    setSyncProvider("google-drive");
    await expect(dataClient.archiveDocument("local-space", page.id)).rejects.toThrow("selected provider changed");
    const unchanged = await createLocalVaultAdapter("none").loadSnapshot();
    expect(unchanged.documents).toEqual(original.documents);
    expect(unchanged.sync_state?.queued_operations).toEqual(original.sync_state?.queued_operations);
    const otherAfter = await createLocalVaultAdapter("google-drive").loadSnapshot();
    expect(otherAfter.documents).toEqual(other.documents);
    expect(otherAfter.sync_state?.queued_operations).toEqual(other.sync_state?.queued_operations);
    setSyncProvider("none");
    expect((await dataClient.listArchivedDocuments("local-space")).some(row => row.id === page.id)).toBe(false);
    await dataClient.archiveDocument("local-space", page.id);
    expect((await dataClient.listArchivedDocuments("local-space")).some(row => row.id === page.id)).toBe(true);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).sync_state?.queued_operations).toEqual([expect.objectContaining({ entity_id: page.id, action: "archive" })]);
    expect((await createLocalVaultAdapter("google-drive").loadSnapshot()).sync_state?.queued_operations).toEqual(other.sync_state?.queued_operations);
  });

  it("refuses lifecycle mutation when the selected provider changes before canonical mutation", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Original selection" });
    const before = new Map(storage);
    const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(createLocalVaultAdapter("none"));
    const support = prototype.supportsAtomicSnapshotCommit;
    vi.spyOn(prototype, "supportsAtomicSnapshotCommit").mockImplementationOnce(async function (this: ReturnType<typeof createLocalVaultAdapter>) { const result = await support.call(this); setSyncProvider("google-drive"); return result; });
    await expect(dataClient.archiveDocument("local-space", page.id)).rejects.toThrow(/storage changed|adapter changed|context changed/i);
    setSyncProvider("none");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents.find(row => row.id === page.id)?.archived_at).toBeUndefined();
    for (const [key, value] of before) expect(storage.get(key)).toBe(value);
  });

  function remoteLifecycleManifest(id: string, action: "archive" | "trash" | "restore" | "unarchive") {
    const envelope = buildProviderRemoteMutationEnvelope(buildProviderRemoteMutationPlan("google-drive", "Google Drive", "push", {
      status: "enabled", root_strategy: "drive-appdata-or-user-selected-folder", auth_model: "oauth2-offline-access", required_scopes: ["drive.file"], write_model: "local-sqlite-first-then-provider-api-push", conflict_model: "provider-revision-plus-local-replay-queue-validation",
    }, { provider: "google-drive", connected: true, account_label: "Generated account", scopes: ["drive.file"], sync_mode: "manual" }), "2026-07-09T12:00:00.000Z", `remote-lifecycle-${action}`);
    return buildProviderRemoteSyncManifest(envelope, { schema_version: 1, operation_count: 1, operations: [{ id: `remote-${action}`, entity_type: "document", entity_id: id, action, payload_status: "available", payload: { space_id: "local-space", archived_at: "remote-archive", deleted_at: "remote-trash" } }] });
  }

  it.each(["archive", "trash", "restore", "unarchive"] as const)("keeps remote %s lifecycle transport unsupported with audited rejection and no mutation or echo", async action => {
    const page = await dataClient.createDocument("local-space", { title: "Remote lifecycle" });
    const child = await dataClient.createDocument("local-space", { title: "Child", parent_id: page.id });
    if (action === "restore") await dataClient.deleteDocument("local-space", page.id);
    if (action === "unarchive" || action === "trash") await dataClient.archiveDocument("local-space", page.id);
    await dataClient.clearQueuedSyncOperations();
    const before = await createLocalVaultAdapter("none").loadSnapshot();
    const result = await dataClient.applyProviderRemoteImportManifest(remoteLifecycleManifest(page.id, action));
    expect(result.status).toBe("blocked"); expect(result.applied_operation_count).toBe(0);
    expect(result.results).toEqual([expect.objectContaining({ operation_id: `remote-${action}`, status: "blocked", message: `Remote replay operation document:${action} is not supported by local provider import` })]);
    const after = await createLocalVaultAdapter("none").loadSnapshot();
    expect(after.documents).toEqual(before.documents);
    expect(after.documents.find(doc => doc.id === child.id)).toEqual(before.documents.find(doc => doc.id === child.id));
    expect(after.drafts).toEqual(before.drafts); expect(after.assets).toEqual(before.assets);
    expect(after.sync_state?.queued_operations).toEqual([]);
    expect(await dataClient.getSyncState()).toBe("conflict");
  });

  it("moves a local document to Trash while retaining drafts for restore", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Disposable draft",
      content: "temporary",
    });
    expect(await dataClient.listDocumentDrafts("local-space", created.id)).toHaveLength(1);

    await dataClient.deleteDocument("local-space", created.id);

    await expect(dataClient.getDocument("local-space", created.id)).rejects.toThrow(
      "Document not found",
    );
    expect(await dataClient.listDocumentDrafts("local-space", created.id)).toHaveLength(1);

    expect(await dataClient.listTrashedDocuments("local-space")).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: created.id, deleted_at: expect.any(String) }),
      ]),
    );
    await dataClient.restoreDocument("local-space", created.id);
    await expect(dataClient.getDocument("local-space", created.id)).resolves.toMatchObject({
      id: created.id,
    });
  });

  it("applies matching template tags and property values to local documents", async () => {
    const property = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "select",
      options: ["Draft"],
    });
    const tag = await dataClient.createTag("local-space", { name: "Planning", color: "#2563eb" });
    const template = await dataClient.createTemplate("local-space", {
      name: "Project brief",
      content: "# Brief",
      tags: ["planning", "missing"],
      properties: { status: "Draft", ignored: null },
    });

    const document = await dataClient.createDocument("local-space", {
      title: "Launch plan",
      template_id: template.id,
      content: "# Brief",
    });

    await expect(dataClient.listDocumentTags("local-space", document.id)).resolves.toEqual([
      expect.objectContaining({ id: tag.id, name: "Planning" }),
    ]);
    const documentProperties = await dataClient.listDocumentProperties("local-space", document.id);
    expect(documentProperties).toEqual([
      expect.objectContaining({ property_id: property.id, value: "Draft" }),
    ]);
    await expect(
      dataClient.searchDocuments("", 10, { has_tag: "true", has_property: "true" }),
    ).resolves.toEqual(
      expect.arrayContaining([expect.objectContaining({ document_id: document.id })]),
    );
    const queued = await dataClient.listQueuedSyncOperations();
    const documentCreate = queued.find(
      (operation) => operation.entity_type === "document" && operation.entity_id === document.id,
    );
    const tagCreate = queued.find(
      (operation) =>
        operation.entity_type === "document_tag" &&
        operation.entity_id === `${document.id}:${tag.id}`,
    );
    const propertyCreate = queued.find(
      (operation) =>
        operation.entity_type === "document_property_values" && operation.entity_id === document.id,
    );
    expect(tagCreate?.payload).toEqual({
      document_id: document.id,
      tag_id: tag.id,
      space_id: "local-space",
    });
    expect(propertyCreate?.payload).toEqual(
      expect.objectContaining({
        document_id: document.id,
        space_id: "local-space",
        values: [expect.objectContaining({ property_id: property.id, value: "Draft" })],
      }),
    );
    expect((documentCreate?.queued_at ?? "") < (tagCreate?.queued_at ?? "")).toBe(true);
    expect((tagCreate?.queued_at ?? "") < (propertyCreate?.queued_at ?? "")).toBe(true);
  });

  it("retains guidance in editable drafts and removes only active instructions on publication", async () => {
    const guidance = serializeTemplateMarker({
      kind: "guidance",
      id: "summary",
      text: "Describe the impact.",
    });
    const body = `Keep authored prose.\n${guidance}\n\`\`\`markdown\n${guidance}\n\`\`\`\n`;
    const created = await dataClient.createDocument("local-space", {
      title: "Guided page",
      content: body,
    });
    expect(created.draft_id).toBeTruthy();
    const draft = await dataClient.getDocumentDraft("local-space", created.id, created.draft_id!);
    expect(draft.content).toBe(body);
    await dataClient.publishDocumentDraft("local-space", created.id, created.draft_id!);
    const published = await dataClient.getDocumentContent("local-space", created.id);
    expect(published.content).toContain("Keep authored prose.");
    expect(published.content.split(guidance)).toHaveLength(2);
    expect(published.content).toContain(`\`\`\`markdown\n${guidance}`);
  });

  it("renames and deletes local folders while preserving their documents", async () => {
    const folder = await dataClient.createFolder("local-space", "Projects");
    await dataClient.updateFolder("local-space", folder.id, { name: "Archive" });

    let folders = await dataClient.listFolders("local-space");
    expect(folders.find((item) => item.id === folder.id)?.name).toBe("Archive");

    const created = await dataClient.createDocument("local-space", {
      title: "Nested",
      folder_id: folder.id,
    });

    await dataClient.deleteFolder("local-space", folder.id);
    folders = await dataClient.listFolders("local-space");
    expect(folders.some((item) => item.id === folder.id)).toBe(false);
    await expect(dataClient.getDocument("local-space", created.id)).resolves.toMatchObject({
      id: created.id,
      folder_id: undefined,
    });
  });

  it("rolls back folder deletion and its queue when durable storage rejects the write", async () => {
    const parent = await dataClient.createFolder("local-space", "Parent");
    const child = await dataClient.createFolder("local-space", "Child", { parent_id: parent.id });
    const page = await dataClient.createDocument("local-space", {
      title: "Retained draft",
      content: "Retained body",
      folder_id: child.id,
    });
    const before = await dataClient.exportLocalVaultSnapshot();
    const queued = await dataClient.listQueuedSyncOperations();
    const disk = new Map(storage);
    const original = window.localStorage.setItem;
    window.localStorage.setItem = () => {
      throw new Error("QuotaExceededError");
    };
    try {
      await expect(dataClient.deleteFolder("local-space", parent.id)).rejects.toThrow(
        "QuotaExceededError",
      );
    } finally {
      window.localStorage.setItem = original;
    }
    const unchanged = await dataClient.exportLocalVaultSnapshot();
    expect({ ...unchanged, sync_state: undefined }).toEqual({ ...before, sync_state: undefined });
    expect(await dataClient.listQueuedSyncOperations()).toEqual(queued);
    expect(storage).toEqual(disk);
    await expect(dataClient.getDocument("local-space", page.id)).resolves.toMatchObject({
      folder_id: child.id,
    });
    await dataClient.deleteFolder("local-space", parent.id);
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(after.folders.some((item) => item.id === parent.id || item.id === child.id)).toBe(false);
    expect(after.documents).toEqual(
      before.documents.map((item) =>
        item.id === page.id ? { ...item, folder_id: undefined } : item,
      ),
    );
    expect(after.drafts).toEqual(before.drafts);
    expect(
      (await dataClient.listQueuedSyncOperations()).filter(
        (item) => item.entity_type === "folder" && item.action === "delete",
      ),
    ).toHaveLength(1);
  });

  it("retains concurrent edits while refusing new page admission under a durably deleted folder", async () => {
    const folder = await dataClient.createFolder("local-space", "Concurrent");
    const page = await dataClient.createDocument("local-space", {
      title: "Before",
      content: "Keep body",
      folder_id: folder.id,
    });
    let acknowledge!: () => void;
    let committed!: () => void;
    const gate = new Promise<void>((resolve) => {
      acknowledge = resolve;
    });
    const started = new Promise<void>((resolve) => {
      committed = resolve;
    });
    const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(
      createLocalVaultAdapter("none"),
    );
    const original = prototype.commitSnapshotMutation;
    vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (
      this: ReturnType<typeof createLocalVaultAdapter>,
      mutate: VaultSnapshotMutationBuilder,
    ) {
      const saved = await original.call(this, mutate);
      committed();
      await gate;
      return saved;
    });
    const deletion = dataClient.deleteFolder("local-space", folder.id);
    await started;
    const edit = dataClient.updateDocument("local-space", page.id, { title: "Concurrent title" });
    const creation = dataClient.createDocument("local-space", {
      title: "Concurrent new page",
      content: "New body",
      folder_id: folder.id,
    }).then(() => null, error => error as Error);
    const childCreation = dataClient.createFolder("local-space", "Concurrent child", {
      parent_id: folder.id,
    }).then(() => null, error => error as Error);
    // Metadata edits publish caches only after the durable acknowledgement.
    // Other preexisting creation families may still stage cache records.
    try { expect((await dataClient.getDocument("local-space", page.id)).title).toBe("Before"); }
    finally { acknowledge(); }
    await deletion;
    await edit;
    expect(await creation).toMatchObject({ message: "Folder not found" });
    expect(await childCreation).toMatchObject({ message: "Folder not found" });
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    expect(snapshot.documents.find((item) => item.id === page.id)).toMatchObject({
      title: "Concurrent title",
      content: "Keep body",
    });
    expect(snapshot.documents.find((item) => item.id === page.id)?.folder_id).toBeUndefined();
    expect(snapshot.documents.some(item => item.title === "Concurrent new page")).toBe(false);
    expect(snapshot.drafts?.some(draft => draft.title === "Concurrent new page")).toBe(false);
    expect(snapshot.folders.some((item) => item.name === "Concurrent child")).toBe(false);
    expect(snapshot.sync_state?.queued_operations.some(operation => operation.entity_type === "folder" && (operation.payload as { name?: string } | undefined)?.name === "Concurrent child")).toBe(false);
    expect(snapshot.folders.some((item) => item.id === folder.id)).toBe(false);
    const durable = await createLocalVaultAdapter("none").loadSnapshot();
    expect(durable.documents.find((item) => item.id === page.id)?.title).toBe("Concurrent title");

  });

  it("does not republish a permanently deleted page while folder acknowledgement is held", async () => {
    const folder = await dataClient.createFolder(
      "local-space",
      "Pending deletion",
    );
    const page = await dataClient.createDocument("local-space", {
      title: "Permanently removed",
      content: "Remove body",
      folder_id: folder.id,
    });
    const schema = await dataClient.createPropertySchema("local-space", {
      name: "Remove property",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", page.id, [
      { property_id: schema.id, value: "Remove value" },
    ]);
    const tag = await dataClient.createTag("local-space", {
      name: "Remove tag",
    });
    await dataClient.addDocumentTag("local-space", page.id, tag.id);
    const workflow = await dataClient.createWorkflowTemplate("local-space", {
      name: "Remove workflow",
      stages: [
        { id: "review", name: "Review", sort_order: 0 },
        { id: "done", name: "Done", sort_order: 1 },
      ],
    });
    await dataClient.enableDocumentWorkflow("local-space", page.id, {
      template_id: workflow.id,
    });
    const before = await dataClient.exportLocalVaultSnapshot();
    expect(before.drafts?.some((item) => item.document_id === page.id)).toBe(
      true,
    );
    expect(
      before.document_workflow_events?.some(
        (item) => item.document_id === page.id,
      ),
    ).toBe(true);
    let acknowledge!: () => void;
    let committed!: () => void;
    const gate = new Promise<void>((resolve) => {
      acknowledge = resolve;
    });
    const started = new Promise<void>((resolve) => {
      committed = resolve;
    });
    const prototype: ReturnType<typeof createLocalVaultAdapter> =
      Object.getPrototypeOf(createLocalVaultAdapter("none"));
    const original = prototype.commitSnapshotMutation;
    vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(
      async function (
        this: ReturnType<typeof createLocalVaultAdapter>,
        mutate: VaultSnapshotMutationBuilder,
      ) {
        const saved = await original.call(this, mutate);
        committed();
        await gate;
        return saved;
      },
    );
    const deletion = dataClient.deleteFolder("local-space", folder.id);
    let trash: Promise<void> | undefined;
    let permanent: Promise<void> | undefined;
    try {
      await started;
      trash = dataClient.deleteDocument("local-space", page.id);
      // The queued lifecycle operation cannot publish a tentative Trash row.
      expect((await dataClient.listTrashedDocuments("local-space")).some(row => row.id === page.id)).toBe(false);
      acknowledge();
      await trash;
      expect(await dataClient.listTrashedDocuments("local-space")).toEqual(
        expect.arrayContaining([expect.objectContaining({ id: page.id })]),
      );
      const originalTrash = (await dataClient.listTrashedDocuments("local-space")).find(row => row.id === page.id)!;
      permanent = dataClient.permanentlyDeleteDocument("local-space", page.id, {created_at: originalTrash.created_at, deleted_at: originalTrash.deleted_at});
      await expect(
        dataClient.getDocument("local-space", page.id),
      ).rejects.toThrow("Document not found");
    } finally {
      acknowledge();
      await Promise.all([deletion, trash, permanent]);
    }
    await expect(
      dataClient.getDocument("local-space", page.id),
    ).rejects.toThrow("Document not found");
    const durable = await createLocalVaultAdapter("none").loadSnapshot();
    expect(durable.documents.some((item) => item.id === page.id)).toBe(false);
    expect(durable.folders.some((item) => item.id === folder.id)).toBe(false);
    for (const rows of [
      durable.drafts,
      durable.property_values,
      durable.doc_tags,
      durable.document_workflows,
      durable.document_workflow_events,
    ])
      expect(rows?.some((item) => item.document_id === page.id)).toBe(false);
    const queue = await dataClient.listQueuedSyncOperations();
    expect(
      queue.filter(
        (item) =>
          item.entity_type === "folder" &&
          item.entity_id === folder.id &&
          item.action === "delete",
      ),
    ).toHaveLength(1);
    expect(
      queue.filter(
        (item) =>
          item.entity_type === "document" &&
          item.entity_id === page.id &&
          item.action === "trash",
      ),
    ).toHaveLength(1);
    expect(
      queue.filter(
        (item) =>
          item.entity_type === "document" &&
          item.entity_id === page.id &&
          item.action === "delete",
      ),
    ).toHaveLength(1);
  });

  it.each(["cycle", "foreign-child", "foreign-page", "locked", "archived", "trashed"])(
    "rejects current durable corrupt/inactive folder inventory: %s",
    async (corruption) => {
      const folder = await dataClient.createFolder("local-space", "Protected");
      const page = await dataClient.createDocument("local-space", {
        title: "Keep",
        folder_id: folder.id,
      });
      await createLocalVaultAdapter("none").commitSnapshotMutation((snapshot) => {
        if (corruption === "cycle")
          snapshot.folders.find((item) => item.id === folder.id)!.parent_id = folder.id;
        if (corruption === "foreign-child")
          snapshot.folders.push({
            id: "foreign-child",
            space_id: "foreign",
            name: "Foreign",
            parent_id: folder.id,
          });
        const document = snapshot.documents.find((item) => item.id === page.id)!;
        if (corruption === "foreign-page") document.space_id = "foreign";
        if (corruption === "locked") document.locked = true;
        if (corruption === "archived") document.archived_at = "2026-01-01";
        if (corruption === "trashed") document.deleted_at = "2026-01-01";
        return { snapshot };
      });
      const before = await dataClient.exportLocalVaultSnapshot();
      const queue = await dataClient.listQueuedSyncOperations();
      const disk = new Map(storage);
      await expect(dataClient.deleteFolder("local-space", folder.id)).rejects.toThrow();
      const after = await dataClient.exportLocalVaultSnapshot();
      expect({ ...after, sync_state: undefined }).toEqual({ ...before, sync_state: undefined });
      expect(storage).toEqual(disk);
      expect(await dataClient.listQueuedSyncOperations()).toEqual(queue);
      expect(
        (await dataClient.listFolders("local-space")).some((item) => item.id === folder.id),
      ).toBe(true);
    },
  );

  it("preserves newer durable page and folder placements outside the deleted subtree", async () => {
    const old = await dataClient.createFolder("local-space", "Old");
    const destination = await dataClient.createFolder("local-space", "Destination");
    const child = await dataClient.createFolder("local-space", "Moved child", {
      parent_id: old.id,
    });
    const page = await dataClient.createDocument("local-space", {
      title: "Moved page",
      folder_id: old.id,
    });
    await createLocalVaultAdapter("none").commitSnapshotMutation((snapshot) => {
      snapshot.folders.find((folder) => folder.id === child.id)!.parent_id = destination.id;
      snapshot.documents.find((document) => document.id === page.id)!.folder_id = destination.id;
      return { snapshot };
    });
    await dataClient.deleteFolder("local-space", old.id);
    await dataClient.updateDocument("local-space", "local-welcome", { title: "Unrelated edit" });
    const durable = await dataClient.exportLocalVaultSnapshot();
    expect(durable.documents.find((document) => document.id === page.id)?.folder_id).toBe(
      destination.id,
    );
    expect(durable.folders.find((folder) => folder.id === child.id)?.parent_id).toBe(
      destination.id,
    );
  });

  it("preserves folder nesting beyond three levels", async () => {
    const root = await dataClient.createFolder("local-space", "Root");
    const child = await dataClient.createFolder("local-space", "Child", {
      parent_id: root.id,
    });
    const grandchild = await dataClient.createFolder("local-space", "Grandchild", {
      parent_id: child.id,
    });

    const created = await dataClient.createDocument("local-space", {
      title: "Level Three Page",
      folder_id: grandchild.id,
    });
    if (!created.draft_id) throw new Error("Expected an initial document draft");
    await dataClient.publishDocumentDraft("local-space", created.id, created.draft_id);
    await expect(
      dataClient.listDocuments("local-space", { folder_id: grandchild.id }),
    ).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: created.id, folder_id: grandchild.id }),
      ]),
    );
    await expect(
      dataClient.createFolder("local-space", "Level Four", { parent_id: grandchild.id }),
    ).resolves.toMatchObject({ parent_id: grandchild.id });

    await dataClient.updateFolder("local-space", child.id, { parent_id: null });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: child.id, parent_id: undefined })]),
    );
  });

  it.each([4, 32, 256])("preserves a %i-level folder journey through ordinary writes, export/import and cold adapter reload", async depth => {
    const page = await dataClient.createDocument("local-space", { title: "Deep body", content: "Exact\r\nraw source <!--starlane:css-->\nblocked url(x)\n<!--/starlane:css-->" });
    const imported = await dataClient.exportLocalVaultSnapshot();
    imported.folders = Array.from({ length: depth }, (_, n) => ({ id: `deep-${n}`, space_id: "local-space", name: `Level ${n}`, parent_id: n ? `deep-${n - 1}` : undefined }));
    await dataClient.importLocalVaultSnapshot(imported);
    await dataClient.moveDocument("local-space", page.id, { target_space_id: "local-space", folder_id: `deep-${depth - 1}`, parent_id: null });
    const before = await dataClient.exportLocalVaultSnapshot();
    const child = await dataClient.createFolder("local-space", "Preserved child", { source_id: "imported-child", parent_id: `deep-${depth - 1}` });
    await dataClient.updateFolder("local-space", `deep-${depth - 1}`, { name: "Renamed leaf" });
    await dataClient.moveFolder("local-space", "deep-1", { target_space_id: "local-space", parent_id: null });
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(child).toMatchObject({ id: "imported-child", parent_id: `deep-${depth - 1}` });
    expect(after.documents).toEqual(before.documents);
    expect(after.drafts).toEqual(before.drafts);
    expect(after.assets).toEqual(before.assets);
    expect(after.folders.find(row => row.id === "deep-1")?.parent_id).toBeUndefined();
    expect(after.folders.find(row => row.id === `deep-${depth - 1}`)?.name).toBe("Renamed leaf");
    const cold = await createLocalVaultAdapter("none").loadSnapshot();
    expect(cold.folders).toEqual(after.folders);
    expect(cold.documents).toEqual(after.documents);
    expect(cold.drafts).toEqual(after.drafts);
    await dataClient.importLocalVaultSnapshot(after);
    expect((await dataClient.exportLocalVaultSnapshot()).folders).toEqual(after.folders);
    const queue = await dataClient.listQueuedSyncOperations();
    expect(queue.filter(op => op.entity_id === "imported-child" && op.action === "create")).toHaveLength(1);
  });

  it("refuses broad operation budget overflow before changing durable/cache state or queue, ignoring unrelated nodes", async () => {
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    snapshot.folders = [{ id: "broad", space_id: "local-space", name: "Broad" }, ...Array.from({ length: 10_000 }, (_, n) => ({ id: `b-${n}`, space_id: "local-space", name: `Child ${n}`, parent_id: "broad" }))];
    await dataClient.importLocalVaultSnapshot(snapshot);
    const before = await createLocalVaultAdapter("none").loadSnapshot();
    const disk = new Map(storage);
    await expect(dataClient.updateFolder("local-space", "broad", { parent_id: null })).rejects.toThrow("10000 participating nodes");
    await expect(dataClient.deleteFolder("local-space", "broad")).rejects.toThrow("10000 participating nodes");
    const refused = await createLocalVaultAdapter("none").loadSnapshot();
    expect(refused).toEqual({ ...before, sync_state: { ...before.sync_state, updated_at: refused.sync_state?.updated_at } });
    expect(storage).toEqual(disk);
    expect((await dataClient.listFolders("local-space")).find(row => row.id === "broad")).toMatchObject({ name: "Broad" });
    await expect(dataClient.createFolder("local-space", "Unrelated root")).resolves.toMatchObject({ name: "Unrelated root" });
  });

  it("keeps prior source, cache and queue when an atomic folder write fails or latest source changes", async () => {
    const folder = await dataClient.createFolder("local-space", "Original");
    const adapter = createLocalVaultAdapter("none");
    const prototype = Object.getPrototypeOf(adapter) as ReturnType<typeof createLocalVaultAdapter>;
    const before = await adapter.loadSnapshot();
    const disk = new Map(storage);
    vi.spyOn(prototype, "commitSnapshotMutation").mockRejectedValueOnce(new Error("Disk full"));
    await expect(dataClient.updateFolder("local-space", folder.id, { name: "Unsaved" })).rejects.toThrow("Disk full");
    const refused = await adapter.loadSnapshot();
    expect(refused).toEqual({ ...before, sync_state: { ...before.sync_state, updated_at: refused.sync_state?.updated_at } });
    expect(storage).toEqual(disk);
    expect(await dataClient.listFolders("local-space")).toContainEqual(expect.objectContaining({ id: folder.id, name: "Original" }));
    await adapter.commitSnapshotMutation(snapshot => ({ snapshot: { ...snapshot, folders: snapshot.folders.map(row => row.id === folder.id ? { ...row, name: "Other window" } : row) } }));
    const newer = await adapter.loadSnapshot();
    await expect(dataClient.updateFolder("local-space", folder.id, { name: "Stale overwrite" })).rejects.toThrow("changed in another window");
    const staleRefused = await adapter.loadSnapshot();
    expect(staleRefused).toEqual({ ...newer, sync_state: { ...newer.sync_state, updated_at: staleRefused.sync_state?.updated_at } });
  });

  it("retains an actually committed folder and its one operation after lost acknowledgement without publishing tentative cache", async () => {
    await dataClient.listSpaces();
    const adapter = createLocalVaultAdapter("none");
    const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(adapter);
    const original = prototype.commitSnapshotMutation;
    vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (this: ReturnType<typeof createLocalVaultAdapter>, mutate: VaultSnapshotMutationBuilder) {
      await original.call(this, mutate);
      throw new Error("Acknowledgement lost");
    });
    await expect(dataClient.createFolder("local-space", "Committed", { source_id: "lost-ack-folder" })).rejects.toThrow("Acknowledgement lost");
    const cold = await adapter.loadSnapshot();
    expect(cold.folders).toContainEqual(expect.objectContaining({ id: "lost-ack-folder", name: "Committed" }));
    expect((await dataClient.listFolders("local-space")).some(row => row.id === "lost-ack-folder")).toBe(false);
    await expect(dataClient.createFolder("local-space", "Committed", { source_id: "lost-ack-folder" })).rejects.toThrow("already exists");
    expect((await adapter.loadSnapshot()).sync_state?.queued_operations.filter(op => op.entity_id === "lost-ack-folder")).toHaveLength(1);
  });

  it.each([4, 32, 256])("preserves %i-level child pages through actual create/move/startup/export/import and cold adapter", async depth => {
    const seed = await dataClient.exportLocalVaultSnapshot();
    seed.documents = Array.from({ length: depth }, (_, n) => ({ id: `page-${n}`, space_id: "local-space", title: `Page ${n}`, content: `Raw ${n}\r\n<!--original-->`, parent_id: n ? `page-${n - 1}` : undefined, published_at: "old", updated_at: "old" }));
    seed.drafts = []; seed.sync_state!.queued_operations = [];
    await dataClient.importLocalVaultSnapshot(seed);
    const child = await dataClient.createDocument("local-space", { source_id: "exact-child", title: "Private child", content: "Exact new body\r\n<!--starlane:css-->unsafe url(x)<!--/starlane:css-->", parent_id: `page-${depth - 1}` });
    const before = await createLocalVaultAdapter("none").loadSnapshot();
    const target = await dataClient.createDocument("local-space", { title: "New parent" });
    await dataClient.moveDocument("local-space", "page-1", { target_space_id: "local-space", parent_id: target.id });
    const cold = await createLocalVaultAdapter("none").loadSnapshot();
    expect(cold.documents.find(row => row.id === "page-1")?.parent_id).toBe(target.id);
    expect(cold.documents.find(row => row.id === child.id)?.parent_id).toBe(`page-${depth - 1}`);
    expect(cold.drafts?.find(row => row.id === child.draft_id)).toEqual(before.drafts?.find(row => row.id === child.draft_id));
    for (const page of before.documents) expect(cold.documents.find(row => row.id === page.id)?.content).toBe(page.content);
    expect(cold.assets).toEqual(before.assets);
    const startup = await runLocalVaultStartupCheck("none");
    expect(startup.repairs).toEqual([]);
    const reopened = await createLocalVaultAdapter("none").loadSnapshot();
    expect(reopened.documents).toEqual(cold.documents);
    await dataClient.importLocalVaultSnapshot(reopened);
    expect((await dataClient.exportLocalVaultSnapshot()).documents).toEqual(cold.documents);
  });

  it("refuses 10001-page participation without changing bytes/queue while allowing an unrelated root", async () => {
    const seed = await dataClient.exportLocalVaultSnapshot();
    seed.documents = Array.from({ length: 10_001 }, (_, n) => ({ id: `budget-page-${n}`, space_id: "local-space", title: `Page ${n}`, content: "Keep", parent_id: n ? "budget-page-0" : undefined, published_at: "old", updated_at: "old" }));
    seed.drafts = []; seed.sync_state!.queued_operations = [];
    await dataClient.importLocalVaultSnapshot(seed);
    const disk = new Map(storage);
    await expect(dataClient.moveDocument("local-space", "budget-page-0", { target_space_id: "local-space", parent_id: "budget-page-1" })).rejects.toThrow("10000 participating nodes");
    expect(storage).toEqual(disk);
    expect((await dataClient.listQueuedSyncOperations())).toEqual([]);
    await expect(dataClient.createDocument("local-space", { title: "Unrelated new root" })).resolves.toHaveProperty("id");
  });

  it.each([9999, 10000])("counts the original source and new sibling in a %i-page duplicate boundary", async sourceParticipants => {
    const seed = await dataClient.exportLocalVaultSnapshot();
    seed.documents = Array.from({ length: sourceParticipants }, (_, n) => ({ id: `copy-boundary-${n}`, space_id: "local-space", title: `Page ${n}`, content: `Exact original ${n}\r\n`, parent_id: n ? `copy-boundary-${n - 1}` : undefined, published_at: "old", updated_at: "old" }));
    seed.drafts = []; seed.sync_state!.queued_operations = [];
    await dataClient.importLocalVaultSnapshot(seed);
    const disk = new Map(storage);
    const sourceId = `copy-boundary-${sourceParticipants - 1}`;
    const sourceBefore = (await createLocalVaultAdapter("none").loadSnapshot()).documents.find(page => page.id === sourceId);
    if (sourceParticipants === 10000) {
      await expect(dataClient.duplicateDocument("local-space", sourceId)).rejects.toThrow("10000 participating nodes");
      expect(storage).toEqual(disk);
      expect((await dataClient.listQueuedSyncOperations())).toEqual([]);
      expect((await dataClient.listDocuments("local-space"))).toHaveLength(sourceParticipants);
    } else {
      const copy = await dataClient.duplicateDocument("local-space", sourceId);
      const cold = await createLocalVaultAdapter("none").loadSnapshot();
      expect(cold.documents).toHaveLength(10000);
      expect(cold.documents.find(page => page.id === copy.id)).toMatchObject({ parent_id: `copy-boundary-${sourceParticipants - 2}`, content: `Exact original ${sourceParticipants - 1}\r\n` });
      expect(cold.documents.find(page => page.id === sourceId)).toEqual(sourceBefore);
      expect(cold.sync_state?.queued_operations).toHaveLength(1);
    }
  });

  it("counts the proposed new page against a full 10000-parent chain before any write", async () => {
    const seed = await dataClient.exportLocalVaultSnapshot();
    seed.documents = Array.from({ length: 10000 }, (_, n) => ({ id: `full-${n}`, space_id: "local-space", title: `Page ${n}`, content: "Exact", parent_id: n ? `full-${n - 1}` : undefined, published_at: "old", updated_at: "old" }));
    seed.drafts = []; seed.sync_state!.queued_operations = [];
    await dataClient.importLocalVaultSnapshot(seed);
    const disk = new Map(storage);
    await expect(dataClient.createDocument("local-space", { source_id: "one-too-many", title: "Held", parent_id: "full-9999" })).rejects.toThrow("10000 participating nodes");
    expect(storage).toEqual(disk);
    const cold = await createLocalVaultAdapter("none").loadSnapshot();
    expect(cold.documents).toHaveLength(10000); expect(cold.drafts).toEqual([]);
    expect(cold.sync_state?.queued_operations).toEqual([]);
  });

  it.each(["locked", "archived", "private", "parent-edge", "orphan", "cycle"])("rechecks %s parent state in the durable creation commit and publishes no tentative page/draft/queue", async failure => {
    const parent = await dataClient.createDocument("local-space", { title: "Parent" });
    const other = await dataClient.createDocument("local-space", { title: "Other" });
    const adapter = createLocalVaultAdapter("none");
    await adapter.commitSnapshotMutation(snapshot => {
      const page = snapshot.documents.find(row => row.id === parent.id)!;
      if (failure === "locked") page.locked = true;
      if (failure === "archived") page.archived_at = "archived";
      if (failure === "private") snapshot.drafts!.find(row => row.document_id === parent.id)!.owner_id = "another-user";
      if (failure === "parent-edge") page.parent_id = other.id;
      if (failure === "orphan") page.parent_id = "missing";
      if (failure === "cycle") page.parent_id = parent.id;
      return { snapshot };
    });
    const disk = new Map(storage), before = await adapter.loadSnapshot();
    await expect(dataClient.createDocument("local-space", { source_id: "rejected-child", title: "Rejected", parent_id: parent.id })).rejects.toThrow();
    expect(storage).toEqual(disk);
    const after = await adapter.loadSnapshot();
    expect(after.documents).toEqual(before.documents); expect(after.drafts).toEqual(before.drafts);
    expect(after.sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
    expect((await dataClient.listDocuments("local-space", { include_nested: true })).some(page => page.id === "rejected-child")).toBe(false);
  });

  it("commits creation with template values and all replay entries together; failed storage admits none", async () => {
    await dataClient.createPropertySchema("local-space", { name: "Status", property_type: "text" });
    const tag = await dataClient.createTag("local-space", { name: "Tag" });
    const template = await dataClient.createTemplate("local-space", { name: "Template", content: "Body", properties: { Status: "Exact value" }, tags: ["Tag"] });
    const adapter = createLocalVaultAdapter("none"), prototype = Object.getPrototypeOf(adapter) as ReturnType<typeof createLocalVaultAdapter>;
    const before = await adapter.loadSnapshot(), disk = new Map(storage);
    vi.spyOn(prototype, "commitSnapshotMutation").mockRejectedValueOnce(new Error("Quota exceeded"));
    await expect(dataClient.createDocument("local-space", { source_id: "atomic-page", title: "Page", template_id: template.id, content: "Unsaved exact\r\nsource" })).rejects.toThrow("Quota exceeded");
    expect(storage).toEqual(disk);
    expect((await adapter.loadSnapshot()).documents).toEqual(before.documents);
    expect((await adapter.loadSnapshot()).drafts).toEqual(before.drafts);
    expect((await dataClient.listDocuments("local-space")).some(page => page.id === "atomic-page")).toBe(false);
    const created = await dataClient.createDocument("local-space", { source_id: "atomic-page", title: "Page", template_id: template.id, content: "Saved exact\r\nsource" });
    const saved = await createLocalVaultAdapter("none").loadSnapshot();
    expect(saved.drafts?.find(row => row.id === created.draft_id)?.content).toBe("Saved exact\r\nsource");
    expect(saved.doc_tags).toContainEqual({ document_id: created.id, tag_id: tag.id });
    expect(saved.property_values).toContainEqual(expect.objectContaining({ document_id: created.id, value: "Exact value" }));
    const operations = saved.sync_state!.queued_operations.filter(op => op.entity_id === created.id || op.entity_id === `${created.id}:${tag.id}`);
    expect(operations.map(op => op.action)).toEqual(["create", "add", "set"]);
    expect(new Set(operations.map(op => op.id)).size).toBe(3);
  });

  it("retains one committed page/draft/create operation after lost acknowledgement without exposing it in cache", async () => {
    await dataClient.listSpaces();
    const adapter = createLocalVaultAdapter("none"), prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(adapter), original = prototype.commitSnapshotMutation;
    vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (this: ReturnType<typeof createLocalVaultAdapter>, mutate: VaultSnapshotMutationBuilder) {
      await original.call(this, mutate); throw new Error("ACK lost");
    });
    await expect(dataClient.createDocument("local-space", { source_id: "lost-page", title: "Retained", content: "Original body" })).rejects.toThrow("ACK lost");
    const saved = await createLocalVaultAdapter("none").loadSnapshot();
    expect(saved.documents.filter(page => page.id === "lost-page")).toHaveLength(1);
    expect(saved.drafts?.filter(draft => draft.document_id === "lost-page")).toHaveLength(1);
    expect(saved.sync_state?.queued_operations.filter(op => op.entity_id === "lost-page")).toHaveLength(1);
    expect((await dataClient.listDocuments("local-space")).some(page => page.id === "lost-page")).toBe(false);
    await expect(dataClient.createDocument("local-space", { source_id: "lost-page", title: "Retained" })).rejects.toThrow("already exists");
    expect((await adapter.loadSnapshot()).drafts?.filter(draft => draft.document_id === "lost-page")).toHaveLength(1);
  });

  it("blocks local folder moves into descendants", async () => {
    const root = await dataClient.createFolder("local-space", "Root");
    const child = await dataClient.createFolder("local-space", "Child", {
      parent_id: root.id,
    });

    await expect(
      dataClient.updateFolder("local-space", root.id, { parent_id: child.id }),
    ).rejects.toThrow("Folder cannot be moved inside one of its descendants");
  });

  it.each(["folder", "document"] as const)(
    "rejects unreviewed local %s subtree moves without changing data or recovery work",
    async (kind) => {
      const targetSpace = await dataClient.createSpace("Archive");
      const sourceFolder = await dataClient.createFolder("local-space", "Projects");
      const childFolder = await dataClient.createFolder("local-space", "Launch", {
        parent_id: sourceFolder.id,
      });
      const parent = await dataClient.createDocument("local-space", {
        title: "Parent",
        content: "Keep parent",
        folder_id: childFolder.id,
      });
      const child = await dataClient.createDocument("local-space", {
        title: "Child",
        content: "Keep child",
        parent_id: parent.id,
        folder_id: childFolder.id,
      });
      const property = await dataClient.createPropertySchema("local-space", {
        name: "Priority",
        property_type: "number",
      });
      await dataClient.setDocumentPropertyValues("local-space", child.id, [
        { property_id: property.id, value: "01" },
      ]);
      const tag = await dataClient.createTag("local-space", { name: "Planning", color: "#2563eb" });
      await dataClient.addDocumentTag("local-space", child.id, tag.id);
      const workflow = await dataClient.createWorkflowTemplate("local-space", {
        name: "Review",
        stages: [
          { id: "review", name: "Review", sort_order: 0 },
          { id: "done", name: "Done", sort_order: 1 },
        ],
      });
      await dataClient.enableDocumentWorkflow("local-space", child.id, {
        template_id: workflow.id,
      });
      await dataClient.transitionDocumentWorkflow("local-space", child.id, { stage_id: "done" });
      const stableSnapshot = async () => {
        const snapshot = await dataClient.exportLocalVaultSnapshot();
        return {
          ...snapshot,
          sync_state: snapshot.sync_state ? { ...snapshot.sync_state, updated_at: "" } : undefined,
        };
      };
      const before = await stableSnapshot();
      const storedBefore = new Map(storage);
      const move =
        kind === "folder"
          ? dataClient.moveFolder("local-space", sourceFolder.id, {
              target_space_id: targetSpace.id,
              parent_id: null,
            })
          : dataClient.moveDocument("local-space", parent.id, {
              target_space_id: targetSpace.id,
              folder_id: null,
              parent_id: null,
            });
      await expect(move).rejects.toThrow("reviewed metadata and access preview");
      expect(await stableSnapshot()).toEqual(before);
      expect(storage).toEqual(storedBefore);
      expect(before.drafts?.some((draft) => draft.document_id === child.id)).toBe(true);
      expect(before.document_workflow_events?.length).toBeGreaterThan(0);
    },
  );

  it.each(["folder", "document", "document_content"] as const)(
    "rejects provider %s cross-space updates after generic preflight without changing metadata or queued work",
    async (kind) => {
      const destination = await dataClient.createSpace("Destination");
      const folder = await dataClient.createFolder("local-space", "Source");
      const childFolder = await dataClient.createFolder("local-space", "Child", {
        parent_id: folder.id,
      });
      const page = await dataClient.createDocument("local-space", {
        title: "Source page",
        content: "Original body",
        folder_id: childFolder.id,
      });
      const child = await dataClient.createDocument("local-space", {
        title: "Child page",
        content: "Original child",
        parent_id: page.id,
        folder_id: childFolder.id,
      });
      const schema = await dataClient.createPropertySchema("local-space", {
        name: "Preserve",
        property_type: "text",
      });
      await dataClient.setDocumentPropertyValues("local-space", child.id, [
        { property_id: schema.id, value: "Original value" },
      ]);
      const tag = await dataClient.createTag("local-space", { name: "Preserve tag" });
      await dataClient.addDocumentTag("local-space", child.id, tag.id);
      const workflow = await dataClient.createWorkflowTemplate("local-space", {
        name: "Preserve workflow",
        stages: [
          { id: "first", name: "First", sort_order: 0 },
          { id: "last", name: "Last", sort_order: 1 },
        ],
      });
      await dataClient.enableDocumentWorkflow("local-space", child.id, {
        template_id: workflow.id,
      });
      await dataClient.transitionDocumentWorkflow("local-space", child.id, { stage_id: "last" });
      await dataClient.clearQueuedSyncOperations();
      await dataClient.createTag("local-space", { name: "Unrelated queued work" });
      const before = await dataClient.exportLocalVaultSnapshot();
      const queue = await dataClient.listQueuedSyncOperations();
      const envelope = buildProviderRemoteMutationEnvelope(
        buildProviderRemoteMutationPlan(
          "google-drive",
          "Google Drive",
          "push",
          {
            status: "enabled",
            root_strategy: "drive-appdata-or-user-selected-folder",
            auth_model: "oauth2-offline-access",
            required_scopes: ["drive.file"],
            write_model: "local-sqlite-first-then-provider-api-push",
            conflict_model: "provider-revision-plus-local-replay-queue-validation",
          },
          {
            provider: "google-drive",
            connected: true,
            account_label: "Google account",
            scopes: ["drive.file"],
            sync_mode: "manual",
          },
        ),
        "2026-10-07T12:00:00.000Z",
        `cross-space-${kind}`,
      );
      const entityId = kind === "folder" ? folder.id : page.id;
      const manifest = buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: `remote-cross-space-${kind}`,
            entity_type: kind === "folder" ? "folder" : "document",
            entity_id: entityId,
            action: kind === "document_content" ? "update_content" : "update",
            payload_status: "available",
            payload: {
              id: entityId,
              space_id: destination.id,
              name: "Relocated folder",
              title: "Relocated page",
              content: "Overwrite body",
              parent_id: null,
              folder_id: null,
            },
          },
        ],
      });
      // Transport preflight validates payloads, not current local ownership. Apply must recheck it.
      const plan = buildProviderRemoteImportApplyPlan(
        buildProviderRemoteImportDryRun(
          manifest,
          queue.map((item) => item.id),
          queue,
        ),
      );
      expect(plan.selected_operation_count).toBe(1);
      const result = await dataClient.applyProviderRemoteImportManifest(manifest);
      expect(result.applied_operation_count).toBe(0);
      expect(result.failed_operation_count).toBe(1);
      expect(result.results[0].message).toContain("reviewed metadata and access preview");
      const after = await dataClient.exportLocalVaultSnapshot();
      const { sync_state: beforeSync, ...beforeData } = before;
      const { sync_state: afterSync, ...afterData } = after;
      expect(afterData).toEqual(beforeData);
      expect(afterSync?.queued_operations).toEqual(beforeSync?.queued_operations);
      expect(await dataClient.listQueuedSyncOperations()).toEqual(queue);
      expect(before.drafts?.some((draft) => draft.document_id === child.id)).toBe(true);
      expect(before.document_workflow_events?.length).toBeGreaterThan(0);
      const stored = JSON.parse(
        Array.from(storage.values()).find(
          (value) => value.includes('"schema_version"') && value.includes('"documents"'),
        ) ?? "null",
      );
      expect(stored.documents).toEqual(before.documents);
      expect(stored.drafts).toEqual(before.drafts);
      expect(stored.property_values).toEqual(before.property_values);
      expect(stored.doc_tags).toEqual(before.doc_tags);
      expect(stored.document_workflows).toEqual(before.document_workflows);
      expect(stored.document_workflow_events).toEqual(before.document_workflow_events);
      expect(stored.sync_state.queued_operations).toEqual(queue);
    },
  );

  it.each(["same-space", "cross-space"] as const)(
    "rejects %s folder-create identity collisions without overwriting the subtree",
    async (scope) => {
      const destination = await dataClient.createSpace("Destination");
      const root = await dataClient.createFolder("local-space", "Original root");
      const child = await dataClient.createFolder("local-space", "Original child", {
        parent_id: root.id,
      });
      const page = await dataClient.createDocument("local-space", {
        title: "Original page",
        content: "Original body",
        folder_id: child.id,
      });
      const schema = await dataClient.createPropertySchema("local-space", {
        name: "Original property",
        property_type: "text",
      });
      await dataClient.setDocumentPropertyValues("local-space", page.id, [
        { property_id: schema.id, value: "Original value" },
      ]);
      const tag = await dataClient.createTag("local-space", { name: "Original tag" });
      await dataClient.addDocumentTag("local-space", page.id, tag.id);
      const workflow = await dataClient.createWorkflowTemplate("local-space", {
        name: "Original workflow",
        stages: [
          { id: "review", name: "Review", sort_order: 0 },
          { id: "done", name: "Done", sort_order: 1 },
        ],
      });
      await dataClient.enableDocumentWorkflow("local-space", page.id, { template_id: workflow.id });
      const before = await dataClient.exportLocalVaultSnapshot();
      const queue = await dataClient.listQueuedSyncOperations();
      const disk = new Map(storage);
      await expect(
        dataClient.createFolder(
          scope === "same-space" ? "local-space" : destination.id,
          "Overwrite",
          { source_id: root.id },
        ),
      ).rejects.toThrow("A folder with this import id already exists");
      const after = await dataClient.exportLocalVaultSnapshot();
      const { sync_state: beforeSync, ...beforeData } = before;
      const { sync_state: afterSync, ...afterData } = after;
      expect(afterData).toEqual(beforeData);
      expect(afterSync?.queued_operations).toEqual(beforeSync?.queued_operations);
      expect(await dataClient.listQueuedSyncOperations()).toEqual(queue);
      expect(storage).toEqual(disk);
      expect(before.drafts?.some((draft) => draft.document_id === page.id)).toBe(true);
      expect(before.document_workflow_events?.length).toBeGreaterThan(0);
    },
  );

  it("retains same-space page and folder subtree moves", async () => {
    const folder = await dataClient.createFolder("local-space", "Projects");
    const target = await dataClient.createFolder("local-space", "Archive");
    const parent = await dataClient.createDocument("local-space", {
      title: "Parent",
      content: "parent",
    });
    const child = await dataClient.createDocument("local-space", {
      title: "Child",
      content: "child",
      parent_id: parent.id,
    });
    await dataClient.moveFolder("local-space", folder.id, {
      target_space_id: "local-space",
      parent_id: target.id,
    });
    expect(await dataClient.listFolders("local-space")).toEqual(
      expect.arrayContaining([expect.objectContaining({ id: folder.id, parent_id: target.id })]),
    );
    await dataClient.moveDocument("local-space", parent.id, {
      target_space_id: "local-space",
      folder_id: folder.id,
      parent_id: null,
    });
    expect(await dataClient.getDocument("local-space", child.id)).toMatchObject({
      id: child.id,
      folder_id: folder.id,
      parent_id: parent.id,
    });
    expect(
      (await dataClient.exportLocalVaultSnapshot()).drafts
        ?.filter((draft) => [parent.id, child.id].includes(draft.document_id))
        .every((draft) => draft.space_id === "local-space"),
    ).toBe(true);
  });

  it("requires local document folders to exist in the current space", async () => {
    await expect(
      dataClient.createDocument("local-space", {
        title: "Missing Folder Page",
        folder_id: "missing-folder",
      }),
    ).rejects.toThrow("Folder not found");
  });

  it("honors source ids for local folder and tag imports", async () => {
    const folder = await dataClient.createFolder("local-space", "Imported Folder", {
      source_id: "server-folder-1",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Imported Tag",
      source_id: "server-tag-1",
    });

    expect(folder.id).toBe("server-folder-1");
    expect(tag.id).toBe("server-tag-1");
    expect((await dataClient.listFolders("local-space")).map((item) => item.id)).toContain(
      "server-folder-1",
    );
    expect((await dataClient.listTags("local-space")).map((item) => item.id)).toContain(
      "server-tag-1",
    );
  });

  it("exports and imports a local vault snapshot", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Portable Note",
      content: "backup me",
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.documents.some((doc) => doc.id === created.id)).toBe(true);

    await dataClient.importLocalVaultSnapshot({
      ...exported,
      documents: exported.documents.map((doc) =>
        doc.id === created.id ? { ...doc, title: "Restored Note" } : doc,
      ),
    });

    const restored = await dataClient.getDocument("local-space", created.id);
    expect(restored.title).toBe("Restored Note");
  });

  it("exports local personal space owner metadata for mirror paths", async () => {
    const exported = await dataClient.exportLocalVaultSnapshot();
    const personalSpace = exported.spaces.find((space) => space.id === "local-space");

    expect(personalSpace).toMatchObject({
      owner_username: "local-user",
      owner_display_name: "Local User",
      created_by: "local-user",
    });
  });

  it("backfills legacy local personal space owner metadata on import", async () => {
    const exported = await dataClient.exportLocalVaultSnapshot();
    await dataClient.importLocalVaultSnapshot({
      ...exported,
      spaces: exported.spaces.map((space) =>
        space.id === "local-space"
          ? {
              id: space.id,
              name: space.name,
              slug: space.slug,
              created_at: space.created_at,
              updated_at: space.updated_at,
            }
          : space,
      ),
    });

    const reexported = await dataClient.exportLocalVaultSnapshot();
    const personalSpace = reexported.spaces.find((space) => space.id === "local-space");
    expect(personalSpace?.owner_username).toBe("local-user");
    expect(personalSpace?.owner_display_name).toBe("Local User");
  });

  it("previews standalone vault migration warnings", async () => {
    const exported = await dataClient.exportLocalVaultSnapshot();
    const summary = dataClient.previewVaultMigration({
      ...exported,
      assets: [
        {
          id: "asset-1",
          name: "diagram.png",
          size: 120,
          url: "asset://diagram.png",
          source: "browser",
          created_at: new Date().toISOString(),
        },
      ],
      tags: [
        {
          id: "tag-1",
          space_id: "local-space",
          name: "Roadmap",
          color: "#2563eb",
          created_at: "2026-07-05T00:00:00Z",
        },
      ],
      doc_tags: [{ document_id: "local-welcome", tag_id: "tag-1" }],
    });

    expect(summary.spaces).toBe(1);
    expect(summary.documents).toBe(1);
    expect(summary.assets).toBe(1);
    expect(summary.tags).toBe(1);
    expect(summary.warnings.some((warning) => warning.includes("Asset binaries"))).toBe(true);
    expect(summary.warnings.some((warning) => warning.includes("tag timestamps"))).toBe(false);
  });

  it("imports standalone vault content into a server with mapped folders", async () => {
    setCurrentLaunchMode("server");
    const assetUrl = "data:text/plain;base64,SGVsbG8=";
    const uploadedAssetUrl =
      "/api/v1/files/spaces/field-notes/attachments/source-doc/server-attachment.txt";
    const requests: Array<{ path: string; method: string; body?: unknown }> = [];
    let folderCount = 0;
    let propertyCount = 0;
    let tagCount = 0;

    (globalThis as { fetch?: unknown }).fetch = (async (url: string, init?: RequestInit) => {
      const path = url.replace("/api/v1", "");
      const method = init?.method ?? "GET";
      const body =
        typeof init?.body === "string" && init.body
          ? JSON.parse(init.body)
          : init?.body instanceof FormData
            ? init.body
            : undefined;
      requests.push({ path, method, body });

      let response: unknown = {};
      if (path === "/spaces" && method === "POST") {
        response = { id: body.source_id, name: body.name, slug: "field-notes" };
      } else if (path === "/spaces/source-space/folders" && method === "POST") {
        folderCount += 1;
        response = {
          id: body.source_id ?? `server-folder-${folderCount}`,
          name: body.name,
          parent_id: body.parent_id,
        };
      } else if (path === "/spaces/source-space/documents" && method === "POST") {
        response = { id: body.source_id };
      } else if (
        path === "/spaces/source-space/documents/source-doc/attachments" &&
        method === "POST"
      ) {
        response = {
          id: "server-attachment",
          filename: "server-attachment.txt",
          original_name: "diagram.txt",
          mime_type: "text/plain",
          size_bytes: 5,
          url: uploadedAssetUrl,
          created_at: "2026-07-05T00:00:00Z",
        };
      } else if (path === "/spaces/source-space/documents/source-doc/content" && method === "PUT") {
        response = {};
      } else if (path === "/spaces/source-space/templates" && method === "POST") {
        response = { id: body.source_id };
      } else if (path === "/spaces/source-space/properties" && method === "POST") {
        propertyCount += 1;
        response = {
          id: body.source_id ?? `server-property-${propertyCount}`,
          name: body.name,
          property_type: body.property_type,
          options: body.options,
          sort_order: propertyCount,
        };
      } else if (path === "/spaces/source-space/properties/reorder" && method === "PUT") {
        response = {};
      } else if (path === "/spaces/source-space/tags" && method === "POST") {
        tagCount += 1;
        response = {
          id: body.source_id ?? `server-tag-${tagCount}`,
          name: body.name,
          color: body.color ?? "#6b7280",
          created_at: body.created_at ?? "2026-07-05T00:00:00Z",
        };
      } else if (path === "/spaces/source-space/workflow-templates" && method === "POST") {
        response = { id: body.source_id };
      } else if (
        path === "/spaces/source-space/documents/source-doc/properties" &&
        method === "PUT"
      ) {
        response = {};
      } else if (path === "/spaces/source-space/documents/source-doc/tags" && method === "POST") {
        response = {};
      } else if (
        path === "/spaces/source-space/documents/source-doc/workflow" &&
        method === "PUT"
      ) {
        response = {
          id: "server-document-workflow",
          document_id: "source-doc",
          workflow_template_id: body.template_id,
          workflow_template_name: "Approval",
          current_stage_id: body.stage_id,
          current_stage_name: "Approved",
          status: "active",
          stages: [],
          approvers: [],
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        };
      } else if (
        path === "/spaces/source-space/documents/source-doc/workflow/events" &&
        method === "POST"
      ) {
        response = [];
      }

      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify(response),
        json: async () => response,
      } as Response;
    }) as typeof fetch;

    const summary = await dataClient.importVaultSnapshotToServer({
      schema_version: 1,
      spaces: [
        {
          id: "source-space",
          name: "Field Notes",
          slug: "field-notes",
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      folders: [
        { id: "parent-folder", space_id: "source-space", name: "Projects" },
        {
          id: "child-folder",
          space_id: "source-space",
          name: "Launch",
          parent_id: "parent-folder",
        },
      ],
      documents: [
        {
          id: "source-doc",
          space_id: "source-space",
          title: "Runbook",
          content: `# Runbook\n![Diagram](${assetUrl})`,
          folder_id: "child-folder",
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      assets: [
        {
          id: "asset-1",
          name: "diagram.txt",
          mime_type: "text/plain",
          size: 5,
          url: assetUrl,
          source: "browser",
          created_at: "2026-07-05T00:00:00Z",
        },
      ],
      templates: [
        {
          id: "template-1",
          space_id: "source-space",
          name: "Brief",
          content: "# Brief",
          tags: [],
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      workflow_templates: [
        {
          id: "workflow-1",
          space_id: "source-space",
          name: "Approval",
          stages: [
            { id: "draft", name: "Draft", sort_order: 0 },
            { id: "approved", name: "Approved", sort_order: 1 },
          ],
          approvers: [],
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      document_workflows: [
        {
          id: "document-workflow-1",
          document_id: "source-doc",
          workflow_template_id: "workflow-1",
          current_stage_id: "approved",
          status: "active",
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      document_workflow_events: [
        {
          id: "workflow-event-1",
          document_workflow_id: "document-workflow-1",
          document_id: "source-doc",
          to_stage_id: "draft",
          action: "enable",
          created_by: "source-user",
          created_at: "2026-07-05T00:00:00Z",
        },
        {
          id: "workflow-event-2",
          document_workflow_id: "document-workflow-1",
          document_id: "source-doc",
          from_stage_id: "draft",
          to_stage_id: "approved",
          action: "transition",
          note: "Approved after review",
          created_by: "source-user",
          created_at: "2026-07-05T00:05:00Z",
        },
        {
          id: "workflow-event-3",
          document_workflow_id: "document-workflow-1",
          document_id: "source-doc",
          to_stage_id: "approved",
          action: "delegate",
          note: "Delegated to editor",
          created_by: "source-user",
          created_at: "2026-07-05T00:10:00Z",
        },
      ],
      property_schemas: [
        {
          id: "property-1",
          space_id: "source-space",
          name: "Status",
          property_type: "select",
          options: ["Draft", "Approved"],
          sort_order: 2,
        },
        {
          id: "property-2",
          space_id: "source-space",
          name: "Priority",
          property_type: "text",
          sort_order: 1,
        },
      ],
      property_values: [
        {
          id: "property-value-1",
          document_id: "source-doc",
          property_id: "property-1",
          value: "Approved",
        },
      ],
      tags: [
        {
          id: "tag-1",
          space_id: "source-space",
          name: "Ops",
          color: "#2563eb",
          created_at: "2026-07-05T00:00:00Z",
        },
      ],
      doc_tags: [{ document_id: "source-doc", tag_id: "tag-1" }],
    });

    const folderRequests = requests.filter((request) => request.path.endsWith("/folders"));
    const spaceRequest = requests.find(
      (request) => request.path === "/spaces" && request.method === "POST",
    );
    const documentRequest = requests.find((request) => request.path.endsWith("/documents"));
    const templateRequest = requests.find(
      (request) => request.path === "/spaces/source-space/templates",
    );
    const attachmentRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/attachments",
    );
    const contentUpdateRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/content",
    );
    const propertyRequests = requests.filter(
      (request) => request.path === "/spaces/source-space/properties",
    );
    const propertyValueRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/properties",
    );
    const tagAssignmentRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/tags",
    );
    const tagRequest = requests.find((request) => request.path === "/spaces/source-space/tags");
    const workflowEnableRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/workflow",
    );
    const workflowTemplateRequest = requests.find(
      (request) => request.path === "/spaces/source-space/workflow-templates",
    );
    const workflowEventRequests = requests.filter(
      (request) => request.path === "/spaces/source-space/documents/source-doc/workflow/events",
    );

    expect(summary.spaces).toBe(1);
    expect(summary.folders).toBe(2);
    expect(summary.assets).toBe(1);
    expect(summary.property_schemas).toBe(2);
    expect(summary.property_values).toBe(1);
    expect(summary.tags).toBe(1);
    expect(summary.doc_tags).toBe(1);
    expect(summary.document_workflows).toBe(1);
    expect(spaceRequest?.body).toMatchObject({
      name: "Field Notes",
      source_id: "source-space",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(folderRequests.map((request) => request.body)).toEqual([
      { name: "Projects", parent_id: null, source_id: "parent-folder" },
      { name: "Launch", parent_id: "parent-folder", source_id: "child-folder" },
    ]);
    expect(documentRequest?.body).toMatchObject({
      title: "Runbook",
      content: `# Runbook\n![Diagram](${assetUrl})`,
      folder_id: "child-folder",
      source_id: "source-doc",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(attachmentRequest?.body).toBeInstanceOf(FormData);
    expect(contentUpdateRequest?.body).toEqual({
      content: `# Runbook\n![Diagram](${uploadedAssetUrl})`,
    });
    expect(templateRequest?.body).toMatchObject({
      name: "Brief",
      source_id: "template-1",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(propertyRequests.map((request) => request.body)).toEqual([
      { name: "Priority", property_type: "text", source_id: "property-2" },
      {
        name: "Status",
        property_type: "select",
        options: ["Draft", "Approved"],
        source_id: "property-1",
      },
    ]);
    expect(propertyValueRequest?.body).toEqual({
      values: [{ property_id: "property-1", value: "Approved", source_id: "property-value-1" }],
    });
    expect(tagAssignmentRequest?.body).toEqual({ tag_id: "tag-1" });
    expect(tagRequest?.body).toMatchObject({
      name: "Ops",
      source_id: "tag-1",
      created_at: "2026-07-05T00:00:00Z",
    });
    expect(workflowTemplateRequest?.body).toMatchObject({
      name: "Approval",
      source_id: "workflow-1",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(workflowEnableRequest?.body).toEqual({
      template_id: "workflow-1",
      stage_id: "approved",
      source_id: "document-workflow-1",
      event_source_id: "workflow-event-1",
      event_created_by: "source-user",
      event_created_at: "2026-07-05T00:00:00Z",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(workflowEventRequests.map((request) => request.body)).toEqual([
      {
        action: "transition",
        note: "Approved after review",
        source_id: "workflow-event-2",
        created_by: "source-user",
        created_at: "2026-07-05T00:05:00Z",
        from_stage_id: "draft",
        to_stage_id: "approved",
      },
      {
        action: "delegate",
        note: "Delegated to editor",
        source_id: "workflow-event-3",
        created_by: "source-user",
        created_at: "2026-07-05T00:10:00Z",
        from_stage_id: undefined,
        to_stage_id: "approved",
      },
    ]);
  });

  it("round-trips original template CSS through local update and portable vault export/import", async () => {
    const rawCss = " \r\n/* retained original */\n.notice { background: url(https://example.invalid/asset); }\n" + "x".repeat(20_001);
    const created = await dataClient.createTemplate("local-space", {
      name: "Legacy presentation",
      content: "# Original",
      properties: { __starlane_presentation: { custom_css: rawCss, page_width: "wide" } },
    });
    const original = await dataClient.getTemplate("local-space", created.id);
    await dataClient.updateTemplate("local-space", created.id, { name: "Renamed", content: "# Edited body", properties: original.properties });
    const updated = await dataClient.getTemplate("local-space", created.id);
    expect(updated.properties).toEqual({ __starlane_presentation: { custom_css: rawCss, page_width: "wide" } });
    const portable = JSON.parse(JSON.stringify(await dataClient.exportLocalVaultSnapshot()));
    expect(portable.templates.find((template: { id: string }) => template.id === created.id).properties.__starlane_presentation.custom_css).toBe(rawCss);
    await __resetLocalDataForTests();
    await dataClient.importLocalVaultSnapshot(portable);
    const restored = await dataClient.getTemplate("local-space", created.id);
    expect(restored.properties).toEqual({ __starlane_presentation: { custom_css: rawCss, page_width: "wide" } });
  });

  it.each(["", " \r\n\0@import 'https://example.invalid/original.css';\n" + "x".repeat(20_001)])("recovers original CSS from a page-template copy through actual local vault storage", async (rawCss) => {
    const { buildPageTemplateSnapshot } = await import("@/components/SaveAsTemplateDialog");
    const source = `Body\n<!-- starlane:custom-css -->${rawCss}<!-- /starlane:custom-css -->`;
    const input = buildPageTemplateSnapshot({ id: "published-fixture", title: "Copy", published_at: "2026-10-08" }, source, [], [], {
      name: "Recovered copy", description: "", tags: false, properties: false, presentation: true,
    });
    const created = await dataClient.createTemplate("local-space", input);
    const portable = JSON.parse(JSON.stringify(await dataClient.exportLocalVaultSnapshot()));
    await __resetLocalDataForTests();
    await dataClient.importLocalVaultSnapshot(portable);
    const restored = await dataClient.getTemplate("local-space", created.id);
    expect(restored.content).toBe("Body");
    expect(restored.properties).toEqual({ __starlane_presentation: { custom_css: rawCss } });
  });

  it("creates, updates, deletes, and exports local templates", async () => {
    const created = await dataClient.createTemplate("local-space", {
      name: "Project Brief",
      description: "Reusable kickoff template",
      content: "# {{project}}\n\n## Goals",
      tags: ["project", "brief"],
      properties: { status: "draft" },
      source_id: "source-template",
    });
    expect(created.id).toBe("source-template");

    let templates = await dataClient.listTemplates("local-space");
    expect(templates.map((template) => template.name)).toEqual(["Project Brief"]);
    expect(templates[0].tags).toEqual(["project", "brief"]);

    await dataClient.updateTemplate("local-space", created.id, {
      name: "Project Brief Updated",
      content: "# Updated",
      tags: ["project"],
    });

    const updated = await dataClient.getTemplate("local-space", created.id);
    expect(updated.name).toBe("Project Brief Updated");
    expect(updated.content).toBe("# Updated");

    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.templates.some((template) => template.id === created.id)).toBe(true);

    await dataClient.deleteTemplate("local-space", created.id);
    templates = await dataClient.listTemplates("local-space");
    expect(templates).toEqual([]);

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "template",
          entity_id: "source-template",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "source-template",
            name: "Project Brief",
            tags: ["project", "brief"],
          }),
        }),
        expect.objectContaining({
          entity_type: "template",
          entity_id: "source-template",
          action: "update",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "source-template",
            name: "Project Brief Updated",
            tags: ["project"],
          }),
        }),
        expect.objectContaining({
          entity_type: "template",
          entity_id: "source-template",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "source-template",
            name: "Project Brief Updated",
          }),
        }),
      ]),
    );
  });

  it("creates, updates, deletes, and exports local workflow templates", async () => {
    const created = await dataClient.createWorkflowTemplate("local-space", {
      name: "Draft Review",
      description: "Default document approval flow",
      source_id: "workflow-template-source",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
        { id: "approved", name: "Approved", sort_order: 2 },
      ],
      approvers: [{ approver_type: "role", approver_id: "owner", stage_id: "review" }],
    });
    expect(created.id).toBe("workflow-template-source");

    let workflows = await dataClient.listWorkflowTemplates("local-space");
    expect(workflows.map((template) => template.name)).toEqual(["Draft Review"]);
    expect(workflows[0].stages.map((stage) => stage.name)).toEqual(["Draft", "Review", "Approved"]);
    expect(workflows[0].approvers[0].approver_id).toBe("owner");

    await dataClient.updateWorkflowTemplate("local-space", created.id, {
      name: "Legal Review",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "legal", name: "Legal", sort_order: 1 },
        { id: "published", name: "Published", sort_order: 2 },
      ],
      approvers: [{ approver_type: "role", approver_id: "admin", stage_id: "legal" }],
    });

    const updated = await dataClient.getWorkflowTemplate("local-space", created.id);
    expect(updated.name).toBe("Legal Review");
    expect(updated.stages.map((stage) => stage.id)).toEqual(["draft", "legal", "published"]);

    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.workflow_templates?.some((template) => template.id === created.id)).toBe(true);

    await dataClient.deleteWorkflowTemplate("local-space", created.id);
    workflows = await dataClient.listWorkflowTemplates("local-space");
    expect(workflows).toEqual([]);

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "workflow_template",
          entity_id: "workflow-template-source",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "workflow-template-source",
            name: "Draft Review",
            stages: expect.arrayContaining([expect.objectContaining({ id: "draft" })]),
          }),
        }),
        expect.objectContaining({
          entity_type: "workflow_template",
          entity_id: "workflow-template-source",
          action: "update",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "workflow-template-source",
            name: "Legal Review",
            stages: expect.arrayContaining([expect.objectContaining({ id: "legal" })]),
          }),
        }),
        expect.objectContaining({
          entity_type: "workflow_template",
          entity_id: "workflow-template-source",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "workflow-template-source",
            name: "Legal Review",
          }),
        }),
      ]),
    );
  });

  it("enables, exports, and disables local document workflows", async () => {
    const created = await dataClient.createWorkflowTemplate("local-space", {
      name: "Draft Review",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [{ approver_type: "role", approver_id: "owner", stage_id: "review" }],
    });

    const workflow = await dataClient.enableDocumentWorkflow("local-space", "local-welcome", {
      template_id: created.id,
    });
    expect(workflow.workflow_template_name).toBe("Draft Review");
    expect(workflow.current_stage_name).toBe("Draft");

    const loaded = await dataClient.getDocumentWorkflow("local-space", "local-welcome");
    expect(loaded?.workflow_template_id).toBe(created.id);

    const transitioned = await dataClient.transitionDocumentWorkflow(
      "local-space",
      "local-welcome",
      { stage_id: "review" },
    );
    expect(transitioned.current_stage_name).toBe("Review");
    expect(transitioned.status).toBe("completed");

    await dataClient.recordDocumentWorkflowEvent("local-space", "local-welcome", {
      action: "delegate",
      note: "Hand off review",
      source_id: "workflow-event-delegate-source",
      created_by: "user-imported",
      created_at: "2026-07-08T12:00:00.000Z",
      from_stage_id: "draft",
      to_stage_id: "review",
    });

    const events = await dataClient.listDocumentWorkflowEvents("local-space", "local-welcome");
    expect(events.map((event) => event.action).sort()).toEqual([
      "delegate",
      "enable",
      "transition",
    ]);
    expect(events.find((event) => event.action === "delegate")).toMatchObject({
      id: "workflow-event-delegate-source",
      created_by: "user-imported",
      created_at: "2026-07-08T12:00:00.000Z",
    });

    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.document_workflows?.some((item) => item.document_id === "local-welcome")).toBe(
      true,
    );
    expect(
      exported.document_workflow_events?.some((item) => item.document_id === "local-welcome"),
    ).toBe(true);

    await dataClient.disableDocumentWorkflow("local-space", "local-welcome");
    await expect(
      dataClient.getDocumentWorkflow("local-space", "local-welcome"),
    ).resolves.toBeNull();

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "document_workflow",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            document_id: "local-welcome",
            workflow_template_id: created.id,
            current_stage_id: "draft",
            space_id: "local-space",
            event: expect.objectContaining({ action: "enable", to_stage_id: "draft" }),
          }),
        }),
        expect.objectContaining({
          entity_type: "document_workflow",
          action: "transition",
          payload_status: "available",
          payload: expect.objectContaining({
            document_id: "local-welcome",
            current_stage_id: "review",
            space_id: "local-space",
            event: expect.objectContaining({
              action: "transition",
              from_stage_id: "draft",
              to_stage_id: "review",
            }),
          }),
        }),
        expect.objectContaining({
          entity_type: "document_workflow_event",
          action: "delegate",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "workflow-event-delegate-source",
            document_id: "local-welcome",
            document_workflow_id: expect.any(String),
            action: "delegate",
            created_by: "user-imported",
            created_at: "2026-07-08T12:00:00.000Z",
            space_id: "local-space",
          }),
        }),
        expect.objectContaining({
          entity_type: "document_workflow",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({
            document_id: "local-welcome",
            workflow_template_id: created.id,
            space_id: "local-space",
          }),
        }),
      ]),
    );
  });

  it("filters local document lists by workflow state", async () => {
    const created = await dataClient.createWorkflowTemplate("local-space", {
      name: "Approval Queue",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
    });
    const queued = await dataClient.createDocument("local-space", {
      title: "Queued",
      content: "needs review",
    });
    const folder = await dataClient.createFolder("local-space", "Nested");
    const nestedQueued = await dataClient.createDocument("local-space", {
      title: "Nested queued",
      folder_id: folder.id,
      content: "also needs review",
    });
    const unqueued = await dataClient.createDocument("local-space", {
      title: "Unqueued",
      content: "no workflow",
    });
    for (const document of [queued, nestedQueued, unqueued]) {
      if (!document.draft_id) throw new Error("Expected an initial document draft");
      await dataClient.publishDocumentDraft("local-space", document.id, document.draft_id);
    }

    await dataClient.enableDocumentWorkflow("local-space", queued.id, {
      template_id: created.id,
    });
    await dataClient.enableDocumentWorkflow("local-space", nestedQueued.id, {
      template_id: created.id,
    });

    const draftDocs = await dataClient.listDocuments("local-space", {
      include_nested: true,
      workflow_status: "active",
      workflow_stage_id: "draft",
    });
    expect(draftDocs.map((doc) => doc.id)).toContain(queued.id);
    expect(draftDocs.map((doc) => doc.id)).toContain(nestedQueued.id);
    expect(draftDocs.map((doc) => doc.id)).not.toContain(unqueued.id);
    expect(draftDocs.find((doc) => doc.id === queued.id)?.workflow_stage_name).toBe("Draft");

    await dataClient.transitionDocumentWorkflow("local-space", queued.id, {
      stage_id: "review",
    });
    const completedDocs = await dataClient.listDocuments("local-space", {
      workflow_status: "completed",
      workflow_template_id: created.id,
    });
    expect(completedDocs.map((doc) => doc.id)).toContain(queued.id);
  });

  it("enforces local workflow stage approvers", async () => {
    const created = await dataClient.createWorkflowTemplate("local-space", {
      name: "Approval Guard",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "owner-review", name: "Owner Review", sort_order: 1 },
        { id: "admin-review", name: "Admin Review", sort_order: 2 },
      ],
      approvers: [
        { approver_type: "role", approver_id: "owner", stage_id: "owner-review" },
        { approver_type: "role", approver_id: "admin", stage_id: "admin-review" },
      ],
    });

    await dataClient.enableDocumentWorkflow("local-space", "local-welcome", {
      template_id: created.id,
    });
    await expect(
      dataClient.transitionDocumentWorkflow("local-space", "local-welcome", {
        stage_id: "owner-review",
      }),
    ).resolves.toMatchObject({ current_stage_id: "owner-review" });
    await expect(
      dataClient.transitionDocumentWorkflow("local-space", "local-welcome", {
        stage_id: "admin-review",
      }),
    ).rejects.toThrow("Workflow transition requires an assigned approver");
  });

  it("creates, updates, deletes, and exports local properties and tags", async () => {
    const createdDoc = await dataClient.createDocument("local-space", {
      title: "Structured",
      content: "with metadata",
    });

    const property = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "select",
      options: ["Draft", "Published"],
      source_id: "property-status",
    });
    expect(property.id).toBe("property-status");
    await dataClient.setDocumentPropertyValues("local-space", createdDoc.id, [
      { property_id: property.id, value: "Draft", source_id: "value-status" },
    ]);

    let values = await dataClient.listDocumentProperties("local-space", createdDoc.id);
    expect(values.find((value) => value.property_id === property.id)?.value).toBe("Draft");
    expect(values.find((value) => value.property_id === property.id)?.options).toEqual([
      "Draft",
      "Published",
    ]);

    await expect(
      dataClient.updatePropertySchema("local-space", property.id, {
        name: "State",
        property_type: "url",
      }),
    ).rejects.toThrow("nonempty values");
    await dataClient.setDocumentPropertyValues("local-space", createdDoc.id, [
      { property_id: property.id, value: null },
    ]);
    await dataClient.updatePropertySchema("local-space", property.id, {
      name: "State",
      property_type: "url",
    });
    values = await dataClient.listDocumentProperties("local-space", createdDoc.id);
    expect(values.find((value) => value.property_id === property.id)?.property_name).toBe("State");
    expect(values.find((value) => value.property_id === property.id)?.property_type).toBe("url");

    const priorityProperty = await dataClient.createPropertySchema("local-space", {
      name: "Priority",
      property_type: "text",
      source_id: "property-priority",
    });
    const ownerProperty = await dataClient.createPropertySchema("local-space", {
      name: "Owner",
      property_type: "user",
      source_id: "property-owner",
    });
    await dataClient.reorderPropertySchemas("local-space", {
      property_ids: [priorityProperty.id, property.id, ownerProperty.id],
    });
    await dataClient.setDocumentPropertyValues("local-space", createdDoc.id, [
      { property_id: ownerProperty.id, value: "local-user" },
    ]);
    values = await dataClient.listDocumentProperties("local-space", createdDoc.id);
    expect(values.map((value) => value.property_id)).toEqual([
      priorityProperty.id,
      property.id,
      ownerProperty.id,
    ]);
    expect(values.find((value) => value.property_id === ownerProperty.id)?.value).toBe(
      "local-user",
    );
    await expect(dataClient.listUserDirectory()).resolves.toEqual([
      {
        id: "local-user",
        email: "local@starlane.notes",
        display_name: "Local User",
        avatar_url: null,
      },
    ]);

    const tag = await dataClient.createTag("local-space", {
      name: "Project",
      color: "#2563eb",
    });
    await dataClient.addDocumentTag("local-space", createdDoc.id, tag.id);
    let docTags = await dataClient.listDocumentTags("local-space", createdDoc.id);
    expect(docTags.map((item) => item.name)).toEqual(["Project"]);

    await dataClient.updateTag("local-space", tag.id, { name: "Work", color: "#059669" });
    docTags = await dataClient.listDocumentTags("local-space", createdDoc.id);
    expect(docTags[0].name).toBe("Work");
    expect(docTags[0].color).toBe("#059669");

    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.property_schemas?.some((item) => item.id === property.id)).toBe(true);
    expect(exported.property_values?.some((item) => item.property_id === property.id)).toBe(true);
    expect(exported.tags?.find((item) => item.id === tag.id)?.created_at).toBeTruthy();
    expect(exported.doc_tags?.some((item) => item.tag_id === tag.id)).toBe(true);

    await dataClient.deletePropertySchema("local-space", property.id);
    await dataClient.deletePropertySchema("local-space", priorityProperty.id);
    await dataClient.deletePropertySchema("local-space", ownerProperty.id);
    expect(await dataClient.listDocumentProperties("local-space", createdDoc.id)).toEqual([]);

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "property_schema",
          entity_id: "property-status",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "property-status",
            name: "Status",
            property_type: "select",
          }),
        }),
        expect.objectContaining({
          entity_type: "property_schema",
          entity_id: "property-status",
          action: "update",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "property-status",
            name: "State",
            property_type: "url",
          }),
        }),
        expect.objectContaining({
          entity_type: "property_schema",
          entity_id: "local-space",
          action: "reorder",
          payload_status: "available",
          payload: expect.objectContaining({
            space_id: "local-space",
            property_ids: ["property-priority", "property-status", "property-owner"],
          }),
        }),
        expect.objectContaining({
          entity_type: "document_property_values",
          entity_id: createdDoc.id,
          action: "set",
          payload_status: "available",
          payload: expect.objectContaining({
            document_id: createdDoc.id,
            space_id: "local-space",
            values: expect.arrayContaining([
              expect.objectContaining({
                id: "value-status",
                property_id: "property-status",
                value: "Draft",
              }),
            ]),
          }),
        }),
        expect.objectContaining({
          entity_type: "property_schema",
          entity_id: "property-status",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "property-status",
            space_id: "local-space",
          }),
        }),
      ]),
    );

    await dataClient.deleteTag("local-space", tag.id);
    expect(await dataClient.listDocumentTags("local-space", createdDoc.id)).toEqual([]);
  });

  it("rejects invalid local vault imports", async () => {
    await expect(dataClient.importLocalVaultSnapshot({ schema_version: 999 })).rejects.toThrow(
      "Unsupported vault schema version",
    );
  });

  it("validates required title and unknown space in local mode", async () => {
    await expect(dataClient.createDocument("local-space", { title: "   " })).rejects.toThrow(
      "Document title is required",
    );

    await expect(dataClient.listDocuments("missing-space")).rejects.toThrow("Space not found");
  });

  it("builds local graph nodes from local documents", async () => {
    const created = await dataClient.createDocument("local-space", { title: "Graph Node" });
    const graph = await dataClient.getGraph("local-space");
    expect(graph.nodes.some((n) => n.id === created.id)).toBe(true);
    expect(graph.edges.length).toBe(0);
  });

  it("reports local sync state", async () => {
    expect(await dataClient.getSyncState()).toBe("idle");
  });

  it("records and clears queued local sync operations", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Queued Write",
      content: "first draft",
    });
    const asset = await dataClient.saveBrowserAsset({
      name: "diagram.txt",
      mime_type: "text/plain",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: "local-space",
      document_id: created.id,
    });
    const folder = await dataClient.createFolder("local-space", "Queued Folder");
    await dataClient.updateDocumentContent("local-space", created.id, "second draft");
    const tag = await dataClient.createTag("local-space", { name: "Queued" });
    await dataClient.addDocumentTag("local-space", created.id, tag.id);
    await dataClient.removeDocumentTag("local-space", created.id, tag.id);
    await dataClient.deleteTag("local-space", tag.id);
    await dataClient.deleteFolder("local-space", folder.id);
    writeSharedSpaceDownloadCheckpoints([
      {
        spaceId: "server-space",
        spaceName: "Server Space",
        slug: "server-space",
        sourceServerUrl: "https://notes.example.test",
        downloadedByUserId: "admin-user",
        downloadedByDisplayName: "Admin User",
        downloadedAt: "2026-07-07T01:00:00.000Z",
        documentCount: 12,
        folderCount: 3,
        tagCount: 2,
        propertyCount: 1,
      },
    ]);

    const operations = await dataClient.listQueuedSyncOperations();
    expect(operations.map((operation) => `${operation.entity_type}:${operation.action}`)).toEqual(
      expect.arrayContaining([
        "document:create",
        "document:update_content",
        "folder:create",
        "folder:delete",
        "tag:create",
        "tag:delete",
        "document_tag:add",
        "document_tag:remove",
        "asset:create",
      ]),
    );

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operation_count).toBe(operations.length);
    expect(manifest.replay_contract).toEqual({
      schema_version: 1,
      operation_order: "queued_at_ascending",
      idempotency_key: "operation.id",
      transport: "server-api",
      server_backends: ["sqlite", "postgresql"],
      required_payload_status: "available",
      conflict_policy: "server_validates_version_and_permissions",
      web_canonical_store: "server-api",
    });
    expect(manifest.replay_readiness).toEqual({
      ready: true,
      ready_operation_count: operations.length,
      blocked_operation_count: 0,
      blockers: [],
      target_backends: ["sqlite", "postgresql"],
    });
    expect(manifest.operation_summary).toEqual({
      total_operations: operations.length,
      missing_payload_count: 0,
      ready_operation_count: operations.length,
      by_entity_type: expect.objectContaining({
        document: expect.any(Number),
        document_tag: 2,
        asset: 1,
        folder: 2,
        tag: 2,
      }),
      by_action: expect.objectContaining({
        create: expect.any(Number),
        delete: 2,
        update_content: 1,
        add: 1,
        remove: 1,
      }),
      unavailable_payloads: [],
    });
    expect(manifest.provider).toBe("none");
    expect(manifest.recovery_checkpoint_count).toBe(1);
    expect(manifest.recovery_checkpoints).toEqual([
      expect.objectContaining({
        spaceId: "server-space",
        spaceName: "Server Space",
        sourceServerUrl: "https://notes.example.test",
        downloadedByUserId: "admin-user",
        documentCount: 12,
      }),
    ]);
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "document",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({ id: created.id, title: "Queued Write" }),
        }),
        expect.objectContaining({
          entity_type: "document_tag",
          action: "add",
          payload_status: "available",
          payload: { document_id: created.id, tag_id: tag.id },
        }),
        expect.objectContaining({
          entity_type: "document_tag",
          action: "remove",
          payload_status: "available",
          payload: { document_id: created.id, tag_id: tag.id, space_id: "local-space" },
        }),
        expect.objectContaining({
          entity_type: "tag",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({ id: tag.id, space_id: "local-space" }),
        }),
        expect.objectContaining({
          entity_type: "folder",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({ id: folder.id, space_id: "local-space" }),
        }),
        expect.objectContaining({
          entity_type: "asset",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            id: asset.id,
            name: "diagram.txt",
            space_id: "local-space",
            document_id: created.id,
            data_url: "data:text/plain;base64,SGVsbG8=",
          }),
        }),
      ]),
    );

    const subsetManifest = await dataClient.exportQueuedSyncReplayManifest([operations[0].id]);
    expect(subsetManifest.operation_count).toBe(1);
    expect(subsetManifest.operations).toHaveLength(1);
    expect(subsetManifest.operations[0]?.id).toBe(operations[0].id);

    await dataClient.clearQueuedSyncOperations([operations[0].id]);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operations.length - 1);

    await dataClient.clearQueuedSyncOperations();
    expect(await dataClient.listQueuedSyncOperations()).toEqual([]);
  });

  it("applies remote provider space create and update imports through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-space-create-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-space-create-1",
            entity_type: "space",
            entity_id: "remote-space-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-space-created",
              name: "Remote Space",
              slug: "remote-space",
              owner_username: "remote-owner",
              owner_display_name: "Remote Owner",
              created_by: "remote-owner",
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
          {
            id: "remote-space-update-1",
            entity_type: "space",
            entity_id: "local-space",
            action: "update",
            payload_status: "available",
            payload: {
              id: "local-space",
              name: "Remote Renamed Personal Space",
              slug: "remote-renamed-personal-space",
              owner_username: "local-user",
              owner_display_name: "Local User",
              updated_at: "2026-07-09T12:02:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 2,
      applied_operation_count: 2,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-space-create-1",
          status: "applied",
        },
        {
          operation_id: "remote-space-update-1",
          status: "applied",
        },
      ],
    });
    expect(await dataClient.getSyncState()).toBe("idle");
    await expect(dataClient.listSpaces()).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-space-created",
          name: "Remote Space",
          slug: "remote-space",
        }),
        expect.objectContaining({
          id: "local-space",
          name: "Remote Renamed Personal Space",
          slug: "remote-renamed-personal-space",
        }),
      ]),
    );
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    expect(snapshot.spaces).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-space-created",
          owner_username: "remote-owner",
          owner_display_name: "Remote Owner",
          created_by: "remote-owner",
          created_at: "2026-07-09T12:00:00.000Z",
          updated_at: "2026-07-09T12:01:00.000Z",
        }),
        expect.objectContaining({
          id: "local-space",
          owner_username: "local-user",
          owner_display_name: "Local User",
          updated_at: "2026-07-09T12:02:00.000Z",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("recovers a provider-deleted space without touching assets owned by another space", async () => {
    const deletedSpace = await dataClient.createSpace("Deleted Remote Space");
    const survivorSpace = await dataClient.createSpace("Surviving Local Space");
    const deletedDocument = await dataClient.createDocument(deletedSpace.id, {
      title: "Delete me",
    });
    const survivorDocument = await dataClient.createDocument(survivorSpace.id, {
      title: "Keep me",
    });
    await dataClient.saveBrowserAsset({
      name: "delete.txt",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: deletedSpace.id,
      document_id: deletedDocument.id,
    });
    await dataClient.saveBrowserAsset({
      name: "keep.txt",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: survivorSpace.id,
      document_id: survivorDocument.id,
    });
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-13T12:00:00.000Z",
      "remote-space-recovery",
    );
    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-space-delete",
            entity_type: "space",
            entity_id: deletedSpace.id,
            action: "delete",
            payload_status: "available",
            payload: { id: deletedSpace.id },
          },
        ],
      }),
      { allowDestructiveSpaceRecovery: true },
    );

    expect(result).toMatchObject({ status: "completed", applied_operation_count: 1 });
    await expect(dataClient.getSpace(deletedSpace.id)).rejects.toThrow("Space not found");
    await expect(
      dataClient.getDocument(survivorSpace.id, survivorDocument.id),
    ).resolves.toMatchObject({ id: survivorDocument.id });
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    expect(snapshot.assets).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: "keep.txt", space_id: survivorSpace.id }),
      ]),
    );
    expect(snapshot.assets).not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: "delete.txt", space_id: deletedSpace.id }),
      ]),
    );
  });

  it("handles idempotent and blocked remote provider space imports", async () => {
    await dataClient.clearQueuedSyncOperations();
    const initialSnapshot = await dataClient.exportLocalVaultSnapshot();
    const existingSpace = initialSnapshot.spaces.find((space) => space.id === "local-space");
    if (!existingSpace) {
      throw new Error("Expected local-space to be present in local vault snapshot");
    }
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-space-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 4,
        operations: [
          {
            id: "remote-space-create-existing-match",
            entity_type: "space",
            entity_id: "local-space",
            action: "create",
            payload_status: "available",
            payload: existingSpace,
          },
          {
            id: "remote-space-create-existing-conflict",
            entity_type: "space",
            entity_id: "local-space",
            action: "create",
            payload_status: "available",
            payload: {
              id: "local-space",
              name: "Conflicting Remote Space",
              slug: existingSpace.slug,
              created_at: existingSpace.created_at,
              updated_at: existingSpace.updated_at,
            },
          },
          {
            id: "remote-space-update-missing",
            entity_type: "space",
            entity_id: "missing-space",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-space",
              name: "Missing Remote Space",
              slug: "missing-remote-space",
            },
          },
          {
            id: "remote-space-create-after-block",
            entity_type: "space",
            entity_id: "remote-space-after-block",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-space-after-block",
              name: "Should Not Apply",
              slug: "should-not-apply",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-space-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-space-create-existing-conflict",
          status: "blocked",
          message: "Remote space create target already exists locally with different metadata",
        }),
        expect.objectContaining({
          operation_id: "remote-space-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-space-create-after-block",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    expect(await dataClient.getSyncState()).toBe("conflict");
    await expect(dataClient.listSpaces()).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "local-space",
          name: existingSpace.name,
          slug: existingSpace.slug,
        }),
      ]),
    );
    await expect(dataClient.listSpaces()).resolves.not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: "remote-space-after-block" })]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it.each(["published", "locked", "private"])("preserves remote provider %s parent_id beyond depth three without requiring local Edit or adding local replay", async parentState => {
    await dataClient.clearQueuedSyncOperations();
    await dataClient.createFolder("local-space", "Remote Folder", { source_id: "remote-folder" });
    await dataClient.clearQueuedSyncOperations();
    const imported = await dataClient.exportLocalVaultSnapshot();
    imported.documents.push(...Array.from({ length: 32 }, (_, n) => ({ id: `remote-parent-${n}`, space_id: "local-space", folder_id: "remote-folder", title: `Parent ${n}`, content: "Keep original", parent_id: n ? `remote-parent-${n - 1}` : undefined, created_by: "Remote User", published_at: parentState === "private" ? undefined : "old", ...(parentState === "locked" ? { locked: true, locked_by: "remote-owner" } : {}), updated_at: "old" })));
    await dataClient.importLocalVaultSnapshot(imported);
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-create-1",
            entity_type: "document",
            entity_id: "remote-doc-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-doc-created",
              space_id: "local-space",
              title: "Remote Created Doc",
              content: "created remotely",
              folder_id: "remote-folder",
              parent_id: "remote-parent-31",
              created_by: "Remote User",
              last_updated_by: "Remote User",
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-create-1",
          status: "applied",
        },
      ],
    });
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents.find(page => page.id === "remote-doc-created")?.parent_id).toBe("remote-parent-31");
    expect(await dataClient.getSyncState()).toBe("idle");
    await expect(
      dataClient.getDocumentContent("local-space", "remote-doc-created"),
    ).resolves.toMatchObject({
      title: "Remote Created Doc",
      content: "created remotely",
    });
    await expect(dataClient.listDocuments("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-doc-created",
          title: "Remote Created Doc",
          folder_id: "remote-folder",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document creates for missing local folders", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-create-missing-folder",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-create-missing-folder-1",
            entity_type: "document",
            entity_id: "remote-doc-missing-folder",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-doc-missing-folder",
              space_id: "local-space",
              title: "Remote Missing Folder Doc",
              content: "created remotely",
              folder_id: "missing-remote-folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-create-missing-folder-1",
          status: "blocked",
          message: "Folder not found",
        },
      ],
    });
    await expect(
      dataClient.getDocumentContent("local-space", "remote-doc-missing-folder"),
    ).rejects.toThrow("Document not found");
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document deletes through the local vault merge executor", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Delete Target",
      content: "local content",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Delete Cleanup",
      color: "#2563eb",
    });
    await dataClient.addDocumentTag("local-space", created.id, tag.id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-delete",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-delete-1",
            entity_type: "document",
            entity_id: created.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Delete Target",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-delete-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).rejects.toThrow(
      "Document not found",
    );
    await expect(dataClient.listDocumentTags("local-space", created.id)).rejects.toThrow(
      "Document not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("skips remote provider document deletes when the target document is already absent locally", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-delete-missing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-delete-missing",
            entity_type: "document",
            entity_id: "missing-document",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-document",
              space_id: "local-space",
              title: "Missing Remote Delete",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-delete-missing",
          status: "skipped",
        },
      ],
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider folder creates through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-create-1",
            entity_type: "folder",
            entity_id: "remote-folder-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-folder-created",
              space_id: "local-space",
              name: "Remote Folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-create-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-folder-created",
          name: "Remote Folder",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider folder creates beyond the old depth ceiling", async () => {
    const root = await dataClient.createFolder("local-space", "Root");
    const child = await dataClient.createFolder("local-space", "Child", {
      parent_id: root.id,
    });
    const grandchild = await dataClient.createFolder("local-space", "Grandchild", {
      parent_id: child.id,
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-create-too-deep",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-create-too-deep",
            entity_type: "folder",
            entity_id: "remote-folder-too-deep",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-folder-too-deep",
              space_id: "local-space",
              name: "Too Deep",
              parent_id: grandchild.id,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed", applied: true, attempted_operation_count: 1,
      applied_operation_count: 1, failed_operation_count: 0,
      results: [{ operation_id: "remote-folder-create-too-deep", status: "applied" }],
    });
    expect(await dataClient.listFolders("local-space")).toContainEqual(expect.objectContaining({ id: "remote-folder-too-deep", parent_id: grandchild.id }));
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document content updates through the local vault merge executor", async () => {
    const folder = await dataClient.createFolder("local-space", "Remote Content Folder");
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Merge Target",
      content: "local draft",
    });
    if (!created.draft_id) throw new Error("Expected an initial document draft");
    await dataClient.publishDocumentDraft("local-space", created.id, created.draft_id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Merge Target",
              content: "remote draft",
              folder_id: folder.id,
              updated_at: "2026-07-09T12:01:00.000Z",
              last_updated_by: "Remote User",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "remote draft",
    });
    await expect(dataClient.listDocuments("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: created.id,
          folder_id: folder.id,
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document content updates with mismatched payload identity", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Identity Target",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-update-mismatch",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-mismatch-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: "different-remote-doc",
              space_id: "local-space",
              title: "Remote Identity Target",
              content: "remote draft",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-update-mismatch-1",
          status: "failed",
          message:
            "Remote replay operation document:update_content payload ID does not match operation entity ID",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local draft",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document content updates with mismatched payload spaces", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Space Target",
      content: "local draft",
    });
    const otherSpace = await dataClient.createSpace("Other Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-update-space-mismatch",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-space-mismatch-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: otherSpace.id,
              title: "Remote Space Target",
              content: "remote draft",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-update-space-mismatch-1",
          status: "failed",
          message:
            "Remote document content update payload space does not match local document. Cross-space moves require a reviewed metadata and access preview before relocation. Keep this item in its current space until reviewed moves are available.",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local draft",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document content updates with missing folder targets", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Folder Target",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-update-missing-folder",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-missing-folder-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Folder Target",
              content: "remote draft",
              folder_id: "missing-remote-folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-update-missing-folder-1",
          status: "blocked",
          message: "Folder not found",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local draft",
    });
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document metadata updates through the local vault merge executor", async () => {
    const folder = await dataClient.createFolder("local-space", "Remote Metadata Folder");
    const created = await dataClient.createDocument("local-space", {
      title: "Local Metadata Target",
      content: "metadata update should not replace content",
    });
    if (!created.draft_id) throw new Error("Expected an initial document draft");
    await dataClient.publishDocumentDraft("local-space", created.id, created.draft_id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-metadata-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-metadata-update-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Metadata Title",
              icon: "sparkles",
              cover_image: "data:image/png;base64,cmVtb3Rl",
              folder_id: folder.id,
              updated_at: "2026-07-09T12:02:00.000Z",
              last_updated_by: "Remote User",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-metadata-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getDocument("local-space", created.id)).resolves.toMatchObject({
      title: "Remote Metadata Title",
      icon: "sparkles",
      cover_image: "data:image/png;base64,cmVtb3Rl",
      last_updated_by: "Remote User",
      updated_at: "2026-07-09T12:02:00.000Z",
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "metadata update should not replace content",
    });
    expect(
      (await dataClient.listDocuments("local-space")).find((doc) => doc.id === created.id),
    ).toMatchObject({ folder_id: folder.id });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document metadata updates with missing folder targets", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Local Metadata Folder Target",
      content: "metadata update should not move to a missing folder",
    });
    if (!created.draft_id) throw new Error("Expected an initial document draft");
    await dataClient.publishDocumentDraft("local-space", created.id, created.draft_id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-metadata-missing-folder",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-metadata-missing-folder-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Metadata Folder Move",
              folder_id: "missing-remote-folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-metadata-missing-folder-1",
          status: "blocked",
          message: "Folder not found",
        },
      ],
    });
    await expect(dataClient.getDocument("local-space", created.id)).resolves.toMatchObject({
      title: "Local Metadata Folder Target",
    });
    expect(
      (await dataClient.listDocuments("local-space")).find((doc) => doc.id === created.id),
    ).toMatchObject({ folder_id: undefined });
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider space metadata updates through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-space-metadata-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-space-metadata-update-1",
            entity_type: "space",
            entity_id: "local-space",
            action: "update",
            payload_status: "available",
            payload: {
              id: "local-space",
              name: "Remote Personal Space",
              slug: "remote-personal-space",
              owner_username: "remote-owner",
              owner_display_name: "Remote Owner",
              updated_at: "2026-07-09T12:03:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-space-metadata-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getSpace("local-space")).resolves.toMatchObject({
      name: "Remote Personal Space",
      slug: "remote-personal-space",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider space creates through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-space-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-space-create-1",
            entity_type: "space",
            entity_id: "remote-space",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-space",
              name: "Remote Research",
              slug: "remote-research",
              owner_username: "remote-owner",
              owner_display_name: "Remote Owner",
              created_by: "remote-owner",
              created_at: "2026-07-09T12:02:00.000Z",
              updated_at: "2026-07-09T12:03:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-space-create-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getSpace("remote-space")).resolves.toMatchObject({
      id: "remote-space",
      name: "Remote Research",
      slug: "remote-research",
    });
    expect(await dataClient.listSpaces()).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: "remote-space", name: "Remote Research" }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);

    const duplicateResult = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(
        buildProviderRemoteMutationEnvelope(
          buildProviderRemoteMutationPlan(
            "google-drive",
            "Google Drive",
            "push",
            {
              status: "enabled",
              root_strategy: "drive-appdata-or-user-selected-folder",
              auth_model: "oauth2-offline-access",
              required_scopes: ["drive.file"],
              write_model: "local-sqlite-first-then-provider-api-push",
              conflict_model: "provider-revision-plus-local-replay-queue-validation",
            },
            {
              provider: "google-drive",
              connected: true,
              account_label: "Google account",
              scopes: ["drive.file"],
              sync_mode: "manual",
            },
          ),
          "2026-07-09T12:04:00.000Z",
          "remote-import-space-create-duplicates",
        ),
        {
          schema_version: 1,
          operation_count: 2,
          operations: [
            {
              id: "remote-space-create-duplicate-match",
              entity_type: "space",
              entity_id: "remote-space",
              action: "create",
              payload_status: "available",
              payload: {
                id: "remote-space",
                name: "Remote Research",
                slug: "remote-research",
                owner_username: "remote-owner",
                owner_display_name: "Remote Owner",
                created_by: "remote-owner",
                created_at: "2026-07-09T12:02:00.000Z",
                updated_at: "2026-07-09T12:03:00.000Z",
              },
            },
            {
              id: "remote-space-create-duplicate-conflict",
              entity_type: "space",
              entity_id: "remote-space",
              action: "create",
              payload_status: "available",
              payload: {
                id: "remote-space",
                name: "Different Remote Research",
                slug: "different-remote-research",
                updated_at: "2026-07-09T12:05:00.000Z",
              },
            },
          ],
        },
      ),
    );

    expect(duplicateResult).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-space-create-duplicate-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-space-create-duplicate-conflict",
          status: "blocked",
        }),
      ]),
    });
    await expect(dataClient.getSpace("remote-space")).resolves.toMatchObject({
      name: "Remote Research",
      slug: "remote-research",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider folder updates through the local vault merge executor", async () => {
    const folder = await dataClient.createFolder("local-space", "Local Folder");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-update-1",
            entity_type: "folder",
            entity_id: folder.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: folder.id,
              space_id: "local-space",
              name: "Remote Folder Rename",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: folder.id,
          name: "Remote Folder Rename",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider folder updates that would create hierarchy cycles", async () => {
    const parent = await dataClient.createFolder("local-space", "Parent Folder");
    const child = await dataClient.createFolder("local-space", "Child Folder", {
      parent_id: parent.id,
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-update-cycle",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-update-cycle-1",
            entity_type: "folder",
            entity_id: parent.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: parent.id,
              space_id: "local-space",
              name: "Parent Folder",
              parent_id: child.id,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-update-cycle-1",
          status: "blocked",
          message: "Folder cannot be moved inside one of its descendants",
        },
      ],
    });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: parent.id, parent_id: undefined }),
        expect.objectContaining({ id: child.id, parent_id: parent.id }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider folder deletes through the local vault merge executor", async () => {
    const parent = await dataClient.createFolder("local-space", "Parent Folder");
    const child = await dataClient.createFolder("local-space", "Child Folder", {
      parent_id: parent.id,
    });
    const doc = await dataClient.createDocument("local-space", {
      title: "Nested Folder Doc",
      content: "nested local document",
      folder_id: child.id,
    });
    const property = await dataClient.createPropertySchema("local-space", {
      name: "Folder Delete State",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", doc.id, [
      { property_id: property.id, value: "remove me" },
    ]);
    const tag = await dataClient.createTag("local-space", {
      name: "Folder Delete Tag",
      color: "#2563eb",
    });
    await dataClient.addDocumentTag("local-space", doc.id, tag.id);
    const workflowTemplate = await dataClient.createWorkflowTemplate("local-space", {
      name: "Folder Delete Workflow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "done", name: "Done", sort_order: 1 },
      ],
    });
    await dataClient.enableDocumentWorkflow("local-space", doc.id, {
      template_id: workflowTemplate.id,
    });
    await dataClient.transitionDocumentWorkflow("local-space", doc.id, {
      stage_id: "done",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const beforeDelete = await dataClient.exportLocalVaultSnapshot();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-delete",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-delete-1",
            entity_type: "folder",
            entity_id: parent.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: parent.id,
              space_id: "local-space",
              name: "Parent Folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-delete-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listFolders("local-space")).resolves.not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: parent.id,
        }),
        expect.objectContaining({
          id: child.id,
        }),
      ]),
    );
    await expect(dataClient.getDocument("local-space", doc.id)).resolves.toMatchObject({
      id: doc.id,
      folder_id: undefined,
    });
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    for (const key of [
      "drafts",
      "assets",
      "doc_tags",
      "property_values",
      "document_workflows",
      "document_workflow_events",
    ] as const)
      expect(snapshot[key]).toEqual(beforeDelete[key]);
    expect(snapshot.documents.find((item) => item.id === doc.id)).toEqual({
      ...beforeDelete.documents.find((item) => item.id === doc.id),
      folder_id: undefined,
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it.each(["absent", "late-target", "stale-cache"])(
    "resolves remote folder absence from durable state (canonical/cache state: %s)",
    async (state) => {
      const lateTarget = state === "late-target";
      await dataClient.clearQueuedSyncOperations();
      const operationsBefore = await dataClient.listQueuedSyncOperations();
      if (state === "stale-cache") {
        await dataClient.createFolder("local-space", "Stale folder", {
          source_id: "missing-folder",
        });
        await dataClient.clearQueuedSyncOperations();
        await createLocalVaultAdapter("none").commitSnapshotMutation((snapshot) => {
          snapshot.folders = snapshot.folders.filter((folder) => folder.id !== "missing-folder");
          return { snapshot };
        });
      }
      if (lateTarget)
        await createLocalVaultAdapter("none").commitSnapshotMutation((snapshot) => {
          snapshot.folders.push({
            id: "missing-folder",
            space_id: "local-space",
            name: "Durably added outside cache",
          });
          return { snapshot };
        });
      const envelope = buildProviderRemoteMutationEnvelope(
        buildProviderRemoteMutationPlan(
          "google-drive",
          "Google Drive",
          "push",
          {
            status: "enabled",
            root_strategy: "drive-appdata-or-user-selected-folder",
            auth_model: "oauth2-offline-access",
            required_scopes: ["drive.file"],
            write_model: "local-sqlite-first-then-provider-api-push",
            conflict_model: "provider-revision-plus-local-replay-queue-validation",
          },
          {
            provider: "google-drive",
            connected: true,
            account_label: "Google account",
            scopes: ["drive.file"],
            sync_mode: "manual",
          },
        ),
        "2026-07-09T12:00:00.000Z",
        "remote-import-folder-delete-missing",
      );

      const result = await dataClient.applyProviderRemoteImportManifest(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 1,
          operations: [
            {
              id: "remote-folder-delete-missing",
              entity_type: "folder",
              entity_id: "missing-folder",
              action: "delete",
              payload_status: "available",
              payload: {
                id: "missing-folder",
                space_id: "local-space",
                name: "Missing Folder",
              },
            },
          ],
        }),
      );

      expect(result).toMatchObject({
        status: "completed",
        applied: true,
        attempted_operation_count: 1,
        applied_operation_count: lateTarget ? 1 : 0,
        skipped_operation_count: lateTarget ? 0 : 1,
        failed_operation_count: 0,
        results: [
          {
            operation_id: "remote-folder-delete-missing",
            status: lateTarget ? "applied" : "skipped",
          },
        ],
      });
      expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
      await dataClient.updateDocument("local-space", "local-welcome", { title: "Unrelated edit" });
      expect(
        (await dataClient.exportLocalVaultSnapshot()).folders.some(
          (folder) => folder.id === "missing-folder",
        ),
      ).toBe(false);
    },
  );

  it("applies remote provider tag creates through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-tag-create-1",
            entity_type: "tag",
            entity_id: "remote-tag-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-tag-created",
              space_id: "local-space",
              name: "Remote Tag",
              color: "#2563eb",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-tag-create-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listTags("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-tag-created",
          name: "Remote Tag",
          color: "#2563eb",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider tag create duplicates", async () => {
    const tag = await dataClient.createTag("local-space", {
      name: "Existing Local Tag",
      color: "#6b7280",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-create-existing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-tag-create-existing-match",
            entity_type: "tag",
            entity_id: tag.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: tag.id,
              space_id: "local-space",
              name: tag.name,
              color: tag.color,
              created_at: tag.created_at,
            },
          },
          {
            id: "remote-tag-create-existing-conflict",
            entity_type: "tag",
            entity_id: tag.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: tag.id,
              space_id: "local-space",
              name: "Remote Duplicate Tag",
              color: "#2563eb",
              created_at: tag.created_at,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-tag-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-tag-create-existing-conflict",
          status: "blocked",
        }),
      ]),
    });
    await expect(dataClient.listTags("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: tag.id,
          name: "Existing Local Tag",
          color: "#6b7280",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider tag updates through the local vault merge executor", async () => {
    const tag = await dataClient.createTag("local-space", {
      name: "Local Tag",
      color: "#6b7280",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-tag-update-1",
            entity_type: "tag",
            entity_id: tag.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: tag.id,
              space_id: "local-space",
              name: "Remote Tag Rename",
              color: "#16a34a",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-tag-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listTags("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: tag.id,
          name: "Remote Tag Rename",
          color: "#16a34a",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider tag deletes through the local vault merge executor", async () => {
    const doc = await dataClient.createDocument("local-space", {
      title: "Tagged Local Doc",
      content: "tagged locally",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Local Tag",
      color: "#6b7280",
    });
    await dataClient.addDocumentTag("local-space", doc.id, tag.id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-delete",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-tag-delete-1",
            entity_type: "tag",
            entity_id: tag.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: tag.id,
              space_id: "local-space",
              name: "Local Tag",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-tag-delete-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listTags("local-space")).resolves.not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: tag.id,
        }),
      ]),
    );
    await expect(dataClient.listDocumentTags("local-space", doc.id)).resolves.toEqual([]);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document tag adds through the local vault merge executor", async () => {
    const doc = await dataClient.createDocument("local-space", {
      title: "Remote Tag Target",
      content: "local document",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Remote Linked Tag",
      color: "#2563eb",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-tag-add",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-tag-add-1",
            entity_type: "document_tag",
            entity_id: `${doc.id}:${tag.id}`,
            action: "add",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: tag.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-document-tag-add-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listDocumentTags("local-space", doc.id)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: tag.id,
          name: "Remote Linked Tag",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document tag removes through the local vault merge executor", async () => {
    const doc = await dataClient.createDocument("local-space", {
      title: "Remote Untag Target",
      content: "local document",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Remote Removed Tag",
      color: "#2563eb",
    });
    await dataClient.addDocumentTag("local-space", doc.id, tag.id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-tag-remove",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-tag-remove-1",
            entity_type: "document_tag",
            entity_id: `${doc.id}:${tag.id}`,
            action: "remove",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: tag.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-document-tag-remove-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listDocumentTags("local-space", doc.id)).resolves.toEqual([]);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider document create duplicates", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Existing Local Doc",
      content: "local copy",
    });
    const existingVaultDocument = (await dataClient.exportLocalVaultSnapshot()).documents.find(
      (document) => document.id === created.id,
    );
    if (!existingVaultDocument) {
      throw new Error("Expected document to be present in local vault snapshot");
    }
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-create-existing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-doc-create-existing-match",
            entity_type: "document",
            entity_id: created.id,
            action: "create",
            payload_status: "available",
            payload: existingVaultDocument,
          },
          {
            id: "remote-doc-create-existing",
            entity_type: "document",
            entity_id: created.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Duplicate",
              content: "remote duplicate",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-doc-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-doc-create-existing",
          status: "blocked",
        }),
      ]),
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      title: "Existing Local Doc",
      content: "local copy",
    });
  });

  it("handles idempotent and blocked remote provider folder create duplicates", async () => {
    const folder = await dataClient.createFolder("local-space", "Existing Local Folder");
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-create-existing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-folder-create-existing-match",
            entity_type: "folder",
            entity_id: folder.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: folder.id,
              space_id: "local-space",
              name: "Existing Local Folder",
            },
          },
          {
            id: "remote-folder-create-existing-conflict",
            entity_type: "folder",
            entity_id: folder.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: folder.id,
              space_id: "local-space",
              name: "Remote Duplicate Folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-folder-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-folder-create-existing-conflict",
          status: "blocked",
        }),
      ]),
    });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: folder.id,
          name: "Existing Local Folder",
        }),
      ]),
    );
  });

  it("blocks remote provider folder updates when the target folder is missing locally", async () => {
    const existing = await dataClient.createFolder("local-space", "Existing Local Folder");
    const otherSpace = await dataClient.createSpace("Other Folder Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-update-missing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-folder-update-missing",
            entity_type: "folder",
            entity_id: "missing-folder",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-folder",
              space_id: "local-space",
              name: "Missing Remote Folder",
            },
          },
          {
            id: "remote-folder-update-wrong-space",
            entity_type: "folder",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Remote Folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-folder-update-missing",
          status: "failed",
        }),
        expect.objectContaining({
          operation_id: "remote-folder-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.listFolders("local-space")).resolves.not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "missing-folder",
        }),
      ]),
    );
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: existing.id,
          name: "Existing Local Folder",
        }),
      ]),
    );
    await expect(dataClient.listFolders(otherSpace.id)).resolves.not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: existing.id })]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider tag updates when the target tag is missing locally", async () => {
    const existing = await dataClient.createTag("local-space", {
      name: "Existing Local Tag",
      color: "#6b7280",
    });
    const otherSpace = await dataClient.createSpace("Other Tag Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-update-missing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-tag-update-missing",
            entity_type: "tag",
            entity_id: "missing-tag",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-tag",
              space_id: "local-space",
              name: "Missing Remote Tag",
              color: "#2563eb",
            },
          },
          {
            id: "remote-tag-update-wrong-space",
            entity_type: "tag",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Remote Tag",
              color: "#16a34a",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-tag-update-missing",
          status: "failed",
        }),
        expect.objectContaining({
          operation_id: "remote-tag-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.listTags("local-space")).resolves.not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "missing-tag",
        }),
      ]),
    );
    await expect(dataClient.listTags("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: existing.id,
          name: "Existing Local Tag",
          color: "#6b7280",
        }),
      ]),
    );
    await expect(dataClient.listTags(otherSpace.id)).resolves.not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: existing.id })]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("skips remote provider tag deletes when the target tag is already absent locally", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-delete-missing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-tag-delete-missing",
            entity_type: "tag",
            entity_id: "missing-tag",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-tag",
              space_id: "local-space",
              name: "Missing Remote Tag",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-tag-delete-missing",
          status: "skipped",
        },
      ],
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider document tag imports", async () => {
    const doc = await dataClient.createDocument("local-space", {
      title: "Remote Tag Edge Cases",
      content: "local document",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Existing Relationship",
      color: "#2563eb",
    });
    const unassignedTag = await dataClient.createTag("local-space", {
      name: "Unassigned Relationship",
      color: "#16a34a",
    });
    await dataClient.addDocumentTag("local-space", doc.id, tag.id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-tag-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-document-tag-add-existing",
            entity_type: "document_tag",
            entity_id: `${doc.id}:${tag.id}`,
            action: "add",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: tag.id,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-tag-remove-absent",
            entity_type: "document_tag",
            entity_id: `${doc.id}:${unassignedTag.id}`,
            action: "remove",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: unassignedTag.id,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-tag-add-missing-tag",
            entity_type: "document_tag",
            entity_id: `${doc.id}:missing-tag`,
            action: "add",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: "missing-tag",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 3,
      applied_operation_count: 0,
      skipped_operation_count: 2,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-tag-add-existing",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-document-tag-remove-absent",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-document-tag-add-missing-tag",
          status: "failed",
        }),
      ]),
    });
    await expect(dataClient.listDocumentTags("local-space", doc.id)).resolves.toEqual([
      expect.objectContaining({
        id: tag.id,
      }),
    ]);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider template creates, updates, and deletes through the local vault merge executor", async () => {
    const existing = await dataClient.createTemplate("local-space", {
      name: "Local Template",
      content: "# Local",
      tags: ["local"],
    });
    const deleted = await dataClient.createTemplate("local-space", {
      name: "Deleted Template",
      content: "# Delete",
      tags: ["cleanup"],
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-template-crud",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-template-create-1",
            entity_type: "template",
            entity_id: "remote-template-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-template-created",
              space_id: "local-space",
              name: "Remote Template",
              description: "Created remotely",
              content: "# Remote",
              tags: ["remote"],
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
          {
            id: "remote-template-update-1",
            entity_type: "template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Remote Template Rename",
              content: "# Remote Update",
              tags: ["updated"],
              updated_at: "2026-07-09T12:02:00.000Z",
            },
          },
          {
            id: "remote-template-delete-1",
            entity_type: "template",
            entity_id: deleted.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: deleted.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 3,
      applied_operation_count: 3,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(
      dataClient.getTemplate("local-space", "remote-template-created"),
    ).resolves.toMatchObject({
      name: "Remote Template",
      content: "# Remote",
      tags: ["remote"],
    });
    await expect(dataClient.getTemplate("local-space", existing.id)).resolves.toMatchObject({
      name: "Remote Template Rename",
      content: "# Remote Update",
      tags: ["updated"],
    });
    await expect(dataClient.getTemplate("local-space", deleted.id)).rejects.toThrow(
      "Template not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("fails remote provider template imports that violate local template limits", async () => {
    const existing = await dataClient.createTemplate("local-space", {
      name: "Template Limit Target",
      content: "# Local",
    });
    await dataClient.clearQueuedSyncOperations();
    const oversizedContent = "x".repeat(5 * 1024 * 1024 + 1);
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-template-limits",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-template-create-long-name",
            entity_type: "template",
            entity_id: "remote-template-long-name",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-template-long-name",
              space_id: "local-space",
              name: "x".repeat(101),
              content: "# Remote",
            },
          },
          {
            id: "remote-template-update-long-description",
            entity_type: "template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Template Limit Target",
              description: "x".repeat(1001),
              content: "# Remote Update",
            },
          },
          {
            id: "remote-template-create-oversized-content",
            entity_type: "template",
            entity_id: "remote-template-oversized-content",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-template-oversized-content",
              space_id: "local-space",
              name: "Oversized Template",
              content: oversizedContent,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      failed_operation_count: 3,
      results: [
        {
          operation_id: "remote-template-create-long-name",
          status: "failed",
          message: "Remote replay operation template:create payload name is too long",
        },
        {
          operation_id: "remote-template-update-long-description",
          status: "failed",
          message: "Remote replay operation template:update payload description is too long",
        },
        {
          operation_id: "remote-template-create-oversized-content",
          status: "failed",
          message: "Remote replay operation template:create payload content exceeds 5 MB limit",
        },
      ],
    });
    await expect(dataClient.getTemplate("local-space", existing.id)).resolves.toMatchObject({
      name: "Template Limit Target",
      content: "# Local",
    });
    await expect(
      dataClient.getTemplate("local-space", "remote-template-long-name"),
    ).rejects.toThrow("Template not found");
    await expect(
      dataClient.getTemplate("local-space", "remote-template-oversized-content"),
    ).rejects.toThrow("Template not found");
  });

  it("handles idempotent and blocked remote provider template imports", async () => {
    const existing = await dataClient.createTemplate("local-space", {
      name: "Existing Template",
      content: "# Existing",
      tags: ["existing"],
    });
    const existingVaultTemplate = (await dataClient.exportLocalVaultSnapshot()).templates.find(
      (template) => template.id === existing.id,
    );
    expect(existingVaultTemplate).toBeDefined();
    const otherSpace = await dataClient.createSpace("Other Template Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-template-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-template-create-existing-match",
            entity_type: "template",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: existingVaultTemplate,
          },
          {
            id: "remote-template-create-existing",
            entity_type: "template",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Duplicate Template",
              content: "# Duplicate",
            },
          },
          {
            id: "remote-template-update-missing",
            entity_type: "template",
            entity_id: "missing-template",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-template",
              space_id: "local-space",
              name: "Missing Template",
              content: "# Missing",
            },
          },
          {
            id: "remote-template-update-wrong-space",
            entity_type: "template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Template",
              content: "# Wrong Space",
            },
          },
          {
            id: "remote-template-delete-missing",
            entity_type: "template",
            entity_id: "missing-delete-template",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-delete-template",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-template-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-template-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-template-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-template-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-template-delete-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.getTemplate("local-space", existing.id)).resolves.toMatchObject({
      name: "Existing Template",
      content: "# Existing",
    });
    await expect(dataClient.getTemplate(otherSpace.id, existing.id)).rejects.toThrow(
      "Template not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider workflow template create, update, and delete imports", async () => {
    const existing = await dataClient.createWorkflowTemplate("local-space", {
      name: "Existing Flow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [{ approver_type: "role", approver_id: "owner", stage_id: "review" }],
    });
    const deleted = await dataClient.createWorkflowTemplate("local-space", {
      name: "Deleted Flow",
      stages: [
        { id: "open", name: "Open", sort_order: 0 },
        { id: "closed", name: "Closed", sort_order: 1 },
      ],
      approvers: [],
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-workflow-template-crud",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-workflow-template-create-1",
            entity_type: "workflow_template",
            entity_id: "remote-workflow-template-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-workflow-template-created",
              space_id: "local-space",
              name: "Remote Flow",
              description: "Created remotely",
              stages: [
                { id: "intake", name: "Intake", sort_order: 0 },
                { id: "approve", name: "Approve", sort_order: 1 },
              ],
              approvers: [{ approver_type: "role", approver_id: "admin", stage_id: "approve" }],
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
          {
            id: "remote-workflow-template-update-1",
            entity_type: "workflow_template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Remote Flow Rename",
              stages: [
                { id: "draft", name: "Draft", sort_order: 0 },
                { id: "legal", name: "Legal", sort_order: 1 },
                { id: "published", name: "Published", sort_order: 2 },
              ],
              approvers: [{ approver_type: "role", approver_id: "legal", stage_id: "legal" }],
              updated_at: "2026-07-09T12:02:00.000Z",
            },
          },
          {
            id: "remote-workflow-template-delete-1",
            entity_type: "workflow_template",
            entity_id: deleted.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: deleted.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 3,
      applied_operation_count: 3,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(
      dataClient.getWorkflowTemplate("local-space", "remote-workflow-template-created"),
    ).resolves.toMatchObject({
      name: "Remote Flow",
      stages: [
        { id: "intake", name: "Intake", sort_order: 0 },
        { id: "approve", name: "Approve", sort_order: 1 },
      ],
      approvers: [expect.objectContaining({ approver_id: "admin", stage_id: "approve" })],
    });
    await expect(dataClient.getWorkflowTemplate("local-space", existing.id)).resolves.toMatchObject(
      {
        name: "Remote Flow Rename",
        stages: expect.arrayContaining([expect.objectContaining({ id: "legal" })]),
        approvers: [expect.objectContaining({ approver_id: "legal", stage_id: "legal" })],
      },
    );
    await expect(dataClient.getWorkflowTemplate("local-space", deleted.id)).rejects.toThrow(
      "Workflow template not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider workflow template imports", async () => {
    const existing = await dataClient.createWorkflowTemplate("local-space", {
      name: "Existing Flow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [],
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    const existingVaultWorkflowTemplate = (exported.workflow_templates ?? []).find(
      (template) => template.id === existing.id,
    );
    if (!existingVaultWorkflowTemplate) {
      throw new Error("Expected workflow template to be present in local vault snapshot");
    }
    const otherSpace = await dataClient.createSpace("Other Workflow Template Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-workflow-template-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-workflow-template-create-existing-match",
            entity_type: "workflow_template",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: existingVaultWorkflowTemplate,
          },
          {
            id: "remote-workflow-template-create-existing",
            entity_type: "workflow_template",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Duplicate Flow",
              stages: [
                { id: "draft", name: "Draft", sort_order: 0 },
                { id: "review", name: "Review", sort_order: 1 },
              ],
              approvers: [],
            },
          },
          {
            id: "remote-workflow-template-update-missing",
            entity_type: "workflow_template",
            entity_id: "missing-workflow-template",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-workflow-template",
              space_id: "local-space",
              name: "Missing Flow",
              stages: [
                { id: "draft", name: "Draft", sort_order: 0 },
                { id: "review", name: "Review", sort_order: 1 },
              ],
              approvers: [],
            },
          },
          {
            id: "remote-workflow-template-update-wrong-space",
            entity_type: "workflow_template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Flow",
              stages: [
                { id: "draft", name: "Draft", sort_order: 0 },
                { id: "review", name: "Review", sort_order: 1 },
              ],
              approvers: [],
            },
          },
          {
            id: "remote-workflow-template-delete-missing",
            entity_type: "workflow_template",
            entity_id: "missing-delete-workflow-template",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-delete-workflow-template",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-workflow-template-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-workflow-template-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-workflow-template-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-workflow-template-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-workflow-template-delete-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.getWorkflowTemplate("local-space", existing.id)).resolves.toMatchObject(
      {
        name: "Existing Flow",
      },
    );
    await expect(dataClient.getWorkflowTemplate(otherSpace.id, existing.id)).rejects.toThrow(
      "Workflow template not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it.each(
    (
      [
        ["null type", { property_type: null }],
        ["null order", { sort_order: null }],
        ["fractional order", { sort_order: 1.5 }],
        ["overflow order", { sort_order: 2147483648 }],
        ["underflow order", { sort_order: -2147483649 }],
      ] as [string, Record<string, unknown>][]
    ).flatMap(([label, malformed]) =>
      ["create", "update"].map((action) => ({ label, malformed, action })),
    ),
  )(
    "rejects present malformed provider schema $label/$action before changing canonical properties",
    async ({ malformed, action }) => {
      const existing = await dataClient.createPropertySchema("local-space", {
        name: "Original",
        property_type: "text",
      });
      await dataClient.setDocumentPropertyValues(
        "local-space",
        "local-welcome",
        [{ property_id: existing.id, value: "Keep raw value" }],
      );
      await dataClient.clearQueuedSyncOperations();
      await dataClient.createPropertySchema("local-space", {
        name: "Unrelated queued schema",
        property_type: "text",
      });
      const envelope = buildProviderRemoteMutationEnvelope(
        buildProviderRemoteMutationPlan(
          "google-drive",
          "Drive",
          "push",
          {
            status: "enabled",
            root_strategy: "drive-appdata-or-user-selected-folder",
            auth_model: "oauth2-offline-access",
            required_scopes: ["drive.file"],
            write_model: "local-sqlite-first-then-provider-api-push",
            conflict_model:
              "provider-revision-plus-local-replay-queue-validation",
          },
          {
            provider: "google-drive",
            connected: true,
            account_label: "Account",
            scopes: ["drive.file"],
            sync_mode: "manual",
          },
        ),
        "2026-10-07T12:00:00Z",
        "malformed-property-admission",
      );
      const id = action === "create" ? "new-malformed-property" : existing.id;
      const before = await dataClient.exportLocalVaultSnapshot();
      const queued = await dataClient.listQueuedSyncOperations();
      const result = await dataClient.applyProviderRemoteImportManifest(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 1,
          operations: [
            {
              id: `malformed-${action}`,
              entity_type: "property_schema",
              entity_id: id,
              action,
              payload_status: "available",
              payload: {
                id,
                space_id: "local-space",
                name: "Must not apply",
                property_type: "text",
                sort_order: 7,
                ...malformed,
              },
            },
          ],
        }),
      );
      expect(result.applied_operation_count).toBe(0);
      expect(result.results[0].status).not.toBe("applied");
      expect(result.results[0].message).toContain(
        Object.prototype.hasOwnProperty.call(malformed, "property_type")
          ? "payload type is invalid"
          : "payload sort order is invalid",
      );
      const after = await dataClient.exportLocalVaultSnapshot();
      expect(after.property_schemas).toEqual(before.property_schemas);
      expect(after.property_values).toEqual(before.property_values);
      expect(after.documents).toEqual(before.documents);
      expect(after.drafts).toEqual(before.drafts);
      expect(await dataClient.listQueuedSyncOperations()).toEqual(queued);
      const reopened = await createLocalVaultAdapter("none").loadSnapshot();
      expect(reopened.property_schemas).toEqual(before.property_schemas);
      expect(reopened.property_values).toEqual(before.property_values);
      expect(reopened.sync_state?.queued_operations).toEqual(queued);
      expect(await dataClient.listPropertySchemas("local-space")).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            id: existing.id,
            name: "Original",
            property_type: "text",
          }),
        ]),
      );
    },
  );

  it("preserves absent legacy provider schema defaults and exact valid integer boundaries", async () => {
    await dataClient.createPropertySchema("local-space", {
      name: "Unrelated pending",
      property_type: "text",
    });
    const queue = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model:
            "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-10-07T12:00:00Z",
      "valid-property-admission",
    );
    const apply = async (
      operationId: string,
      action: string,
      payload: Record<string, unknown>,
    ) => {
      const result = await dataClient.applyProviderRemoteImportManifest(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 1,
          operations: [
            {
              id: operationId,
              entity_type: "property_schema",
              entity_id: payload.id as string,
              action,
              payload_status: "available",
              payload,
            },
          ],
        }),
      );
      expect(result.applied_operation_count).toBe(1);
      expect(await dataClient.listQueuedSyncOperations()).toEqual(queue);
    };
    await apply("legacy-create", "create", {
      id: "legacy-raw-id",
      space_id: "local-space",
      name: "Legacy",
      options: { raw_metadata: ["Keep", "CASE"] },
    });
    const created = (
      await dataClient.exportLocalVaultSnapshot()
    ).property_schemas!.find((schema) => schema.id === "legacy-raw-id")!;
    expect(created).toMatchObject({
      property_type: "text",
      sort_order: 2,
      options: { raw_metadata: ["Keep", "CASE"] },
    });
    await apply("legacy-update", "update", {
      id: created.id,
      space_id: "local-space",
      name: "Legacy updated",
      options: created.options,
    });
    expect(
      (await dataClient.exportLocalVaultSnapshot()).property_schemas!.find(
        (schema) => schema.id === created.id,
      ),
    ).toMatchObject({
      property_type: "text",
      sort_order: created.sort_order,
      options: created.options,
    });
    for (const sort_order of [-2147483648, 2147483647]) {
      const id = `boundary-${sort_order}`;
      const payload = {
        id,
        space_id: "local-space",
        name: id,
        property_type: "select",
        options: {
          options: ["Keep CASE", "Exact"],
          metadata: { preserved: true },
        },
        sort_order,
      };
      await apply(`create-${id}`, "create", payload);
      await apply(`update-${id}`, "update", {
        ...payload,
        name: `${id} updated`,
      });
      expect(
        (
          await createLocalVaultAdapter("none").loadSnapshot()
        ).property_schemas!.find((schema) => schema.id === id),
      ).toMatchObject({ ...payload, name: `${id} updated` });
    }
  });

  it("applies remote provider property schema create, update, reorder, and delete imports", async () => {
    const updated = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "text",
    });
    const deleted = await dataClient.createPropertySchema("local-space", {
      name: "Deprecated",
      property_type: "text",
    });
    const first = await dataClient.createPropertySchema("local-space", {
      name: "First",
      property_type: "text",
    });
    const second = await dataClient.createPropertySchema("local-space", {
      name: "Second",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: deleted.id, value: "remove me" },
    ]);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-property-schema-crud",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 4,
        operations: [
          {
            id: "remote-property-schema-create-1",
            entity_type: "property_schema",
            entity_id: "remote-property-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-property-created",
              space_id: "local-space",
              name: "Remote Status",
              property_type: "select",
              options: { options: ["Draft", "Published"] },
              sort_order: 12,
            },
          },
          {
            id: "remote-property-schema-update-1",
            entity_type: "property_schema",
            entity_id: updated.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: updated.id,
              space_id: "local-space",
              name: "Remote State",
              property_type: "url",
              options: { placeholder: "https://example.test" },
            },
          },
          {
            id: "remote-property-schema-reorder-1",
            entity_type: "property_schema",
            entity_id: "local-space",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "local-space",
              property_ids: [second.id, first.id],
            },
          },
          {
            id: "remote-property-schema-delete-1",
            entity_type: "property_schema",
            entity_id: deleted.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: deleted.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 4,
      applied_operation_count: 4,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    const schemas = await dataClient.listPropertySchemas("local-space");
    expect(schemas).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-property-created",
          name: "Remote Status",
          property_type: "select",
          sort_order: 12,
        }),
        expect.objectContaining({
          id: updated.id,
          name: "Remote State",
          property_type: "url",
        }),
        expect.objectContaining({ id: second.id, sort_order: 1 }),
        expect.objectContaining({ id: first.id, sort_order: 2 }),
      ]),
    );
    expect(schemas.some((schema) => schema.id === deleted.id)).toBe(false);
    const values = await dataClient.listDocumentProperties("local-space", "local-welcome");
    expect(values.some((value) => value.property_id === deleted.id)).toBe(false);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider property schema imports", async () => {
    const existing = await dataClient.createPropertySchema("local-space", {
      name: "Existing Status",
      property_type: "text",
    });
    const reorderDuplicate = await dataClient.createPropertySchema("local-space", {
      name: "Duplicate Order Status",
      property_type: "text",
    });
    const otherSpace = await dataClient.createSpace("Other Property Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-property-schema-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 7,
        operations: [
          {
            id: "remote-property-schema-create-existing-match",
            entity_type: "property_schema",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: existing.name,
              property_type: existing.property_type,
              options: existing.options,
              sort_order: existing.sort_order,
            },
          },
          {
            id: "remote-property-schema-create-existing",
            entity_type: "property_schema",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Duplicate Status",
              property_type: "text",
            },
          },
          {
            id: "remote-property-schema-update-missing",
            entity_type: "property_schema",
            entity_id: "missing-property-schema",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-property-schema",
              space_id: "local-space",
              name: "Missing Status",
              property_type: "text",
            },
          },
          {
            id: "remote-property-schema-update-wrong-space",
            entity_type: "property_schema",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Status",
              property_type: "select",
              options: { values: ["Wrong"] },
            },
          },
          {
            id: "remote-property-schema-reorder-missing",
            entity_type: "property_schema",
            entity_id: "local-space",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "local-space",
              property_ids: ["missing-property-schema"],
            },
          },
          {
            id: "remote-property-schema-reorder-duplicate",
            entity_type: "property_schema",
            entity_id: "local-space",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "local-space",
              property_ids: [reorderDuplicate.id, reorderDuplicate.id],
            },
          },
          {
            id: "remote-property-schema-delete-missing",
            entity_type: "property_schema",
            entity_id: "missing-delete-property-schema",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-delete-property-schema",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 2,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-property-schema-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-reorder-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-reorder-duplicate",
          status: "skipped",
          message:
            "Remote replay operation property_schema:reorder payload includes duplicate property IDs",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-delete-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.listPropertySchemas("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: existing.id, name: "Existing Status" }),
        expect.objectContaining({
          id: reorderDuplicate.id,
          sort_order: reorderDuplicate.sort_order,
        }),
      ]),
    );
    await expect(dataClient.listPropertySchemas(otherSpace.id)).resolves.not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: existing.id })]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("rolls back staged provider property caches when canonical persistence fails", async () => {
    const schema = await dataClient.createPropertySchema("local-space", {
      name: "Imported durable",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: schema.id, value: "Before" },
    ]);
    await dataClient.clearQueuedSyncOperations();
    const before = await dataClient.exportLocalVaultSnapshot();
    const disk = new Map(storage);
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-property-durability-failure",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 1,
      operations: [
        {
          id: "remote-durable-failure",
          entity_type: "document_property_values",
          entity_id: "local-welcome",
          action: "set",
          payload_status: "available",
          payload: {
            document_id: "local-welcome",
            space_id: "local-space",
            values: [
              { id: "remote-durable-value", property_id: schema.id, value: "Failed remote value" },
            ],
          },
        },
      ],
    });
    const original = window.localStorage.setItem;
    window.localStorage.setItem = (key, value) => {
      if (value.includes("Failed remote value")) throw new Error("QuotaExceededError");
      original(key, value);
    };
    try {
      const result = await dataClient.applyProviderRemoteImportManifest(manifest);
      expect(result.failed_operation_count).toBe(1);
    } finally {
      window.localStorage.setItem = original;
    }
    expect((await dataClient.exportLocalVaultSnapshot()).property_values).toEqual(
      before.property_values,
    );
    expect(await dataClient.listQueuedSyncOperations()).toEqual([]);
    for (const [key, value] of disk) {
      if (!key.startsWith("starlane_local_vault_")) continue;
      const beforeDisk = JSON.parse(value);
      const afterDisk = JSON.parse(storage.get(key)!);
      expect(afterDisk.property_values).toEqual(beforeDisk.property_values);
      expect(afterDisk.property_schemas).toEqual(beforeDisk.property_schemas);
      expect(afterDisk.sync_state.queued_operations).toEqual(
        beforeDisk.sync_state.queued_operations,
      );
    }
  });

  it("applies remote provider document property value set imports", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Properties",
      content: "local draft",
    });
    const status = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "text",
    });
    const owner = await dataClient.createPropertySchema("local-space", {
      name: "Owner",
      property_type: "user",
    });
    await dataClient.setDocumentPropertyValues("local-space", created.id, [
      {
        source_id: "local-status-value",
        property_id: status.id,
        value: "Local",
        created_at: "2026-07-09T11:00:00.000Z",
      },
    ]);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-property-values-set",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-property-values-set-1",
            entity_type: "document_property_values",
            entity_id: created.id,
            action: "set",
            payload_status: "available",
            payload: {
              document_id: created.id,
              space_id: "local-space",
              values: [
                {
                  id: "remote-status-value",
                  property_id: status.id,
                  value: "Remote",
                  created_at: "2026-07-09T12:00:00.000Z",
                  updated_at: "2026-07-09T12:01:00.000Z",
                },
                {
                  id: "remote-owner-value",
                  property_id: owner.id,
                  value: "local-user",
                  created_at: "2026-07-09T12:00:00.000Z",
                },
              ],
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(dataClient.listDocumentProperties("local-space", created.id)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ property_id: status.id, value: "Remote" }),
        expect.objectContaining({ property_id: owner.id, value: "local-user" }),
      ]),
    );
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.property_values).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-status-value",
          document_id: created.id,
          property_id: status.id,
          value: "Remote",
        }),
        expect.objectContaining({
          id: "remote-owner-value",
          document_id: created.id,
          property_id: owner.id,
          value: "local-user",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("fails remote provider document property value imports with missing prerequisites", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Property Prereqs",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-property-values-prereqs",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-document-property-values-missing-doc",
            entity_type: "document_property_values",
            entity_id: "missing-document",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "missing-document",
              space_id: "local-space",
              values: [],
            },
          },
          {
            id: "remote-document-property-values-missing-schema",
            entity_type: "document_property_values",
            entity_id: created.id,
            action: "set",
            payload_status: "available",
            payload: {
              document_id: created.id,
              space_id: "local-space",
              values: [{ property_id: "missing-property", value: "Remote" }],
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-property-values-missing-doc",
          status: "failed",
        }),
        expect.objectContaining({
          operation_id: "remote-document-property-values-missing-schema",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.listDocumentProperties("local-space", created.id)).resolves.toEqual([]);
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("rejects remote provider document property value ID conflicts and duplicates", async () => {
    const conflictDoc = await dataClient.createDocument("local-space", {
      title: "Remote Property Conflict",
      content: "local draft",
    });
    const duplicateDoc = await dataClient.createDocument("local-space", {
      title: "Remote Property Duplicate",
      content: "local draft",
    });
    const ownerDoc = await dataClient.createDocument("local-space", {
      title: "Remote Property Existing Owner",
      content: "local draft",
    });
    const status = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", ownerDoc.id, [
      {
        source_id: "existing-property-value-id",
        property_id: status.id,
        value: "Existing",
      },
    ]);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-property-values-integrity",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-document-property-values-id-conflict",
            entity_type: "document_property_values",
            entity_id: conflictDoc.id,
            action: "set",
            payload_status: "available",
            payload: {
              document_id: conflictDoc.id,
              space_id: "local-space",
              values: [
                {
                  id: "existing-property-value-id",
                  property_id: status.id,
                  value: "Remote",
                },
              ],
            },
          },
          {
            id: "remote-document-property-values-duplicate-property",
            entity_type: "document_property_values",
            entity_id: duplicateDoc.id,
            action: "set",
            payload_status: "available",
            payload: {
              document_id: duplicateDoc.id,
              space_id: "local-space",
              values: [
                { property_id: status.id, value: "First" },
                { property_id: status.id, value: "Second" },
              ],
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-property-values-id-conflict",
          status: "failed",
          message:
            "Remote document property values payload ID conflicts with another local property value",
        }),
        expect.objectContaining({
          operation_id: "remote-document-property-values-duplicate-property",
          status: "skipped",
          message:
            "Remote replay operation document_property_values:set payload includes duplicate property values",
        }),
      ]),
    });
    await expect(dataClient.listDocumentProperties("local-space", conflictDoc.id)).resolves.toEqual(
      expect.arrayContaining([expect.objectContaining({ property_id: status.id, value: null })]),
    );
    await expect(
      dataClient.listDocumentProperties("local-space", duplicateDoc.id),
    ).resolves.toEqual(
      expect.arrayContaining([expect.objectContaining({ property_id: status.id, value: null })]),
    );
    await expect(dataClient.listDocumentProperties("local-space", ownerDoc.id)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ property_id: status.id, value: "Existing" }),
      ]),
    );
    expect(await dataClient.getSyncState()).toBe("error");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider browser asset create imports", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Asset",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-asset-create-1",
            entity_type: "asset",
            entity_id: "remote-asset-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-asset-1",
              space_id: "local-space",
              document_id: created.id,
              name: "remote.txt",
              mime_type: "text/plain",
              size: 5,
              url: "data:text/plain;base64,SGVsbG8=",
              source: "browser",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-asset-1",
          name: "remote.txt",
          mime_type: "text/plain",
          size: 5,
          url: "data:text/plain;base64,SGVsbG8=",
          source: "browser",
          space_id: "local-space",
          document_id: created.id,
          created_at: "2026-07-09T12:00:00.000Z",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles duplicate and missing-byte remote provider asset imports", async () => {
    const existing = await dataClient.saveBrowserAsset({
      name: "existing.txt",
      mime_type: "text/plain",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: "local-space",
      document_id: "local-welcome",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-asset-create-existing-match",
            entity_type: "asset",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              document_id: "local-welcome",
              name: existing.name,
              mime_type: existing.mime_type,
              size: existing.size,
              url: existing.url,
              source: "browser",
              created_at: existing.created_at,
            },
          },
          {
            id: "remote-asset-create-existing",
            entity_type: "asset",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              document_id: "local-welcome",
              name: "duplicate.txt",
              size: 5,
              url: "data:text/plain;base64,SGVsbG8=",
              source: "browser",
            },
          },
          {
            id: "remote-asset-create-missing-bytes",
            entity_type: "asset",
            entity_id: "remote-asset-missing-bytes",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-asset-missing-bytes",
              space_id: "local-space",
              document_id: "local-welcome",
              name: "missing.txt",
              size: 5,
              url: "asset://localhost/missing.txt",
              source: "tauri",
            },
          },
          {
            id: "remote-asset-create-missing-doc",
            entity_type: "asset",
            entity_id: "remote-asset-missing-doc",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-asset-missing-doc",
              space_id: "local-space",
              document_id: "missing-document",
              name: "missing-doc.txt",
              size: 5,
              url: "data:text/plain;base64,SGVsbG8=",
              source: "browser",
            },
          },
          {
            id: "remote-asset-create-size-mismatch",
            entity_type: "asset",
            entity_id: "remote-asset-size-mismatch",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-asset-size-mismatch",
              space_id: "local-space",
              document_id: "local-welcome",
              name: "wrong-size.txt",
              size: 99,
              url: "data:text/plain;base64,SGVsbG8=",
              source: "browser",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 3,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-asset-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-asset-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-asset-create-missing-bytes",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-asset-create-missing-doc",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-asset-create-size-mismatch",
          status: "skipped",
          message:
            "Remote replay operation asset:create payload byte size does not match declared size",
        }),
      ]),
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets.filter((asset) => asset.id === existing.id)).toHaveLength(1);
    expect(exported.assets.some((asset) => asset.id === "remote-asset-missing-bytes")).toBe(false);
    expect(exported.assets.some((asset) => asset.id === "remote-asset-missing-doc")).toBe(false);
    expect(exported.assets.some((asset) => asset.id === "remote-asset-size-mismatch")).toBe(false);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider asset deletes through the local vault merge executor", async () => {
    const existing = await dataClient.saveBrowserAsset({
      name: "remove.txt",
      mime_type: "text/plain",
      size: 6,
      url: "data:text/plain;base64,UmVtb3Zl",
      space_id: "local-space",
      document_id: "local-welcome",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-delete",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-asset-delete-existing",
            entity_type: "asset",
            entity_id: existing.id,
            action: "delete",
            payload_status: "available",
          },
          {
            id: "remote-asset-delete-missing",
            entity_type: "asset",
            entity_id: "missing-asset",
            action: "delete",
            payload_status: "available",
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 2,
      applied_operation_count: 1,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-asset-delete-existing",
          status: "applied",
        },
        {
          operation_id: "remote-asset-delete-missing",
          status: "skipped",
        },
      ],
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets.some((asset) => asset.id === existing.id)).toBe(false);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider asset deletes when a local queued operation targets the same asset", async () => {
    const existing = await dataClient.saveBrowserAsset({
      name: "pending-local-asset.txt",
      mime_type: "text/plain",
      size: 7,
      url: "data:text/plain;base64,UGVuZGluZw==",
      space_id: "local-space",
      document_id: "local-welcome",
    });
    const queuedOperations = await dataClient.listQueuedSyncOperations();
    expect(
      queuedOperations.some(
        (operation) => operation.entity_type === "asset" && operation.entity_id === existing.id,
      ),
    ).toBe(true);
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-delete-conflict",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-asset-delete-conflict",
            entity_type: "asset",
            entity_id: existing.id,
            action: "delete",
            payload_status: "available",
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-asset-delete-conflict",
          status: "blocked",
          message: "A local queued operation targets the same entity",
        },
      ],
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets.some((asset) => asset.id === existing.id)).toBe(true);
    expect(await dataClient.getSyncState()).toBe("conflict");
  });

  it("rejects remote provider asset delete tombstones with mismatched document context", async () => {
    const otherSpace = await dataClient.createSpace("Other Space");
    const existing = await dataClient.saveBrowserAsset({
      name: "mismatched-context.txt",
      mime_type: "text/plain",
      size: 7,
      url: "data:text/plain;base64,Q29udGV4dA==",
      space_id: "local-space",
      document_id: "local-welcome",
    });
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-delete-context",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-asset-delete-context",
            entity_type: "asset",
            entity_id: existing.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: existing.id,
              document_id: "local-welcome",
              space_id: otherSpace.id,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-asset-delete-context",
          status: "failed",
          message: "Remote asset delete payload space does not match local document",
        },
      ],
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets.some((asset) => asset.id === existing.id)).toBe(true);
    expect(await dataClient.getSyncState()).toBe("error");
  });

  it("applies remote provider document workflow create, update, transition, and delete imports", async () => {
    const template = await dataClient.createWorkflowTemplate("local-space", {
      name: "Remote Workflow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
        { id: "done", name: "Done", sort_order: 2 },
      ],
      approvers: [{ approver_type: "role", approver_id: "admin", stage_id: "review" }],
    });
    const createdDoc = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Create",
    });
    const updatedDoc = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Update",
    });
    const transitionedDoc = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Transition",
    });
    const deletedDoc = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Delete",
    });
    const updatedWorkflow = await dataClient.enableDocumentWorkflow("local-space", updatedDoc.id, {
      template_id: template.id,
      stage_id: "draft",
    });
    const transitionedWorkflow = await dataClient.enableDocumentWorkflow(
      "local-space",
      transitionedDoc.id,
      { template_id: template.id, stage_id: "draft" },
    );
    const deletedWorkflow = await dataClient.enableDocumentWorkflow("local-space", deletedDoc.id, {
      template_id: template.id,
      stage_id: "draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-workflow-crud",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 4,
        operations: [
          {
            id: "remote-document-workflow-create-1",
            entity_type: "document_workflow",
            entity_id: "remote-workflow-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-workflow-created",
              document_id: createdDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "draft",
              status: "active",
              created_by: "remote-user",
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-update-1",
            entity_type: "document_workflow",
            entity_id: updatedWorkflow.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: updatedWorkflow.id,
              document_id: updatedDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "review",
              status: "active",
              updated_at: "2026-07-09T12:02:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-transition-1",
            entity_type: "document_workflow",
            entity_id: transitionedWorkflow.id,
            action: "transition",
            payload_status: "available",
            payload: {
              id: transitionedWorkflow.id,
              document_id: transitionedDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "done",
              status: "completed",
              updated_at: "2026-07-09T12:03:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-delete-1",
            entity_type: "document_workflow",
            entity_id: deletedWorkflow.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: deletedWorkflow.id,
              document_id: deletedDoc.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 4,
      applied_operation_count: 4,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(
      dataClient.getDocumentWorkflow("local-space", createdDoc.id),
    ).resolves.toMatchObject({
      id: "remote-workflow-created",
      current_stage_id: "draft",
      status: "active",
    });
    await expect(
      dataClient.getDocumentWorkflow("local-space", updatedDoc.id),
    ).resolves.toMatchObject({
      current_stage_id: "review",
      status: "active",
    });
    await expect(
      dataClient.getDocumentWorkflow("local-space", transitionedDoc.id),
    ).resolves.toMatchObject({
      current_stage_id: "done",
      status: "completed",
    });
    await expect(dataClient.getDocumentWorkflow("local-space", deletedDoc.id)).resolves.toBeNull();
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider document workflow imports", async () => {
    const template = await dataClient.createWorkflowTemplate("local-space", {
      name: "Existing Workflow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [],
    });
    const existingDoc = await dataClient.createDocument("local-space", {
      title: "Existing Workflow",
    });
    const existingWorkflow = await dataClient.enableDocumentWorkflow(
      "local-space",
      existingDoc.id,
      {
        template_id: template.id,
        stage_id: "draft",
      },
    );
    const exported = await dataClient.exportLocalVaultSnapshot();
    const existingVaultWorkflow = (exported.document_workflows ?? []).find(
      (workflow) => workflow.id === existingWorkflow.id,
    );
    if (!existingVaultWorkflow) {
      throw new Error("Expected document workflow to be present in local vault snapshot");
    }
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-workflow-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 7,
        operations: [
          {
            id: "remote-document-workflow-create-existing-match",
            entity_type: "document_workflow",
            entity_id: existingWorkflow.id,
            action: "create",
            payload_status: "available",
            payload: {
              ...existingVaultWorkflow,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-workflow-create-existing",
            entity_type: "document_workflow",
            entity_id: existingWorkflow.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existingWorkflow.id,
              document_id: existingDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "review",
              status: "completed",
            },
          },
          {
            id: "remote-document-workflow-create-existing-document",
            entity_type: "document_workflow",
            entity_id: "remote-workflow-different-id",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-workflow-different-id",
              document_id: existingDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "draft",
              status: "active",
            },
          },
          {
            id: "remote-document-workflow-update-missing",
            entity_type: "document_workflow",
            entity_id: "missing-workflow",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-workflow",
              document_id: "missing-document",
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "draft",
              status: "active",
            },
          },
          {
            id: "remote-document-workflow-update-different-id",
            entity_type: "document_workflow",
            entity_id: "remote-workflow-update-different-id",
            action: "update",
            payload_status: "available",
            payload: {
              id: "remote-workflow-update-different-id",
              document_id: existingDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "review",
              status: "active",
            },
          },
          {
            id: "remote-document-workflow-transition-missing-stage",
            entity_type: "document_workflow",
            entity_id: existingWorkflow.id,
            action: "transition",
            payload_status: "available",
            payload: {
              id: existingWorkflow.id,
              document_id: existingDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "missing-stage",
              status: "active",
            },
          },
          {
            id: "remote-document-workflow-delete-missing",
            entity_type: "document_workflow",
            entity_id: "missing-delete-workflow",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-delete-workflow",
              document_id: "missing-delete-document",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-workflow-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-create-existing-document",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-update-different-id",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-transition-missing-stage",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-delete-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(
      dataClient.getDocumentWorkflow("local-space", existingDoc.id),
    ).resolves.toMatchObject({
      id: existingWorkflow.id,
      current_stage_id: "draft",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document workflow event imports", async () => {
    const template = await dataClient.createWorkflowTemplate("local-space", {
      name: "Remote Workflow Events",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [],
    });
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Event",
      content: "local draft",
    });
    const workflow = await dataClient.enableDocumentWorkflow("local-space", created.id, {
      template_id: template.id,
      stage_id: "draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-workflow-event-delegate",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-workflow-event-delegate-1",
            entity_type: "document_workflow_event",
            entity_id: "remote-workflow-event-1",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "remote-workflow-event-1",
              document_workflow_id: workflow.id,
              document_id: created.id,
              space_id: "local-space",
              from_stage_id: "draft",
              to_stage_id: "review",
              action: "delegate",
              note: "Remote handoff",
              created_by: "remote-user",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(dataClient.listDocumentWorkflowEvents("local-space", created.id)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-workflow-event-1",
          document_workflow_id: workflow.id,
          action: "delegate",
          note: "Remote handoff",
          created_by: "remote-user",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and invalid remote provider document workflow event imports", async () => {
    const template = await dataClient.createWorkflowTemplate("local-space", {
      name: "Remote Workflow Event Edges",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [],
    });
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Event Edges",
      content: "local draft",
    });
    const workflow = await dataClient.enableDocumentWorkflow("local-space", created.id, {
      template_id: template.id,
      stage_id: "draft",
    });
    const otherSpace = await dataClient.createSpace("Other Workflow Event Space");
    await dataClient.recordDocumentWorkflowEvent("local-space", created.id, {
      action: "delegate",
      note: "Existing event",
      source_id: "existing-workflow-event",
      to_stage_id: "review",
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    const existingVaultEvent = (exported.document_workflow_events ?? []).find(
      (event) => event.id === "existing-workflow-event",
    );
    if (!existingVaultEvent) {
      throw new Error("Expected workflow event to be present in local vault snapshot");
    }
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-workflow-event-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-document-workflow-event-existing-match",
            entity_type: "document_workflow_event",
            entity_id: "existing-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              ...existingVaultEvent,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-workflow-event-existing",
            entity_type: "document_workflow_event",
            entity_id: "existing-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "existing-workflow-event",
              document_workflow_id: workflow.id,
              document_id: created.id,
              space_id: "local-space",
              to_stage_id: "review",
              action: "delegate",
              note: "Duplicate event",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-event-existing-wrong-space",
            entity_type: "document_workflow_event",
            entity_id: "existing-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              ...existingVaultEvent,
              space_id: otherSpace.id,
            },
          },
          {
            id: "remote-document-workflow-event-missing-workflow",
            entity_type: "document_workflow_event",
            entity_id: "missing-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "missing-workflow-event",
              document_workflow_id: "missing-workflow",
              document_id: created.id,
              space_id: "local-space",
              to_stage_id: "review",
              action: "delegate",
              note: "Missing workflow",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-event-bad-action",
            entity_type: "document_workflow_event",
            entity_id: "bad-action-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "bad-action-workflow-event",
              document_workflow_id: workflow.id,
              document_id: created.id,
              space_id: "local-space",
              to_stage_id: "review",
              action: "escalate",
              note: "Bad action",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 2,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-existing-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-missing-workflow",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-bad-action",
          status: "skipped",
          message:
            "Remote replay operation document_workflow_event:delegate payload action does not match operation action",
        }),
      ]),
    });
    const events = await dataClient.listDocumentWorkflowEvents("local-space", created.id);
    expect(events.filter((event) => event.id === "existing-workflow-event")).toHaveLength(1);
    expect(events.some((event) => event.id === "missing-workflow-event")).toBe(false);
    expect(events.some((event) => event.id === "bad-action-workflow-event")).toBe(false);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks server-only remote provider import operations without mutating local data", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Unsupported Remote Merge",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-comment-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 9,
        operations: [
          {
            id: "remote-comment-create-1",
            entity_type: "comment",
            entity_id: "remote-comment-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-comment-1",
              document_id: created.id,
              body: "Unsupported remote comment",
            },
          },
          {
            id: "remote-document-publish-1",
            entity_type: "document",
            entity_id: created.id,
            action: "publish",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              public_slug: "remote-public-slug",
            },
          },
          {
            id: "remote-document-unpublish-1",
            entity_type: "document",
            entity_id: created.id,
            action: "unpublish",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-reaction-toggle-1",
            entity_type: "document_reaction",
            entity_id: `${created.id}:thumbs-up`,
            action: "toggle",
            payload_status: "available",
            payload: {
              document_id: created.id,
              space_id: "local-space",
              emoji: "thumbs-up",
            },
          },
          {
            id: "remote-document-version-restore-1",
            entity_type: "document_version",
            entity_id: "remote-version-1",
            action: "restore",
            payload_status: "available",
            payload: {
              id: "remote-version-1",
              document_id: created.id,
              space_id: "local-space",
            },
          },
          {
            id: "remote-webhook-create-1",
            entity_type: "webhook",
            entity_id: "remote-webhook-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-webhook-1",
              url: "https://example.com/webhook",
            },
          },
          {
            id: "remote-notification-read-1",
            entity_type: "notification",
            entity_id: "remote-notification-1",
            action: "read",
            payload_status: "available",
            payload: {
              id: "remote-notification-1",
              read: true,
            },
          },
          {
            id: "remote-api-key-create-1",
            entity_type: "api_key",
            entity_id: "remote-api-key-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-api-key-1",
              name: "Remote key",
            },
          },
          {
            id: "remote-space-member-add-1",
            entity_type: "space_member",
            entity_id: "local-space:remote-user-1",
            action: "add",
            payload_status: "available",
            payload: {
              space_id: "local-space",
              user_id: "remote-user-1",
              role: "viewer",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-comment-create-1",
          status: "blocked",
          message:
            "Remote replay operation comment:create is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-document-publish-1",
          status: "blocked",
          message:
            "Remote replay operation document:publish is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-document-unpublish-1",
          status: "blocked",
          message:
            "Remote replay operation document:unpublish is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-document-reaction-toggle-1",
          status: "blocked",
          message:
            "Remote replay operation document_reaction:toggle is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-document-version-restore-1",
          status: "blocked",
          message:
            "Remote replay operation document_version:restore is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-webhook-create-1",
          status: "blocked",
          message:
            "Remote replay operation webhook:create is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-notification-read-1",
          status: "blocked",
          message:
            "Remote replay operation notification:read is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-api-key-create-1",
          status: "blocked",
          message:
            "Remote replay operation api_key:create is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-space-member-add-1",
          status: "blocked",
          message:
            "Remote replay operation space_member:add is server-only and cannot be imported into a local vault",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local draft",
    });
    expect(await dataClient.getSyncState()).toBe("conflict");
  });

  it("blocks oversized remote provider document creates without mutating local data", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-oversized-document-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-create-oversized",
            entity_type: "document",
            entity_id: "remote-oversized-doc",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-oversized-doc",
              space_id: "local-space",
              title: "Oversized Remote Doc",
              content: "x".repeat(5 * 1024 * 1024 + 1),
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 1,
      results: [
        {
          operation_id: "remote-document-create-oversized",
          status: "failed",
          message: "Remote replay operation document:create payload content exceeds 5 MB limit",
        },
      ],
    });
    await expect(dataClient.getDocument("local-space", "remote-oversized-doc")).rejects.toThrow(
      "Document not found",
    );
    expect(await dataClient.getSyncState()).toBe("error");
  });

  it("blocks remote provider document imports when a local queued operation targets the same entity", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Local Pending Edit",
      content: "local draft",
    });
    await dataClient.updateDocumentContent("local-space", created.id, "local pending draft");
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-conflict",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-conflict",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Local Pending Edit",
              content: "remote draft",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-update-conflict",
          status: "blocked",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local pending draft",
    });
    expect(await dataClient.getSyncState()).toBe("conflict");
  });

  it("retains independent browser uploads under each original local draft incarnation", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Scoped assets", content: "raw source\r\n" });
    const first = await dataClient.getDocumentDraft("local-space", page.id, page.draft_id!);
    const second = await dataClient.createDocumentDraft("local-space", page.id, { force_new: true });
    const before = await dataClient.exportLocalVaultSnapshot();
    const assets = [];
    for (const draft of [first, second, first]) assets.push(await dataClient.saveDocumentDataAsset({ name: "same.txt", mime_type: "text/plain", data_url: "data:text/plain;base64,QQBCDQou", space_id: "local-space", document_id: page.id, draft_id: draft.id }));
    expect(new Set(assets.map(asset => asset.id)).size).toBe(3);
    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    const operations = manifest.operations.filter(op => op.entity_type === "asset");
    expect(operations).toHaveLength(3);
    assets.forEach((asset, index) => expect(operations.find(op => op.entity_id === asset.id)).toMatchObject({ action: "create", payload: { draft_id: [first, second, first][index].id, local_draft_source: { schema_version: 1, space_id: "local-space", document_id: page.id, draft_id: [first, second, first][index].id, draft_created_at: [first, second, first][index].created_at }, data_url: "data:text/plain;base64,QQBCDQou" } }));
    expect(first.id).toMatch(/^draft-/);
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(after.drafts).toEqual(before.drafts);
    expect(after.documents).toEqual(before.documents);
    expect(after.sync_state?.queued_operations?.filter(op => op.entity_type !== "asset")).toEqual(before.sync_state?.queued_operations);
  });

  it("streams a draft-scoped browser upload into the native original-import owner", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Native browser bytes", content: "exact source" });
    const original = await dataClient.getDocumentDraft("local-space", page.id, page.draft_id!);
    const before = await dataClient.exportLocalVaultSnapshot();
    const restoreAdapter = installNativeImportJournalTestDouble();
    const invoke = nativeBlobImportProtocol(before);
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    try {
      const asset = await dataClient.saveDocumentDataAsset({ name: "browser.txt", mime_type: "text/plain", data_url: "data:text/plain;base64,QQBCDQou", space_id: "local-space", document_id: page.id, draft_id: original.id });
      expect(asset).toMatchObject({ id: expect.stringMatching(/^asset-import-/), source: "tauri", size: 6 });
      expect(invoke).toHaveBeenCalledWith("append_original_native_local_import", expect.objectContaining({ index: 0, bytes: [65, 0, 66, 13, 10, 46] }));
      expect(invoke).not.toHaveBeenCalledWith("import_vault_asset_from_data_url", expect.anything());
      const persisted = await createLocalVaultAdapter("none").loadSnapshot();
      const operation = persisted.sync_state?.queued_operations.find(row => row.entity_id === asset.id);
      expect(operation).toMatchObject({ id: `syncop-import-${asset.id.slice("asset-import-".length)}`, action: "create", payload: { draft_id: original.id, local_draft_source: { schema_version: 1, space_id: "local-space", document_id: page.id, draft_id: original.id, draft_created_at: original.created_at } } });
      expect(operation?.payload).not.toHaveProperty("data_url");
      expect((await dataClient.exportQueuedSyncReplayManifest()).operations.find(row => row.entity_id === asset.id)).toMatchObject({ payload_status: "available", payload: { data_url: "data:text/plain;base64,QQBCDQou" } });
    } finally { restoreAdapter(); delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__; }
  });

  it("hydrates genuine native-prefix import payload without replacing its original local draft", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Native scope", content: "exact source\r\n" });
    const original = await dataClient.getDocumentDraft("local-space", page.id, page.draft_id!);
    const before = await dataClient.exportLocalVaultSnapshot();
    const context = { space_id: "local-space", document_id: page.id, draft_id: original.id };
    let release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    const restoreAdapter = installNativeImportJournalTestDouble();
    const invoke = nativePathImportProtocol(before, held);
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    try {
      const pending = dataClient.importAssetFromPath("/selected/native.txt", context);
      await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("run_original_native_local_import_path", expect.anything()));
      context.draft_id = "draft-other-selection";
      release();
      const asset = await pending;
      const manifest = await dataClient.exportQueuedSyncReplayManifest();
      const operation = manifest.operations.find(op => op.entity_type === "asset")!;
      expect(asset.id).toMatch(/^asset-import-/);
      expect(operation).toMatchObject({ entity_id: asset.id, action: "create", payload_status: "available", payload: { draft_id: original.id, local_draft_source: { schema_version: 1, space_id: "local-space", document_id: page.id, draft_id: original.id, draft_created_at: original.created_at }, data_url: "data:text/plain;base64,QQBCDQou" } });
      expect((await dataClient.getDocumentDraft("local-space", page.id, original.id)).content).toBe("exact source\r\n");
    } finally { restoreAdapter(); delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__; }
  });

  it("reads back only the exact retained committed native import operation", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Native readback", content: "exact source" });
    const original = await dataClient.getDocumentDraft("local-space", page.id, page.draft_id!);
    const before = await dataClient.exportLocalVaultSnapshot();
    const restoreAdapter = installNativeImportJournalTestDouble();
    const invoke = nativePathImportProtocol(before);
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    try {
      const asset = await dataClient.importAssetFromPath("/selected/native.txt", { space_id: "local-space", document_id: page.id, draft_id: original.id });
      const operationId = [...restoreAdapter.stores.get("progress")!.keys()][0];
      const { reconcileOriginalNativeLocalImport } = await import("./nativeLocalImport");
      const admit = vi.fn(async () => undefined);
      const readback = await reconcileOriginalNativeLocalImport(operationId, { space_id: "local-space", document_id: page.id, draft_id: original.id, draft_created_at: original.created_at }, () => undefined, admit);
      expect(readback).toEqual({ operationId, asset });
      expect(invoke).toHaveBeenCalledWith("inspect_original_native_local_import", expect.objectContaining({ operationId }));
      expect(admit).toHaveBeenCalledOnce();
    } finally { restoreAdapter(); delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__; }
  });

  it("holds returned native bytes on account drift without queuing them under a new context", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Held native", content: "unchanged" });
    const before = await dataClient.exportLocalVaultSnapshot();
    let release!: () => void;
    const held = new Promise<void>(resolve => { release = resolve; });
    const restoreAdapter = installNativeImportJournalTestDouble();
    const invoke = nativePathImportProtocol(before, held);
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    try {
      const pending = dataClient.importAssetFromPath("/selected/native.txt", { space_id: "local-space", document_id: page.id, draft_id: page.draft_id });
      await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("run_original_native_local_import_path", expect.anything()));
      api.setToken("changed-account"); release();
      await expect(pending).rejects.toMatchObject({ name: "LocalAssetImportHeldError", asset: { id: expect.stringMatching(/^asset-import-/), vault_path: expect.stringContaining("/assets/local-imports/") }, original: { source: { draft_id: page.draft_id } } });
    } finally { restoreAdapter(); delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__; api.setToken(null); }
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(after.assets).toEqual(before.assets);
    expect(after.sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
    expect(after.drafts).toEqual(before.drafts);
  });

  it("rejects a mismatched original local draft before native import and leaves old unscoped queues unchanged", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Legacy", content: "source" });
    await dataClient.saveBrowserAsset({ name: "legacy.txt", size: 1, url: "data:text/plain;base64,QQ==", space_id: "local-space", document_id: page.id });
    const before = await dataClient.exportLocalVaultSnapshot();
    const invoke = vi.fn();
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    try { await expect(dataClient.importAssetFromPath("/selected/file", { space_id: "local-space", document_id: page.id, draft_id: "draft-missing" })).rejects.toThrow("original local draft"); expect(invoke).not.toHaveBeenCalled(); }
    finally { delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__; }
    const after = await dataClient.exportLocalVaultSnapshot();
    // loadSnapshot stamps sync_state.updated_at at export time (localVault.ts),
    // so normalize only that derived field; every persisted field stays exact.
    expect({ ...after, sync_state: { ...after.sync_state, updated_at: before.sync_state?.updated_at } }).toEqual(before);
    const queued = after.sync_state?.queued_operations?.find(op => op.entity_type === "asset");
    expect(queued?.payload).not.toHaveProperty("draft_id");
    expect(queued?.payload).not.toHaveProperty("local_draft_source");
  });

  it("retains original asset and queue atomically when provider changes after their durable commit", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Commit drift", content: "exact original" });
    const foreignBefore = await createLocalVaultAdapter("google-drive").loadSnapshot();
    let release!: () => void, committed!: () => void;
    const wait = new Promise<void>(resolve => { release = resolve; });
    const ready = new Promise<void>(resolve => { committed = resolve; });
    let pause = true;
    (window as unknown as { navigator?: unknown }).navigator = { locks: { request: async (_name: string, run: () => Promise<unknown>) => { const result = await run(); if (pause) { pause = false; committed(); await wait; } return result; } } };
    const pending = dataClient.saveDocumentDataAsset({ name: "owned.txt", mime_type: "text/plain", data_url: "data:text/plain;base64,QQBCDQou", space_id: "local-space", document_id: page.id, draft_id: page.draft_id });
    await ready;
    setSyncProvider("google-drive"); release();
    await expect(pending).rejects.toMatchObject({ name: "LocalAssetImportHeldError", original: { source: { draft_id: page.draft_id } } });
    const original = await createLocalVaultAdapter("none").loadSnapshot();
    const operation = original.sync_state?.queued_operations?.find(op => op.entity_type === "asset");
    expect(operation).toBeDefined();
    if (!operation) throw new Error("Original scoped asset operation was not retained");
    expect(operation).toMatchObject({ action: "create", payload: { draft_id: page.draft_id, data_url: "data:text/plain;base64,QQBCDQou" } });
    expect(original.assets.find(asset => asset.id === operation.entity_id)).toMatchObject({ name: "owned.txt", url: "data:text/plain;base64,QQBCDQou" });
    expect(original.drafts?.find(draft => draft.id === page.draft_id)?.content).toBe("exact original");
    const foreignAfter = await createLocalVaultAdapter("google-drive").loadSnapshot();
    expect({ ...foreignAfter, sync_state: { ...foreignAfter.sync_state, updated_at: foreignBefore.sync_state?.updated_at } }).toEqual(foreignBefore);
    setSyncProvider("none");
  });

  it("checks persisted draft incarnation rather than stale local cache before asset commit", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Persisted incarnation", content: "original" });
    const before = await dataClient.exportLocalVaultSnapshot();
    let release!: () => void;
    const wait = new Promise<void>(resolve => { release = resolve; });
    const restoreAdapter = installNativeImportJournalTestDouble();
    const invoke = nativePathImportProtocol(before, wait);
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    try {
      const pending = dataClient.importAssetFromPath("/selected/native.txt", { space_id: "local-space", document_id: page.id, draft_id: page.draft_id });
      await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("run_original_native_local_import_path", expect.anything()));
      // A second window replaced the persisted incarnation; this dataClient's
      // localDrafts cache deliberately still contains the original created_at.
      const replacement = JSON.parse(storage.get("starlane_local_vault_v1_none")!);
      replacement.drafts.find((draft: { id: string }) => draft.id === page.draft_id).created_at = "2027-01-01T00:00:00Z";
      storage.set("starlane_local_vault_v1_none", JSON.stringify(replacement));
      release();
      await expect(pending).rejects.toMatchObject({ name: "LocalAssetImportHeldError", asset: { id: expect.stringMatching(/^asset-import-/) } });
    } finally { restoreAdapter(); delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__; }
    const persisted = await createLocalVaultAdapter("none").loadSnapshot();
    expect(persisted.assets).toEqual(before.assets);
    expect(persisted.sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
    expect(persisted.drafts?.find(draft => draft.id === page.draft_id)?.created_at).toBe("2027-01-01T00:00:00Z");
  });

  it("blocks queued asset replay when binary bytes are unavailable", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Asset",
      content: "asset reference",
    });
    await dataClient.saveBrowserAsset({
      name: "remote.png",
      mime_type: "image/png",
      size: 128,
      url: "asset://localhost/local-only.png",
      space_id: "local-space",
      document_id: created.id,
    });

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    const assetOperation = manifest.operations.find(
      (operation) => operation.entity_type === "asset" && operation.action === "create",
    );
    expect(assetOperation).toMatchObject({
      payload_status: "missing",
      payload: expect.objectContaining({
        name: "remote.png",
        space_id: "local-space",
        document_id: created.id,
      }),
    });
    expect(manifest.replay_readiness.ready).toBe(false);
    expect(manifest.operation_summary.missing_payload_count).toBe(1);
    expect(manifest.operation_summary.unavailable_payloads).toEqual([
      expect.objectContaining({
        operation_id: assetOperation?.id,
        entity_type: "asset",
        action: "create",
        reason: "Asset binary bytes are unavailable for replay",
        asset_name: "remote.png",
        document_id: created.id,
        source_url: "asset://localhost/local-only.png",
        byte_sources_checked: ["asset://localhost/local-only.png"],
        recovery_action:
          "Restore the asset file under the active vault or reattach the asset so replay can embed bytes.",
      }),
    ]);
  });

  it("preserves native asset byte packing failures in replay blockers", async () => {
    (
      window as unknown as { __TAURI_INTERNALS__?: { invoke: () => Promise<string> } }
    ).__TAURI_INTERNALS__ = {
      invoke: async () => {
        throw new Error("Native replay asset file was not found");
      },
    };
    const created = await dataClient.createDocument("local-space", {
      title: "Native Asset",
      content: "asset reference",
    });
    await dataClient.saveBrowserAsset({
      name: "native.png",
      mime_type: "image/png",
      size: 128,
      url: "asset://localhost/native.png",
      space_id: "local-space",
      document_id: created.id,
    });

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    const assetOperation = manifest.operations.find(
      (operation) => operation.entity_type === "asset" && operation.action === "create",
    );
    expect(assetOperation).toMatchObject({
      payload_status: "missing",
      payload: expect.objectContaining({
        native_pack_error: "Native replay asset file was not found",
      }),
    });
    expect(manifest.operation_summary.unavailable_payloads).toEqual([
      expect.objectContaining({
        operation_id: assetOperation?.id,
        reason: "Native replay asset file was not found",
        byte_sources_checked: ["asset://localhost/native.png"],
      }),
    ]);
  });

  it("returns local search results and empty notifications in local mode", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Roadmap Notes",
      content: "alpha beta gamma",
    });
    const untagged = await dataClient.createDocument("local-space", {
      title: "Plain Notes",
      content: "plain beta gamma",
    });
    const tag = await dataClient.createTag("local-space", { name: "Planning" });
    await dataClient.addDocumentTag("local-space", created.id, tag.id);
    const property = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "select",
      options: ["Draft", "Published"],
    });
    await dataClient.setDocumentPropertyValues("local-space", created.id, [
      { property_id: property.id, value: "Draft" },
    ]);
    await dataClient.updateDocument("local-space", created.id, {
      cover_image: "starlane://asset/cover-roadmap",
    });
    await dataClient.saveBrowserAsset({
      name: "roadmap.txt",
      mime_type: "text/plain",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: "local-space",
      document_id: created.id,
    });

    const results = await dataClient.searchDocuments("alpha", 10);
    expect(results.some((r) => r.document_id === created.id)).toBe(true);
    let exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.search_index?.find((entry) => entry.document_id === created.id)).toMatchObject({
      title: "Roadmap Notes",
      space_id: "local-space",
    });

    await dataClient.updateDocumentContent("local-space", created.id, "delta epsilon");
    const updatedResults = await dataClient.searchDocuments("delta", 10);
    expect(updatedResults.map((r) => r.document_id)).toContain(created.id);
    exported = await dataClient.exportLocalVaultSnapshot();
    expect(
      exported.search_index?.find((entry) => entry.document_id === created.id)?.search_text,
    ).toContain("delta");

    const tagResults = await dataClient.searchDocuments("", 10, { tag: "Planning" });
    expect(tagResults.map((r) => r.document_id)).toContain(created.id);

    const folder = await dataClient.createFolder("local-space", "Roadmaps");
    await dataClient.updateDocument("local-space", created.id, { folder_id: folder.id });
    const folderResults = await dataClient.searchDocuments("", 10, { folder: "Roadmaps" });
    expect(folderResults.map((r) => r.document_id)).toContain(created.id);
    const folderIdResults = await dataClient.searchDocuments("", 10, { folder: folder.id });
    expect(folderIdResults.map((r) => r.document_id)).toContain(created.id);

    const propertyResults = await dataClient.searchDocuments("", 10, {
      property_name: "Status",
      property_value: "Draft",
    });
    expect(propertyResults.map((r) => r.document_id)).toContain(created.id);

    const combinedResults = await dataClient.searchDocuments("alpha", 10, {
      property_name: "Status",
      property_value: "Missing",
    });
    expect(combinedResults.map((r) => r.document_id)).not.toContain(created.id);

    const negativePresenceResults = await dataClient.searchDocuments("", 20, {
      has_attachment: "false",
      has_cover: "false",
      has_workflow: "false",
      has_tag: "false",
      has_property: "false",
      is_locked: "false",
    });
    expect(negativePresenceResults.map((r) => r.document_id)).toContain(untagged.id);
    expect(negativePresenceResults.map((r) => r.document_id)).not.toContain(created.id);

    const storedDocument = await dataClient.getDocument("local-space", created.id);
    expect(storedDocument.created_at).toBeTruthy();
    expect(storedDocument.updated_at).toBeTruthy();
    const createdDateResults = await dataClient.searchDocuments("", 20, {
      created_on: storedDocument.created_at!.slice(0, 10),
    });
    expect(createdDateResults.map((r) => r.document_id)).toContain(created.id);
    const updatedDateResults = await dataClient.searchDocuments("", 20, {
      updated_on: storedDocument.updated_at!.slice(0, 10),
    });
    expect(updatedDateResults.map((r) => r.document_id)).toContain(created.id);
    const missingDateResults = await dataClient.searchDocuments("", 20, {
      updated_on: "1900-01-01",
    });
    expect(missingDateResults.map((r) => r.document_id)).not.toContain(created.id);

    const notifications = await dataClient.listNotifications();
    expect(notifications).toEqual([]);
  });

  it("passes notification pagination and unread filters to the server", async () => {
    setCurrentLaunchMode("server");
    const requests: string[] = [];
    (globalThis as { fetch?: typeof fetch }).fetch = (async (input) => {
      requests.push(String(input));
      return {
        ok: true,
        status: 200,
        text: async () => "[]",
        json: async () => [],
      } as Response;
    }) as typeof fetch;

    await expect(
      dataClient.listNotifications({
        limit: 25,
        unreadOnly: true,
        before: "2026-07-12T20:00:00Z|notification-1",
      }),
    ).resolves.toEqual([]);

    expect(requests).toEqual([
      "/api/v1/notifications?limit=25&before=2026-07-12T20%3A00%3A00Z%7Cnotification-1&unread_only=true",
    ]);
  });

  it("queries local documents with first-pass Dataview predicates", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Dataview Alpha",
      content: "release checklist",
    });
    const other = await dataClient.createDocument("local-space", {
      title: "Dataview Beta",
      content: "published notes",
    });
    const tag = await dataClient.createTag("local-space", { name: "Roadmap" });
    await dataClient.addDocumentTag("local-space", created.id, tag.id);
    const status = await dataClient.createPropertySchema("local-space", {
      name: "reviewDate",
      property_type: "date",
    });
    await dataClient.setDocumentPropertyValues("local-space", created.id, [
      { property_id: status.id, value: "2026-07-06" },
    ]);

    const results = await dataClient.queryDataview(
      'title:"Dataview Alpha" AND tag:#roadmap AND reviewDate is 2026-07-06',
    );
    expect(results.map((result) => result.document_id)).toEqual([created.id]);
    expect(results.map((result) => result.document_id)).not.toContain(other.id);
  });

  it("supports provider sync no-op and conflict simulation in local mode", async () => {
    const pull = await dataClient.syncWithProvider("pull");
    expect(pull.conflicts).toBe(0);
    expect(await dataClient.getSyncState()).toBe("idle");

    await dataClient.simulateSyncConflict();
    const readiness = await dataClient.getProviderSyncReadiness();
    expect(readiness.ready).toBe(false);
    expect(readiness.pending_conflict_count).toBe(1);
    expect(readiness.blockers[0]).toContain("must be resolved");
    await expect(dataClient.syncWithProvider("push")).rejects.toThrow("Provider sync is not ready");
    expect(await dataClient.getSyncState()).toBe("conflict");

    await dataClient.clearSyncIssue();
    expect(await dataClient.getSyncState()).toBe("idle");
  });

  it("supports provider connect/disconnect and conflict resolution actions", async () => {
    setSyncProvider("google-drive");
    const disconnectedReadiness = await dataClient.getProviderSyncReadiness();
    expect(disconnectedReadiness.ready).toBe(false);
    expect(disconnectedReadiness.blockers).toContain("Google Drive is not connected");

    const connected = await dataClient.connectProvider();
    expect(connected.connected).toBe(true);
    expect((await dataClient.getProviderStatus()).connected).toBe(true);
    const connectedReadiness = await dataClient.getProviderSyncReadiness();
    expect(connectedReadiness.ready).toBe(true);
    expect(connectedReadiness.remote_mutation_plans).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          direction: "push",
          status: "dry_run_only",
          remote_mutation_enabled: false,
        }),
      ]),
    );

    await dataClient.simulateSyncConflict();
    await dataClient.resolveSyncConflict("keep-local");
    expect(await dataClient.getSyncState()).toBe("idle");

    await dataClient.simulateSyncConflict();
    await dataClient.resolveSyncConflict("merge");
    expect(await dataClient.getSyncState()).toBe("idle");

    const disconnected = await dataClient.disconnectProvider();
    expect(disconnected.connected).toBe(false);
  });

  it("exposes explicit provider remote sync enablement in local mode", async () => {
    setSyncProvider("google-drive");
    writeProviderSession("google-drive", {
      provider: "google-drive",
      connected: true,
      sync_mode: "manual",
      auth: {
        access_token: "access-token",
        refresh_token: "refresh-token",
        account_label: "eagle@example.com",
        scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        issued_at: "2026-07-08T12:00:00.000Z",
        expires_at: "2099-07-08T13:00:00.000Z",
        provider_root_id: "drive-root-id",
        provider_root_name: "Starlane Notes",
      },
      last_operation: "connect",
    });

    const enabled = await dataClient.enableProviderRemoteSync();
    expect(enabled.remote_contract.status).toBe("enabled");
    expect(enabled.remote_sync_enablement.enabled).toBe(true);
    expect(enabled.remote_mutation_plans).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          direction: "push",
          status: "ready",
          remote_mutation_enabled: true,
          dry_run: false,
        }),
      ]),
    );

    const fetchCalls: Array<{ input: string; init: RequestInit }> = [];
    (globalThis as { fetch?: typeof fetch }).fetch = (async (input, init) => {
      fetchCalls.push({ input: String(input), init: init ?? {} });
      return {
        ok: true,
        status: 200,
        json: async () => ({
          id: "drive-provider-sync-manifest",
          name: "provider-sync-manifest.json",
        }),
      } as Response;
    }) as typeof fetch;

    const syncResult = await dataClient.syncWithProvider("push");
    expect(syncResult.pushed).toBe(1);
    expect(syncResult.remote_mutation_attempt).toEqual(
      expect.objectContaining({
        provider: "google-drive",
        direction: "push",
        status: "applied",
        transport_kind: "provider-api",
        applied: true,
        blockers: [],
      }),
    );
    expect(fetchCalls).toHaveLength(1);
    expect(fetchCalls[0].input).toBe(
      "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart",
    );
    expect(fetchCalls[0].init.method).toBe("POST");
    expect(fetchCalls[0].init.headers).toMatchObject({
      Authorization: "Bearer access-token",
      "X-Starlane-Idempotency-Key": expect.stringContaining("google-drive-push"),
    });
    expect(String(fetchCalls[0].init.body)).toContain("provider-sync-manifest.json");
    expect(String(fetchCalls[0].init.body)).toContain("starlane-provider-sync-manifest");
    expect(String(fetchCalls[0].init.body)).toContain('"replay_manifest"');
    expect(String(fetchCalls[0].init.body)).toContain('"operation_summary"');
    expect(await dataClient.getSyncState()).toBe("idle");

    const disabled = await dataClient.disableProviderRemoteSync("test disabled");
    expect(disabled.remote_contract.status).toBe("contract_only");
    expect(disabled.remote_sync_enablement).toEqual(
      expect.objectContaining({
        enabled: false,
        reason: "test disabled",
      }),
    );
  });

  it("reviews provider pull imports against local queued operations", async () => {
    setSyncProvider("google-drive");
    writeProviderSession("google-drive", {
      provider: "google-drive",
      connected: true,
      sync_mode: "manual",
      auth: {
        access_token: "access-token",
        refresh_token: "refresh-token",
        account_label: "eagle@example.com",
        scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        issued_at: "2026-07-08T12:00:00.000Z",
        expires_at: "2099-07-08T13:00:00.000Z",
        provider_root_id: "drive-root-id",
        provider_root_name: "Starlane Notes",
      },
      last_operation: "connect",
    });
    await dataClient.enableProviderRemoteSync();
    const created = await dataClient.createDocument("local-space", {
      title: "Pending Local",
      content: "local draft",
    });
    await dataClient.updateDocumentContent("local-space", created.id, "local queued draft");
    const tag = await dataClient.createTag("local-space", { name: "Pending Tag" });
    await dataClient.updateTag("local-space", tag.id, { name: "Pending Tag Rename" });

    const remoteEnvelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-10T12:00:00.000Z",
      "remote-pull-review-conflict",
    );
    const fetchCalls: Array<{ input: string; init: RequestInit }> = [];
    (globalThis as { fetch?: typeof fetch }).fetch = (async (input, init) => {
      fetchCalls.push({ input: String(input), init: init ?? {} });
      if (String(input).includes("/drive/v3/files/drive-provider-sync-manifest")) {
        return {
          ok: true,
          status: 200,
          json: async () =>
            buildProviderRemoteSyncManifest(remoteEnvelope, {
              schema_version: 1,
              operation_count: 2,
              operation_summary: {
                ready_operation_count: 2,
                missing_payload_count: 0,
              },
              operations: [
                {
                  id: "remote-doc-update-conflict",
                  entity_type: "document",
                  entity_id: created.id,
                  action: "update_content",
                  payload_status: "available",
                  payload: {
                    id: created.id,
                    space_id: "local-space",
                    title: "Pending Local",
                    content: "remote draft",
                  },
                },
                {
                  id: "remote-document-tag-related-conflict",
                  entity_type: "document_tag",
                  entity_id: `${created.id}:${tag.id}`,
                  action: "add",
                  payload_status: "available",
                  payload: {
                    document_id: created.id,
                    tag_id: tag.id,
                    space_id: "local-space",
                  },
                },
              ],
            }),
        } as Response;
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          files: [
            {
              id: "drive-provider-sync-manifest",
              name: "provider-sync-manifest.json",
              modifiedTime: "2026-07-10T12:00:00.000Z",
            },
          ],
        }),
      } as Response;
    }) as typeof fetch;

    const result = await dataClient.syncWithProvider("pull");

    expect(fetchCalls).toHaveLength(2);
    expect(result.conflicts).toBe(2);
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(result.remote_mutation_attempt?.import_dry_run).toMatchObject({
      operation_count: 2,
      would_apply_count: 0,
      conflict_count: 2,
      operations: [
        {
          operation_id: "remote-doc-update-conflict",
          status: "conflict",
          message: "A local queued operation targets the same entity",
        },
        {
          operation_id: "remote-document-tag-related-conflict",
          status: "conflict",
          message: "A local queued operation targets a related entity",
        },
      ],
    });
    expect(result.remote_mutation_attempt?.import_apply_plan).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 2,
      operations: [
        {
          operation_id: "remote-doc-update-conflict",
          apply_status: "skipped_conflict",
        },
        {
          operation_id: "remote-document-tag-related-conflict",
          apply_status: "skipped_conflict",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local queued draft",
    });
  });

  it("queues, lists, and resolves document-level conflicts by id", async () => {
    const first = await dataClient.createDocument("local-space", {
      title: "Conflict One",
      content: "local a",
    });
    const second = await dataClient.createDocument("local-space", {
      title: "Conflict Two",
      content: "local b",
    });

    const firstConflict = await dataClient.simulateSyncConflictForDocument(first.id);
    const secondConflict = await dataClient.simulateSyncConflictForDocument(second.id);
    expect(firstConflict?.document_id).toBe(first.id);
    expect(secondConflict?.document_id).toBe(second.id);

    const conflicts = await dataClient.listConflicts();
    expect(conflicts).toHaveLength(2);
    expect(conflicts.map((c) => c.document_id)).toEqual(
      expect.arrayContaining([first.id, second.id]),
    );
    expect(await dataClient.getSyncState()).toBe("conflict");

    await dataClient.resolveConflict(firstConflict!.id, "keep-local");
    expect((await dataClient.listConflicts()).map((c) => c.id)).toEqual([secondConflict!.id]);
    expect(await dataClient.getSyncState()).toBe("conflict");

    const secondBefore = await dataClient.getDocumentContent("local-space", second.id);
    await dataClient.resolveConflict(secondConflict!.id, "keep-remote");
    const secondAfter = await dataClient.getDocumentContent("local-space", second.id);
    expect(secondAfter.content).not.toBe(secondBefore.content);
    expect(secondAfter.content).toContain("[Remote change detected");
    expect(await dataClient.listConflicts()).toHaveLength(0);
    expect(await dataClient.getSyncState()).toBe("idle");
  });
  it("retains original attachment bytes and refuses a different-account save after upload waits", async () => {
    setCurrentLaunchMode("server");
    api.setToken(`e30.${btoa(JSON.stringify({ sub: "a", sid: "session-a", token_type: "access" }))}.fixture`);
    let reject!: (error: Error) => void;
    const upload = vi.spyOn(dataClient, "uploadDocumentAttachment").mockImplementation(() => new Promise((_resolve, fail) => { reject = fail; }));
    const mirror = vi.spyOn(dataClient, "saveBrowserAsset");
    const save = dataClient.saveDocumentDataAsset({ name: "original.png", mime_type: "image/png", data_url: "data:image/png;base64,QQ==", space_id: "space-a", document_id: "page-a" });
    api.setToken(`e30.${btoa(JSON.stringify({ sub: "b", sid: "session-b", token_type: "access" }))}.fixture`); reject(new TypeError("Offline"));
    await expect(save).rejects.toThrow("context changed"); expect(listOfflineServerAttachments()).toEqual([expect.objectContaining({ dataUrl: "data:image/png;base64,QQ==", origin: expect.objectContaining({ accountId: "a", sessionId: "session-a" }), replayState: "unknown" })]); expect(mirror).not.toHaveBeenCalled(); upload.mockRestore();
  });

  it("retains known-unsent attachment bytes without invoking upload or native asset persistence", async () => {
    setCurrentLaunchMode("server");
    api.setToken(`e30.${btoa(JSON.stringify({ sub: "a", sid: "session-a", token_type: "access" }))}.fixture`);
    vi.stubGlobal("navigator", { onLine: false });
    const upload = vi.spyOn(dataClient, "uploadDocumentAttachment");
    const mirror = vi.spyOn(dataClient, "saveBrowserAsset");
    try {
      const saved = await dataClient.saveDocumentDataAsset({ name: "unsent.png", mime_type: "image/png", data_url: "data:image/png;base64,QQ==", space_id: "s", document_id: "a" });
      expect(saved).toMatchObject({ url: "data:image/png;base64,QQ==", source: "browser", document_id: "a" });
      expect(upload).not.toHaveBeenCalled(); expect(mirror).not.toHaveBeenCalled();
      expect(listOfflineServerAttachments()[0]).toMatchObject({ dataUrl: "data:image/png;base64,QQ==" });
      expect(listOfflineServerAttachments()[0].replayState).toBeUndefined();
    } finally { vi.unstubAllGlobals(); }
  });

  for (const drift of ["account", "mode"] as const) {
    it(`rechecks the real attachment helper after blob conversion before ${drift} drift can retarget transport`, async () => {
      setCurrentLaunchMode("server");
      api.setToken(`e30.${btoa(JSON.stringify({ sub: "a", sid: "session-a", token_type: "access" }))}.fixture`);
      const request = vi.fn(); (globalThis as { fetch?: unknown }).fetch = request;
      const mirror = vi.spyOn(dataClient, "saveBrowserAsset");
      // The REAL upload helper awaits vaultAssetToBlob even for a data URL.
      const pending = dataClient.saveDocumentDataAsset({ name: "original.png", mime_type: "image/png", data_url: "data:image/png;base64,QQ==", space_id: "s", document_id: "a" });
      if (drift === "account") api.setToken(`e30.${btoa(JSON.stringify({ sub: "b", sid: "session-b", token_type: "access" }))}.fixture`);
      else setCurrentLaunchMode("local");
      await expect(pending).rejects.toThrow("context changed");
      expect(request).not.toHaveBeenCalled(); expect(mirror).not.toHaveBeenCalled();
      expect(listOfflineServerAttachments()).toEqual([expect.objectContaining({ dataUrl: "data:image/png;base64,QQ==", origin: expect.objectContaining({ accountId: "a", sessionId: "session-a" }), replayState: "unknown" })]);
    });
  }

  it("passes original source and holds conflict without a GET baseline or replacement", async () => {
    setCurrentLaunchMode("server"); api.setToken(draftSaveSessionToken("a"));
    const request = vi.fn(async (_url: string, init?: RequestInit) => {
      expect(JSON.parse(String(init?.body))).toEqual({ content: "A edit", source_digest: "a".repeat(64) });
      return new Response(JSON.stringify({ error: "conflict", message: "source_conflict: newer B edit" }), { status: 409 });
    }); (globalThis as { fetch?: unknown }).fetch = request;
    await expect(dataClient.updateDocumentDraft("s", "a", "d", { content: "A edit" }, "a".repeat(64))).rejects.toMatchObject({ reason: "source_conflict" });
    expect(request).toHaveBeenCalledTimes(1);
  });
  it("rejects absent source before transport and advances only from a proven update response", async () => {
    setCurrentLaunchMode("server"); api.setToken(draftSaveSessionToken("a"));
    const request = vi.fn(async () => new Response(JSON.stringify({ id: "d", document_id: "a", space_id: "s", document_title: "Draft", title: "Draft", content: "edit", owner_id: "a", owner_name: "Author", access_role: "owner", can_publish: true, created_at: "created", updated_at: "saved", base_updated_at: "base", source_digest: "b".repeat(64) }), { status: 200 }));
    (globalThis as { fetch?: unknown }).fetch = request;
    await expect(dataClient.updateDocumentDraft("s", "a", "d", { content: "edit" })).rejects.toMatchObject({ reason: "source_required" });
    expect(request).not.toHaveBeenCalled();
    await expect(dataClient.updateDocumentDraft("s", "a", "d", { content: "edit" }, "a".repeat(64))).resolves.toMatchObject({ source_digest: "b".repeat(64) });
    expect(request).toHaveBeenCalledTimes(1);
  });


  it("retains queued original digest/body and holds rather than claiming a new confirmed source", async () => {
    setCurrentLaunchMode("server"); api.setToken(draftSaveSessionToken("a"));
    vi.stubGlobal("navigator", { onLine: false });
    const request = vi.fn(); (globalThis as { fetch?: unknown }).fetch = request;
    const { listOfflineServerMutations } = await import("./offlineServerCache");
    await expect(dataClient.updateDocumentDraft("s", "a", "d", { content: "Original unsent bytes" }, "a".repeat(64))).rejects.toMatchObject({ reason: "queued" });
    expect(request).not.toHaveBeenCalled();
    expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ body: { content: "Original unsent bytes", source_digest: "a".repeat(64) }, path: "/spaces/s/documents/a/drafts/d" })]);
    vi.unstubAllGlobals();
  });

  it("keeps later B recovery bytes while known-unsent A holds the actual save coordinator", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("navigator", { onLine: false });
    try {
      setCurrentLaunchMode("server"); api.setToken(draftSaveSessionToken("a"));
      const request = vi.fn(); (globalThis as { fetch?: unknown }).fetch = request;
      const { createAutoSaveController } = await import("../hooks/useAutoSave");
      const { persistDocumentDraftRecovery, readDocumentDraftRecovery } = await import("./documentDraftRecovery");
      const { listOfflineServerMutations } = await import("./offlineServerCache");
      const scope = { storageMode: "server" as const, authIdentity: "a", serverTarget: "http://localhost:3000", spaceId: "s", documentId: "a", draftId: "d" };
      const body = (content: string) => ({ title: "Original title", content, synopsis: "", pageWidth: "standard" as const, customCss: "" });
      const save = vi.fn(async (content: string) => { await dataClient.updateDocumentDraft("s", "a", "d", { content }, "a".repeat(64)); });
      const controller = createAutoSaveController(save, () => undefined, 100, () => false);
      expect(persistDocumentDraftRecovery(scope, body("A queued edit"), "old-stamp")).toBeTruthy();
      expect(await controller.saveNow("A queued edit")).toBe(false);
      expect(persistDocumentDraftRecovery(scope, body("B newest retained edit"), "old-stamp")).toBeTruthy();
      controller.scheduleAutoSave("B newest retained edit");
      await vi.advanceTimersByTimeAsync(1000);
      expect(await controller.saveNow("B newest retained edit")).toBe(false);
      expect(save).toHaveBeenCalledTimes(1); expect(request).not.toHaveBeenCalled();
      expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ body: { content: "A queued edit", source_digest: "a".repeat(64) } })]);
      controller.dispose();
      // A newly constructed reader uses the same durable storage; no source GET or queued B is invented.
      expect(readDocumentDraftRecovery({ ...scope })).toMatchObject({ content: "B newest retained edit", baseDraftUpdatedAt: "old-stamp" });
    } finally { vi.useRealTimers(); vi.unstubAllGlobals(); }
  });

  it("holds loaded draft provenance across account, provider, server and mode changes while allowing same-session refresh", async () => {
    const { captureDraftSaveSelection, verifyDraftSaveSelection } = await import("./dataClient");
    setCurrentLaunchMode("server"); api.setToken(draftSaveSessionToken("a"));
    const original = captureDraftSaveSelection("a");
    api.setRefreshToken("rotated-refresh");
    api.setToken(`e30.${btoa(JSON.stringify({ sub: "a", sid: "draft-session-a", token_type: "access", exp: 9999999999, stamp: 2 }))}.fixture`);
    expect(() => verifyDraftSaveSelection(original, "a")).not.toThrow();
    api.setToken(draftSaveSessionToken("b"));
    expect(() => verifyDraftSaveSelection(original, "b")).toThrow("Draft session or storage changed");
    api.setToken(draftSaveSessionToken("a")); setSyncProvider("google-drive");
    expect(() => verifyDraftSaveSelection(original, "a")).toThrow("Draft session or storage changed");
    setSyncProvider("none");
    (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
    localStorage.setItem("server_url", "https://other-server.invalid");
    expect(() => verifyDraftSaveSelection(original, "a")).toThrow("Draft session or storage changed");
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    setCurrentLaunchMode("local");
    expect(() => verifyDraftSaveSelection(original, "a")).toThrow("Draft session or storage changed");
    expect(JSON.stringify(original)).not.toContain("fixture");
    expect(JSON.stringify(original)).not.toContain("rotated-refresh");
  });


  async function publishedReplayFixture() {
    const token = identifierSessionToken("published-owner"); api.setToken(token); localStorage.setItem("access_token", token);
    const page = await dataClient.createDocument("local-space", { title: "Published source", content: "Original server bytes" });
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    const source = { schema_version: 1 as const, digest: "a".repeat(64), space_id: "local-space", document_id: page.id,
      replay_provider: "none" as const, server_context: { server_base: "/api/v1", account_id: "published-owner", session_scope: serverCacheScopeForSession(token, api.getRefreshToken()) } };
    snapshot.documents.find((doc) => doc.id === page.id)!.published_source = source;
    snapshot.sync_state!.queued_operations = [];
    await dataClient.importLocalVaultSnapshot(snapshot);
    return { page, source };
  }

  it("captures original published source across fresh adapter reopen and explicit snapshot rehydration without rebasing B", async () => {
    const { page, source } = await publishedReplayFixture();
    await dataClient.updateDocumentContent("local-space", page.id, "A exact bytes");
    const first = (await dataClient.exportLocalVaultSnapshot()).sync_state!.queued_operations[0];
    await dataClient.updateDocumentContent("local-space", page.id, "B newer bytes");
    const saved = await createLocalVaultAdapter("none").loadSnapshot();
    expect(saved.documents.find((doc) => doc.id === page.id)?.published_source).toEqual(source);
    expect(saved.sync_state!.queued_operations[0]).toEqual(first);
    expect(saved.sync_state!.queued_operations.map((op) => op.payload)).toMatchObject([
      { content: "A exact bytes", published_source_digest: source.digest }, { content: "B newer bytes", published_source_digest: source.digest },
    ]);
    // Reset deliberately deletes stored vault data; explicitly reimport the
    // independently reopened snapshot to exercise the real hydration mapping.
    await __resetLocalDataForTests();
    await dataClient.importLocalVaultSnapshot(saved);
    const rehydrated = await dataClient.exportLocalVaultSnapshot();
    expect(rehydrated.documents.find((doc) => doc.id === page.id)?.published_source).toEqual(source);
    expect(rehydrated.sync_state!.queued_operations).toEqual(saved.sync_state!.queued_operations);
  });

  it("legacy local edits retain bytes with unavailable-source review and never invent a published source", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Legacy", content: "Before" });
    await dataClient.updateDocumentContent("local-space", page.id, "Retained legacy edit");
    const operations = (await dataClient.exportLocalVaultSnapshot()).sync_state!.queued_operations;
    const op = operations.find((item) => item.action === "update_content")!;
    expect(op.payload).toMatchObject({ content: "Retained legacy edit" });
    expect(op.payload).not.toHaveProperty("published_source_digest");
    expect(op.last_error).toContain("Original published server source is unavailable");
  });

  it("actual conflict resolution carries the original source instead of the post-resolution body/time", async () => {
    const { page, source } = await publishedReplayFixture();
    const conflict = await dataClient.simulateSyncConflictForDocument(page.id);
    await dataClient.resolveConflict(conflict!.id, "keep-remote");
    const op = (await dataClient.exportLocalVaultSnapshot()).sync_state!.queued_operations.find((item) => item.action === "resolve_conflict:keep-remote")!;
    expect(op.payload).toMatchObject({ published_source_digest: source.digest, content: expect.stringContaining("[Remote change detected") });
    expect(op.server_context).toEqual(source.server_context);
    expect(await dataClient.getSyncState()).toBe("idle");
  });

  for (const variant of ["exact", "absent", "wrong-fingerprint", "wrong-origin", "wrong-page", "wrong-space", "wrong-source", "newer-B"] as const) {
    it(`actual replay ${variant} advances only an exact unchanged original outcome and never rewrites queue bytes`, async () => {
      const { page, source } = await publishedReplayFixture();
      await dataClient.updateDocumentContent("local-space", page.id, "A server outcome");
      const original = (await dataClient.exportLocalVaultSnapshot()).sync_state!.queued_operations[0];
      const fingerprint = await publishedReplayFingerprint(original.payload);
      let replayCalls = 0;
      vi.spyOn(api, "fetch").mockImplementation(async (path, options) => {
        if (path === "/auth/me") return { id: "published-owner" } as never;
        if (path !== "/sync/replay") throw new Error(`Unexpected transport ${path}`);
        replayCalls += 1;
        if (variant === "newer-B" && replayCalls === 1) await dataClient.updateDocumentContent("local-space", page.id, "B retained while A response waits");
        if (variant === "wrong-origin") api.setToken(identifierSessionToken("foreign-owner"));
        const replayed = variant === "newer-B" && replayCalls === 2
          ? (options?.body as { operations: (typeof original)[] }).operations[0]
          : original;
        const replayedFingerprint = replayed.id === original.id ? fingerprint : await publishedReplayFingerprint(replayed.payload);
        return { results: [{ operation_id: replayed.id, entity_type: replayed.entity_type, action: replayed.action, status: "applied", message: "Exact committed receipt",
          ...(variant === "absent" ? {} : { published_source: { published_source_digest: (variant === "newer-B" && replayCalls === 2 ? "c" : "b").repeat(64), payload_fingerprint: variant === "wrong-fingerprint" ? "c".repeat(64) : replayedFingerprint, document_id: variant === "wrong-page" ? "foreign-page" : page.id, space_id: variant === "wrong-space" ? "foreign-space" : "local-space", original_source_digest: variant === "wrong-source" ? "f".repeat(64) : source.digest } }),
        }] } as never;
      });
      if (variant === "wrong-origin") await expect(dataClient.runQueuedSyncReplay([original.id])).rejects.toThrow("Replay context changed");
      else await dataClient.runQueuedSyncReplay([original.id]);
      const after = await createLocalVaultAdapter("none").loadSnapshot();
      if (variant === "exact" || variant === "newer-B") expect(after.sync_state!.queued_operations.some((op) => op.id === original.id)).toBe(false);
      else expect(after.sync_state!.queued_operations[0]).toEqual(original);
      const doc = after.documents.find((item) => item.id === page.id)!;
      expect(doc.published_source?.digest).toBe(variant === "exact" ? "b".repeat(64) : source.digest);
      expect(doc.content).toBe(variant === "newer-B" ? "B retained while A response waits" : "A server outcome");
      if (variant === "newer-B") {
        const retained = after.sync_state!.queued_operations[0];
        expect(retained.payload).toMatchObject({ content: doc.content, published_source_digest: source.digest });
        await dataClient.runQueuedSyncReplay([retained.id]);
        const converged = await createLocalVaultAdapter("none").loadSnapshot();
        expect(converged.sync_state!.queued_operations).toEqual([]);
        expect(converged.documents.find((item) => item.id === page.id)).toMatchObject({ content: "B retained while A response waits", published_source: { digest: "c".repeat(64) } });
        expect(replayCalls).toBe(2);
      }
    });
  }

  it("payload fingerprint matches Rust scalar key ordering for nested astral/BMP keys and copied wire normalization", async () => {
    const value = { outer: { [String.fromCodePoint(0x10000)]: "astral", [String.fromCodePoint(0xe000)]: "bmp" }, zero: -0, omitted: undefined };
    expect(await publishedReplayFingerprint(value)).toBe("120ca7e55a422297f6a51d6d198455ee1b44a702fbdacbc6423cd380a0146b82");
  });


  for (const kind of ["exact", "malformed", "context-drift"] as const) {
    it(`actual server export ${kind} enrolls only its original content response/context, never list timestamps`, async () => {
      setCurrentLaunchMode("server");
      const token = identifierSessionToken("export-owner"); api.setToken(token); localStorage.setItem("access_token", token);
      vi.spyOn(dataClient, "listSpaces").mockResolvedValue([{ id: "export-space", name: "Export", slug: "export-space" }]);
      vi.spyOn(dataClient, "listDocuments").mockResolvedValue([{ id: "export-page", title: "Stale list title", updated_at: "stale-list-stamp", published_at: "old-published-stamp" }]);
      vi.spyOn(dataClient, "listFolders").mockResolvedValue([]); vi.spyOn(dataClient, "listTemplates").mockResolvedValue([]);
      vi.spyOn(dataClient, "listWorkflowTemplates").mockResolvedValue([]); vi.spyOn(dataClient, "listPropertySchemas").mockResolvedValue([]); vi.spyOn(dataClient, "listTags").mockResolvedValue([]);
      vi.spyOn(dataClient, "getDocumentWorkflow").mockResolvedValue(null); vi.spyOn(dataClient, "listDocumentWorkflowEvents").mockResolvedValue([]);
      vi.spyOn(dataClient, "listDocumentProperties").mockResolvedValue([]); vi.spyOn(dataClient, "listDocumentTags").mockResolvedValue([]);
      vi.spyOn(api, "fetch").mockImplementation(async (path) => {
        if (path === "/auth/me") return { id: "export-owner" } as never;
        if (path !== "/spaces/export-space/documents/export-page/content") throw new Error(`Unexpected ${path}`);
        if (kind === "context-drift") setSyncProvider("google-drive");
        return { id: "export-page", title: "Actual read title", content: "Exact served original body", published_source_digest: kind === "malformed" ? "bad" : "d".repeat(64) } as never;
      });
      if (kind === "exact") {
        const snapshot = await dataClient.exportServerVaultSnapshot();
        expect(snapshot.documents[0]).toMatchObject({ title: "Actual read title", content: "Exact served original body", published_source: { digest: "d".repeat(64), document_id: "export-page", space_id: "export-space", replay_provider: "none", server_context: { account_id: "export-owner", server_base: "/api/v1" } } });
        expect(JSON.stringify(snapshot)).not.toContain(token);
        // Consume the actual exported snapshot after an explicit provider choice.
        setCurrentLaunchMode("local"); setSyncProvider("google-drive");
        await dataClient.importLocalVaultSnapshot(snapshot);
        await dataClient.updateDocumentContent("export-space", "export-page", "Edited after cloud import");
        const imported = await createLocalVaultAdapter("google-drive").loadSnapshot();
        const op = imported.sync_state!.queued_operations[0];
        expect(imported.documents[0].published_source).toEqual(snapshot.documents[0].published_source);
        expect(op.replay_provider).toBe("google-drive");
        expect(op.payload).toMatchObject({ content: "Edited after cloud import", published_source_digest: "d".repeat(64) });
        const payloadFingerprint = await publishedReplayFingerprint(op.payload);
        vi.mocked(api.fetch).mockImplementation(async (path) => {
          if (path === "/auth/me") return { id: "export-owner" } as never;
          if (path !== "/sync/replay") throw new Error(`Unexpected ${path}`);
          return { results: [{ operation_id: op.id, entity_type: op.entity_type, action: op.action, status: "applied", message: "Receipt", published_source: { published_source_digest: "e".repeat(64), payload_fingerprint: payloadFingerprint, document_id: "export-page", space_id: "export-space", original_source_digest: "d".repeat(64) } }] } as never;
        });
        await dataClient.runQueuedSyncReplay([op.id]);
        const confirmed = await createLocalVaultAdapter("google-drive").loadSnapshot();
        expect(confirmed.documents[0].published_source?.digest).toBe("e".repeat(64));
        expect(confirmed.sync_state!.queued_operations).toEqual([]);
      } else await expect(dataClient.exportServerVaultSnapshot()).rejects.toThrow(kind === "malformed" ? "source response is unproven" : "account changed");
    });
  }

  for (const account of ["published-owner", "foreign-owner"] as const) {
    it(`new edit enrolls current ${account} session only when its subject matches the original read`, async () => {
      const { page, source } = await publishedReplayFixture();
      const token = `e30.${btoa(JSON.stringify({ sub: account, sid: "new-login-session" }))}.fixture`;
      // Actual login/logout invalidation rotates this durable cache partition.
      clearServerCacheSessionScope();
      api.setToken(token); localStorage.setItem("access_token", token);
      await dataClient.updateDocumentContent("local-space", page.id, "New login retained edit");
      const saved = await createLocalVaultAdapter("none").loadSnapshot();
      expect(saved.documents.find((doc) => doc.id === page.id)?.published_source).toEqual(source);
      const op = saved.sync_state!.queued_operations[0];
      if (account === "published-owner") {
        expect(op.server_context?.session_scope).toBe(serverCacheScopeForSession(token, api.getRefreshToken()));
        expect(op.server_context?.session_scope).not.toBe(source.server_context.session_scope);
        expect(op.payload).toMatchObject({ published_source_digest: source.digest });
        const fingerprint = await publishedReplayFingerprint(op.payload);
        vi.spyOn(api, "fetch").mockImplementation(async (path) => {
          if (path === "/auth/me") return { id: account } as never;
          if (path !== "/sync/replay") throw new Error(`Unexpected ${path}`);
          return { results: [{ operation_id: op.id, entity_type: op.entity_type, action: op.action, status: "applied", message: "Receipt", published_source: { published_source_digest: "c".repeat(64), payload_fingerprint: fingerprint, document_id: page.id, space_id: "local-space", original_source_digest: source.digest } }] } as never;
        });
        await dataClient.runQueuedSyncReplay([op.id]);
        expect((await createLocalVaultAdapter("none").loadSnapshot()).documents.find((doc) => doc.id === page.id)?.published_source?.digest).toBe("c".repeat(64));
      } else {
        expect(op).not.toHaveProperty("server_context");
        expect(op.payload).not.toHaveProperty("published_source_digest");
        expect(op.last_error).toContain("explicit review");
      }
    });
  }

  for (const variant of ["exact", "edited-same-id", "tail-provider-drift"] as const) {
    it(`failed replay ${variant} updates only its exact original queue under the captured context`, async () => {
      const { page } = await publishedReplayFixture();
      await dataClient.updateDocumentContent("local-space", page.id, "Original request bytes");
      const adapter = createLocalVaultAdapter("none");
      const original = (await adapter.loadSnapshot()).sync_state!.queued_operations[0];
      let edited = original;
      vi.spyOn(api, "fetch").mockImplementation(async (path) => {
        if (path === "/auth/me") return { id: "published-owner" } as never;
        if (path !== "/sync/replay") throw new Error(`Unexpected ${path}`);
        if (variant === "edited-same-id") {
          await adapter.commitSnapshotMutation((snapshot) => {
            const current = snapshot.sync_state!.queued_operations[0];
            current.payload = { changed: "Newer exact bytes" }; current.last_error = "Newer local evidence";
            edited = JSON.parse(JSON.stringify(current));
            return { snapshot };
          });
        }
        if (variant === "tail-provider-drift") {
          const prototype = Object.getPrototypeOf(adapter) as { supportsAtomicSnapshotCommit(): Promise<boolean> };
          vi.spyOn(prototype, "supportsAtomicSnapshotCommit").mockImplementation(async () => { setSyncProvider("google-drive"); return true; });
        }
        return { results: [{ operation_id: original.id, entity_type: original.entity_type, action: original.action, status: "failed", message: "Exact server failure" }] } as never;
      });
      if (variant === "tail-provider-drift") await expect(dataClient.runQueuedSyncReplay([original.id])).rejects.toThrow("Replay context changed");
      else await dataClient.runQueuedSyncReplay([original.id]);
      // The dispatched consumer owns another adapter; a cached original
      // adapter cannot stand in for actual disk reread after its commit.
      const saved = await createLocalVaultAdapter("none").loadSnapshot();
      expect(saved.documents.find((doc) => doc.id === page.id)?.content).toBe("Original request bytes");
      expect(saved.sync_state!.queued_operations[0]).toEqual(variant === "exact" ? { ...original, attempts: (original.attempts ?? 0) + 1, last_error: "Exact server failure" } : edited);
      if (variant === "tail-provider-drift") expect((await createLocalVaultAdapter("google-drive").loadSnapshot()).sync_state?.queued_operations ?? []).toEqual([]);
    });
  }

  it("payload fingerprint emits integer-like nested keys in Rust lexicographic order", async () => {
    expect(await publishedReplayFingerprint({ outer: { "2": "two", "10": "ten", "1": "one" } })).toBe("bfd6dd857828af5a479e1449fdf1c565bf985c7e89e3c5aa7c29b1c862e28fe5");
  });

  it("provider selection alone cannot copy old cached published pages into another provider", async () => {
    const { page } = await publishedReplayFixture();
    const before = await createLocalVaultAdapter("none").loadSnapshot();
    setSyncProvider("google-drive");
    await expect(dataClient.updateDocumentContent("local-space", page.id, "Must not copy old cache")).rejects.toThrow("Import the selected provider");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(before.documents);
    expect((await createLocalVaultAdapter("google-drive").loadSnapshot()).documents).toEqual([]);
  });

  it("selected-provider import rejects drift after capability await before canonical commit", async () => {
    await publishedReplayFixture();
    const original = await dataClient.exportLocalVaultSnapshot();
    setSyncProvider("google-drive");
    const prototype = Object.getPrototypeOf(createLocalVaultAdapter("google-drive")) as { supportsAtomicSnapshotCommit(): Promise<boolean> };
    vi.spyOn(prototype, "supportsAtomicSnapshotCommit").mockImplementation(async () => { setSyncProvider("icloud-drive"); return true; });
    await expect(dataClient.importLocalVaultSnapshot(original)).rejects.toThrow("context changed");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(original.documents);
    expect((await createLocalVaultAdapter("google-drive").loadSnapshot()).documents).toEqual([]);
    expect((await createLocalVaultAdapter("icloud-drive").loadSnapshot()).documents).toEqual([]);
  });

  it.each(["canonical", "legacy"])("server metadata-only snapshot refuses incomplete %s exact attachment bytes", async (kind) => {
    setCurrentLaunchMode("server");const token=identifierSessionToken("export-owner");api.setToken(token);localStorage.setItem("access_token",token);
    const pinned="11111111-1111-4111-8111-111111111111";
    vi.spyOn(dataClient,"listSpaces").mockResolvedValue([{id:"third-space",name:"Third",slug:"third"}]);
    vi.spyOn(dataClient,"listDocuments").mockResolvedValue([{id:"moved-page",title:"Pinned retired original"}]);
    for(const key of ["listFolders","listTemplates","listWorkflowTemplates","listPropertySchemas","listTags","listDocumentWorkflowEvents","listDocumentProperties","listDocumentTags"] as const) vi.spyOn(dataClient,key).mockResolvedValue([]);
    vi.spyOn(dataClient,"getDocumentWorkflow").mockResolvedValue(null);
    vi.spyOn(api,"fetch").mockImplementation(async path=>{
      if(path==="/auth/me")return {id:"export-owner"} as never;
      if(path==="/spaces/third-space/documents/moved-page/content")return {id:"moved-page",title:"Pinned retired original",content:kind === "canonical" ? `[file:Same name](starlane-attachment://${pinned})` : `[file:Same name](/api/v1/files/spaces/original/attachments/original-page/${pinned}.bin)`,published_source_digest:"d".repeat(64)} as never;
      throw new Error(`Unexpected ${path}`);
    });
    await expect(dataClient.exportServerVaultSnapshot()).rejects.toThrow("Server export requires bounded exact attachment bytes");
  });

});
