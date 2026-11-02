import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createLocalVaultAdapter,
  assertImportedQueuedSyncOperations,
  importNativeVaultAssetFromDataUrl,
  repairLocalVaultSnapshotStructure,
  runLocalVaultStartupCheck,
  type LocalVaultSnapshot,
} from "./localVault";
import {
  NATIVE_LOCAL_DATABASE_COMMANDS,
  type NativeLocalDatabaseRecordCollection,
} from "./nativeLocalDatabase";

function emptyNativeSnapshot(): LocalVaultSnapshot {
  return {
    schema_version: 1,
    spaces: [],
    folders: [],
    documents: [],
    assets: [],
    templates: [],
    property_schemas: [],
    property_values: [],
    tags: [],
    doc_tags: [],
    search_index: [],
    sync_state: {
      state: "idle",
      updated_at: "2026-07-08T00:00:00.000Z",
      queued_operations: [],
    },
  };
}

function testRecordId(collection: NativeLocalDatabaseRecordCollection, value: unknown): string {
  const record = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  if (collection === "doc_tags") {
    return `${String(record.document_id ?? "document")}:${String(record.tag_id ?? "tag")}`;
  }
  if (collection === "search_index") return String(record.document_id ?? "");
  if (collection === "document_workflows") return String(record.id ?? record.document_id ?? "");
  if (collection === "property_values") {
    return String(
      record.id ??
        `${String(record.document_id ?? "document")}:${String(record.property_id ?? "property")}`,
    );
  }
  return String(record.id ?? "");
}

function upsertNativeSnapshotRecord(
  snapshot: LocalVaultSnapshot,
  collection: NativeLocalDatabaseRecordCollection,
  id: string,
  payload: unknown,
): void {
  if (collection === "sync_state") {
    snapshot.sync_state = payload as LocalVaultSnapshot["sync_state"];
    return;
  }
  if (collection === "queued_operations") {
    const operations = snapshot.sync_state?.queued_operations ?? [];
    snapshot.sync_state = {
      state: snapshot.sync_state?.state ?? "idle",
      updated_at: snapshot.sync_state?.updated_at ?? "2026-07-08T00:00:00.000Z",
      queued_operations: [
        ...operations.filter((operation) => operation.id !== id),
        payload as NonNullable<LocalVaultSnapshot["sync_state"]>["queued_operations"][number],
      ],
    };
    return;
  }
  const snapshotRecords = snapshot as unknown as Record<string, unknown[]>;
  const records = snapshotRecords[collection] ?? [];
  snapshotRecords[collection] = [
    ...records.filter((record) => testRecordId(collection, record) !== id),
    payload,
  ];
}

function deleteNativeSnapshotRecord(
  snapshot: LocalVaultSnapshot,
  collection: NativeLocalDatabaseRecordCollection,
  id: string,
): void {
  if (collection === "queued_operations") {
    const operations = snapshot.sync_state?.queued_operations ?? [];
    snapshot.sync_state = {
      state: snapshot.sync_state?.state ?? "idle",
      updated_at: snapshot.sync_state?.updated_at ?? "2026-07-08T00:00:00.000Z",
      queued_operations: operations.filter((operation) => operation.id !== id),
    };
    return;
  }
  if (collection === "sync_state") {
    snapshot.sync_state = {
      state: "idle",
      updated_at: "2026-07-08T00:00:00.000Z",
      queued_operations: [],
    };
    return;
  }
  const snapshotRecords = snapshot as unknown as Record<string, unknown[]>;
  const records = snapshotRecords[collection] ?? [];
  snapshotRecords[collection] = records.filter((record) => testRecordId(collection, record) !== id);
}

describe("localVault adapter", () => {
  const storage = new Map<string, string>();
  const nativeSnapshots = new Map<string, LocalVaultSnapshot>();
  let enableNativeLocalDatabase = false;
  let failNativeRecordCommands = false;
  let enableAtomicNative = false;
  let failNativeCommit = false;
  let loseNativeAcknowledgement = false;
  let reorderLostAcknowledgement = false;
  let failMirror = false;
  let failDescribe = false;
  let failAssetCleanup = false;
  let raceBootstrap = false;
  const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
    const provider = String(args?.provider ?? "none");
    switch (cmd) {
      case NATIVE_LOCAL_DATABASE_COMMANDS.describe:
        if (failDescribe) throw new Error("IPC disconnected");
        if (!enableNativeLocalDatabase)
          throw new Error("Unknown command describe_native_local_database");
        return {
          schema_version: 1,
          provider,
          store_kind: "sqlite",
          database_path: "/tmp/starlane-vault.sqlite3",
          supports_snapshot_bridge: true,
          supports_atomic_snapshot_commit: enableAtomicNative,
          supports_reviewed_draft_transfer_evidence: true,
          supports_operation_queue: true,
          replay_backends: ["sqlite", "postgresql"],
        };
      case NATIVE_LOCAL_DATABASE_COMMANDS.readSnapshot:
        return nativeSnapshots.get(`native:${provider}`) ?? null;
      case NATIVE_LOCAL_DATABASE_COMMANDS.commitSnapshot: {
        if (raceBootstrap) {
          raceBootstrap = false;
          const newer = emptyNativeSnapshot();
          newer.canonical_revision = "external-revision";
          newer.property_values = [
            { id: "external", document_id: "doc", property_id: "field", value: "External" },
          ];
          newer.sync_state!.queued_operations = [
            {
              id: "external-queue",
              entity_type: "document_property_values",
              entity_id: "doc",
              action: "set",
              queued_at: "now",
              payload: {},
            },
          ];
          nativeSnapshots.set(`native:${provider}`, newer);
        }
        if (failNativeCommit) throw new Error("Native transaction failed");
        const previous = nativeSnapshots.get(`native:${provider}`);
        if ((args?.expectedRevision ?? null) !== (previous?.canonical_revision ?? null))
          throw new Error("Native snapshot changed; reload");
        const snapshot = JSON.parse(JSON.stringify(args?.snapshot)) as LocalVaultSnapshot;
        snapshot.canonical_revision = `revision-${(snapshot.sync_state?.queued_operations ?? []).length}`;
        snapshot.mirror_pending = failMirror;
        if (reorderLostAcknowledgement) snapshot.sync_state?.queued_operations.reverse();
        nativeSnapshots.set(`native:${provider}`, snapshot);
        if (loseNativeAcknowledgement) throw new Error("IPC response lost");
        return {
          canonical_committed: true,
          canonical_revision: snapshot.canonical_revision,
          mirror_pending: failMirror,
          mirror_error: failMirror ? "Mirror failed" : null,
        };
      }
      case NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot:
        nativeSnapshots.set(`native:${provider}`, args?.snapshot as LocalVaultSnapshot);
        return undefined;
      case NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord:
        if (!enableNativeLocalDatabase) throw new Error("native local database unavailable");
        if (failNativeRecordCommands) throw new Error("record commands unavailable");
        {
          const snapshot = nativeSnapshots.get(`native:${provider}`) ?? emptyNativeSnapshot();
          upsertNativeSnapshotRecord(
            snapshot,
            args?.collection as NativeLocalDatabaseRecordCollection,
            String(args?.id ?? ""),
            args?.payload,
          );
          nativeSnapshots.set(`native:${provider}`, snapshot);
        }
        return undefined;
      case NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord:
        if (!enableNativeLocalDatabase) throw new Error("native local database unavailable");
        if (failNativeRecordCommands) throw new Error("record commands unavailable");
        {
          const snapshot = nativeSnapshots.get(`native:${provider}`) ?? emptyNativeSnapshot();
          deleteNativeSnapshotRecord(
            snapshot,
            args?.collection as NativeLocalDatabaseRecordCollection,
            String(args?.id ?? ""),
          );
          nativeSnapshots.set(`native:${provider}`, snapshot);
        }
        return undefined;
      case NATIVE_LOCAL_DATABASE_COMMANDS.syncMirror:
        if (!enableNativeLocalDatabase) throw new Error("native local database unavailable");
        if (failMirror) throw new Error("Mirror failed");
        {
          const current = nativeSnapshots.get(`native:${provider}`);
          if (current) current.mirror_pending = false;
        }
        return undefined;
      case NATIVE_LOCAL_DATABASE_COMMANDS.reset:
        nativeSnapshots.delete(`native:${provider}`);
        return undefined;
      case "read_vault_snapshot":
        return nativeSnapshots.get(provider) ?? null;
      case "write_vault_snapshot":
        nativeSnapshots.set(provider, args?.snapshot as LocalVaultSnapshot);
        return undefined;
      case "reset_vault_snapshot":
        nativeSnapshots.delete(provider);
        return undefined;
      case "import_vault_asset_from_data_url":
        return {
          id: "asset-native",
          name: String(args?.name ?? "asset"),
          mime_type: String(args?.mimeType ?? "application/octet-stream"),
          size: 4,
          url: "asset://localhost/asset-native.png",
          vault_path: "/tmp/vault/attachments/asset-native.png",
          source: "tauri",
          space_id: args?.spaceId,
          document_id: args?.documentId,
          created_at: "2026-07-28T00:00:00.000Z",
        };
      case "delete_vault_asset":
        if (failAssetCleanup) throw new Error("File cleanup failed");
        return undefined;
      default:
        throw new Error(`Unexpected command: ${cmd}`);
    }
  });

  beforeEach(() => {
    storage.clear();
    nativeSnapshots.clear();
    enableNativeLocalDatabase = false;
    failNativeRecordCommands = false;
    enableAtomicNative = false;
    failNativeCommit = false;
    loseNativeAcknowledgement = false;
    reorderLostAcknowledgement = false;
    failMirror = false;
    failDescribe = false;
    failAssetCleanup = false;
    raceBootstrap = false;
    invoke.mockClear();
    (globalThis as { window?: unknown }).window = {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => {
          storage.set(key, value);
        },
        removeItem: (key: string) => {
          storage.delete(key);
        },
      },
    };
  });

  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
  });

  it("preserves complete replay operations and sync-state extras through both adapters", async () => {
    const snapshot = emptyNativeSnapshot();
    const operation = {
      id: "raw-operation", entity_type: "document", entity_id: "page-a", action: "update_content",
      queued_at: "2026-10-09T10:00:00Z", replay_provider: "google-drive" as const,
      server_context: { server_base: "https://old.example.test/api/v1", account_id: "original-account", session_scope: "original-session", future_binding: "retained" },
      payload: { content: "Original\r\nbody", source: { published_at: null, revision: "old" } },
      future_operation: { raw: [null, "unchanged"] },
    };
    snapshot.sync_state = { ...snapshot.sync_state!, future_state: { checkpoint: "retained" }, queued_operations: [operation] } as typeof snapshot.sync_state;
    assertImportedQueuedSyncOperations(snapshot);
    for (const native of [false, true]) {
      if (native) {
        enableNativeLocalDatabase = true;
        enableAtomicNative = true;
        (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = { invoke };
      }
      const vault = createLocalVaultAdapter(native ? "google-drive" : "none");
      await vault.saveSnapshot(snapshot);
      const reloaded = await createLocalVaultAdapter(native ? "google-drive" : "none").loadSnapshot();
      expect(reloaded.sync_state?.queued_operations).toEqual([operation]);
      expect(reloaded.sync_state).toMatchObject({ future_state: { checkpoint: "retained" } });
      expect(reloaded.sync_state?.updated_at).toEqual(expect.any(String));
      expect(Object.prototype.hasOwnProperty.call(reloaded.sync_state!.queued_operations[0], "attempts")).toBe(false);
    }
  });

  it("does not reconcile a lost ACK against the same operations in a different stored order", async () => {
    enableNativeLocalDatabase = true;
    enableAtomicNative = true;
    (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = { invoke };
    const vault = createLocalVaultAdapter("google-drive");
    const snapshot = emptyNativeSnapshot();
    snapshot.sync_state!.queued_operations = [
      { id: "z-first", entity_type: "document", entity_id: "page", action: "update_content", queued_at: "same", payload: { content: "First" } },
      { id: "a-second", entity_type: "document", entity_id: "page", action: "update_content", queued_at: "same", payload: { content: "Second" } },
    ];
    await vault.saveSnapshot(snapshot);
    loseNativeAcknowledgement = true;
    await expect(vault.saveSnapshot(snapshot)).resolves.toBeUndefined();
    expect((await vault.loadSnapshot()).sync_state?.queued_operations).toEqual(snapshot.sync_state!.queued_operations);
    reorderLostAcknowledgement = true;
    await expect(vault.saveSnapshot(snapshot)).rejects.toThrow("outcome is unknown");
    expect(nativeSnapshots.get("native:google-drive")?.sync_state?.queued_operations.map((operation) => operation.id)).toEqual(["a-second", "z-first"]);
  });

  it("rejects malformed imported replay identity while allowing intentional builder defaults", async () => {
    const operation = { id: "op", entity_type: "document", entity_id: "page", action: "update_content", queued_at: "old", payload: null };
    for (const sync_state of [null, { state: "unknown", queued_operations: [operation] }, { state: "idle", queued_operations: {} },
      { state: "idle", queued_operations: [operation, operation] },
      ...[{ id: undefined }, { queued_at: "" }, { replay_provider: null }, { server_context: null }, { attempts: -1 }, { last_error: null }].map((change) => ({ state: "idle", queued_operations: [{ ...operation, ...change }] }))]) {
      expect(() => assertImportedQueuedSyncOperations({ sync_state })).toThrow();
    }
    expect(() => assertImportedQueuedSyncOperations({})).not.toThrow();
    expect(() => assertImportedQueuedSyncOperations({ sync_state: { state: "idle" } })).not.toThrow();
    expect(() => assertImportedQueuedSyncOperations({ sync_state: { state: "idle", queued_operations: [operation] } })).not.toThrow();
    const vault = createLocalVaultAdapter("none");
    const queued = await vault.enqueueSyncOperation({ entity_type: "document", entity_id: "page", action: "update_content", payload: null });
    expect(queued.id).toEqual(expect.any(String));
    expect(queued.queued_at).toEqual(expect.any(String));
    expect(queued.attempts).toBe(0);
  });

  it("starts empty after reset", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.reset();
    await vault.init();
    const spaces = await vault.listSpaces();
    expect(spaces.length).toBe(0);
  });

  it("imports downloaded bytes into the native vault", async () => {
    (
      globalThis.window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }
    ).__TAURI_INTERNALS__ = { invoke };

    const asset = await importNativeVaultAssetFromDataUrl({
      name: "Architecture.png",
      mime_type: "image/png",
      data_url: "data:image/png;base64,cG5n",
      space_id: "space-1",
      document_id: "doc-1",
    });

    expect(asset).toEqual(
      expect.objectContaining({
        source: "tauri",
        url: "asset://localhost/asset-native.png",
        space_id: "space-1",
        document_id: "doc-1",
      }),
    );
    expect(invoke).toHaveBeenCalledWith("import_vault_asset_from_data_url", {
      provider: "none",
      name: "Architecture.png",
      mimeType: "image/png",
      dataUrl: "data:image/png;base64,cG5n",
      spaceId: "space-1",
      documentId: "doc-1",
    });
  });

  it("persists document state across adapter instances", async () => {
    const vault = createLocalVaultAdapter("google-drive");
    await vault.reset();
    await vault.init();
    await vault.saveDocument({
      id: "doc-1",
      space_id: "local-space",
      title: "Draft",
      content: "hello",
      updated_at: new Date().toISOString(),
    });

    const vaultReloaded = createLocalVaultAdapter("google-drive");
    await vaultReloaded.init();
    const found = await vaultReloaded.getDocument("doc-1");
    expect(found?.title).toBe("Draft");
    expect(await vaultReloaded.getSyncState()).toBe("idle");
  });

  it("persists sync metadata across adapter instances", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.reset();
    await vault.init();
    await vault.setSyncState("conflict");

    const exported = await vault.loadSnapshot();
    expect(exported.sync_state?.state).toBe("conflict");

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    expect(await reloaded.getSyncState()).toBe("conflict");
    expect((await reloaded.loadSnapshot()).sync_state?.queued_operations).toEqual([]);
  });

  it("preserves queued sync operations across saves and adapter instances", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.reset();
    await vault.init();
    const queued = await vault.enqueueSyncOperation({
      entity_type: "document",
      entity_id: "doc-queued",
      action: "update_content",
    });
    await vault.saveDocument({
      id: "doc-queued",
      space_id: "local-space",
      title: "Queued",
      content: "offline edit",
      updated_at: "2026-07-06T00:00:00.000Z",
    });

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    expect((await reloaded.listQueuedSyncOperations()).map((operation) => operation.id)).toEqual([
      queued.id,
    ]);

    await reloaded.clearQueuedSyncOperations([queued.id]);
    expect(await reloaded.listQueuedSyncOperations()).toEqual([]);
  });

  it("persists asset metadata in the local vault snapshot", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.reset();
    await vault.init();
    await vault.saveAsset({
      id: "asset-1",
      name: "diagram.png",
      mime_type: "image/png",
      size: 1200,
      url: "data:image/png;base64,abc",
      source: "browser",
      created_at: "2026-06-25T00:00:00.000Z",
    });

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    const assets = await reloaded.listAssets();
    expect(assets).toHaveLength(1);
    expect(assets[0].name).toBe("diagram.png");
  });

  it("deletes asset metadata from the local vault snapshot", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.reset();
    await vault.init();
    await vault.saveAsset({
      id: "asset-delete",
      name: "delete-me.txt",
      mime_type: "text/plain",
      size: 12,
      url: "data:text/plain;base64,ZGVsZXRlLW1l",
      source: "browser",
      created_at: "2026-06-25T00:00:00.000Z",
    });

    await vault.deleteAsset("asset-delete");

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    expect(await reloaded.listAssets()).toEqual([]);
  });

  it("persists template metadata in the local vault snapshot", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.reset();
    await vault.init();
    await vault.saveTemplate({
      id: "template-1",
      space_id: "local-space",
      name: "Daily Note",
      content: "# {{date}}",
      tags: ["daily"],
      created_at: "2026-07-01T00:00:00.000Z",
      updated_at: "2026-07-01T00:00:00.000Z",
    });

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    const templates = await reloaded.listTemplates();
    expect(templates).toHaveLength(1);
    expect(templates[0].name).toBe("Daily Note");
  });

  it("persists workflow template metadata in the local vault snapshot", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.reset();
    await vault.init();
    await vault.saveWorkflowTemplate({
      id: "workflow-1",
      space_id: "local-space",
      name: "Draft Review",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [{ id: "approver-1", approver_type: "role", approver_id: "owner" }],
      created_at: "2026-07-05T00:00:00.000Z",
      updated_at: "2026-07-05T00:00:00.000Z",
    });

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    const workflows = await reloaded.listWorkflowTemplates();
    expect(workflows).toHaveLength(1);
    expect(workflows[0].stages.map((stage) => stage.name)).toEqual(["Draft", "Review"]);
  });

  it("persists document workflow state in the local vault snapshot", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.reset();
    await vault.init();
    await vault.saveDocumentWorkflow({
      id: "document-workflow-1",
      document_id: "doc-1",
      workflow_template_id: "workflow-1",
      current_stage_id: "draft",
      status: "active",
      created_by: "local-user",
      created_at: "2026-07-05T00:00:00.000Z",
      updated_at: "2026-07-05T00:00:00.000Z",
    });

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    const workflow = await reloaded.getDocumentWorkflow("doc-1");
    expect(workflow?.current_stage_id).toBe("draft");

    await reloaded.deleteDocumentWorkflow("doc-1");
    expect(await reloaded.getDocumentWorkflow("doc-1")).toBeNull();
  });

  it("uses native Tauri vault storage when available", async () => {
    (
      globalThis.window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }
    ).__TAURI_INTERNALS__ = { invoke };

    const vault = createLocalVaultAdapter("none");
    await vault.init();
    await vault.saveDocument({
      id: "doc-native",
      space_id: "local-space",
      title: "Native Draft",
      content: "stored on disk",
      updated_at: new Date().toISOString(),
    });

    expect(invoke).toHaveBeenCalledWith("write_vault_snapshot", expect.any(Object));
    expect(nativeSnapshots.get("none")?.documents[0]?.title).toBe("Native Draft");

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    const found = await reloaded.getDocument("doc-native");
    expect(found?.content).toBe("stored on disk");
  });

  it("prefers the native local database snapshot bridge when exposed by Tauri", async () => {
    enableNativeLocalDatabase = true;
    (
      globalThis.window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }
    ).__TAURI_INTERNALS__ = { invoke };

    const vault = createLocalVaultAdapter("none");
    await vault.init();
    await vault.saveDocument({
      id: "doc-native-db",
      space_id: "local-space",
      title: "SQLite Draft",
      content: "local first",
      updated_at: "2026-07-08T12:00:00.000Z",
    });

    expect(invoke).not.toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot,
      expect.any(Object),
    );
    expect(invoke).toHaveBeenCalledWith(NATIVE_LOCAL_DATABASE_COMMANDS.syncMirror, {
      provider: "none",
    });
    expect(invoke).toHaveBeenCalledWith(NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord, {
      provider: "none",
      collection: "documents",
      id: "doc-native-db",
      payload: expect.objectContaining({ id: "doc-native-db", title: "SQLite Draft" }),
      rowOrder: undefined,
    });
    expect(invoke).not.toHaveBeenCalledWith("write_vault_snapshot", expect.any(Object));

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    expect((await reloaded.getDocument("doc-native-db"))?.title).toBe("SQLite Draft");
  });

  it("falls back to native snapshot writes when record commands are unavailable", async () => {
    enableNativeLocalDatabase = true;
    failNativeRecordCommands = true;
    (
      globalThis.window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }
    ).__TAURI_INTERNALS__ = { invoke };

    const vault = createLocalVaultAdapter("none");
    await vault.init();
    await vault.saveDocument({
      id: "doc-native-fallback",
      space_id: "local-space",
      title: "Fallback Draft",
      content: "snapshot bridge",
      updated_at: "2026-07-08T12:00:00.000Z",
    });

    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot,
      expect.objectContaining({
        provider: "none",
        snapshot: expect.objectContaining({
          documents: [expect.objectContaining({ id: "doc-native-fallback" })],
        }),
      }),
    );
  });

  it("routes native local collection writes through record commands before mirroring", async () => {
    enableNativeLocalDatabase = true;
    (
      globalThis.window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }
    ).__TAURI_INTERNALS__ = { invoke };

    const vault = createLocalVaultAdapter("none");
    await vault.init();
    await vault.saveAsset({
      id: "asset-native",
      name: "diagram.png",
      mime_type: "image/png",
      size: 1200,
      url: "asset://localhost/asset-native.png",
      vault_path: "/tmp/vault/attachments/asset-native.png",
      source: "tauri",
      created_at: "2026-06-25T00:00:00.000Z",
    });
    await vault.saveTemplate({
      id: "template-native",
      space_id: "local-space",
      name: "Daily Note",
      content: "# {{date}}",
      tags: [],
      created_at: "2026-07-01T00:00:00.000Z",
      updated_at: "2026-07-01T00:00:00.000Z",
    });
    await vault.saveWorkflowTemplate({
      id: "workflow-native",
      space_id: "local-space",
      name: "Draft Review",
      stages: [],
      approvers: [],
      created_at: "2026-07-05T00:00:00.000Z",
      updated_at: "2026-07-05T00:00:00.000Z",
    });
    await vault.saveDocumentWorkflow({
      id: "document-workflow-native",
      document_id: "doc-native",
      workflow_template_id: "workflow-native",
      current_stage_id: "draft",
      status: "active",
      created_by: "local-user",
      created_at: "2026-07-05T00:00:00.000Z",
      updated_at: "2026-07-05T00:00:00.000Z",
    });
    await vault.deleteAsset("asset-native");
    await vault.setSyncState("conflict");

    expect(invoke).toHaveBeenCalledWith("delete_vault_asset", {
      vaultPath: "/tmp/vault/attachments/asset-native.png",
    });

    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "assets", id: "asset-native" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "templates", id: "template-native" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "workflow_templates", id: "workflow-native" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({
        collection: "document_workflows",
        id: "document-workflow-native",
      }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "sync_state", id: "none" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord,
      expect.objectContaining({ collection: "assets", id: "asset-native" }),
    );
    expect(invoke).toHaveBeenCalledWith(NATIVE_LOCAL_DATABASE_COMMANDS.syncMirror, {
      provider: "none",
    });
    expect(nativeSnapshots.get("native:none")?.assets).toEqual([]);
    expect(nativeSnapshots.get("native:none")?.sync_state?.state).toBe("conflict");
  });

  it("clears all native queued operations without a full snapshot bridge write", async () => {
    enableNativeLocalDatabase = true;
    (
      globalThis.window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }
    ).__TAURI_INTERNALS__ = { invoke };

    const vault = createLocalVaultAdapter("none");
    await vault.init();
    const first = await vault.enqueueSyncOperation({
      entity_type: "document",
      entity_id: "doc-one",
      action: "update_content",
      queued_at: "2026-07-08T12:00:00.000Z",
    });
    const second = await vault.enqueueSyncOperation({
      entity_type: "document",
      entity_id: "doc-two",
      action: "update_content",
      queued_at: "2026-07-08T12:01:00.000Z",
    });

    invoke.mockClear();
    await vault.clearQueuedSyncOperations();

    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord,
      expect.objectContaining({ collection: "queued_operations", id: first.id }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord,
      expect.objectContaining({ collection: "queued_operations", id: second.id }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({
        collection: "sync_state",
        payload: expect.objectContaining({ queued_operations: [] }),
      }),
    );
    expect(invoke).not.toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot,
      expect.any(Object),
    );
    expect(nativeSnapshots.get("native:none")?.sync_state?.queued_operations).toEqual([]);

    const reloaded = createLocalVaultAdapter("none");
    await reloaded.init();
    expect(await reloaded.listQueuedSyncOperations()).toEqual([]);
  });

  it("diffs full native snapshot saves into metadata record mutations", async () => {
    enableNativeLocalDatabase = true;
    (
      globalThis.window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }
    ).__TAURI_INTERNALS__ = { invoke };

    nativeSnapshots.set("native:none", {
      schema_version: 1,
      spaces: [],
      folders: [],
      documents: [],
      drafts: [
        {
          id: "draft-old",
          document_id: "doc-old",
          space_id: "local-space",
          owner_id: "user-1",
          title: "Old draft",
          content: "old",
          base_updated_at: "2026-07-08T11:00:00.000Z",
          created_at: "2026-07-08T11:00:00.000Z",
          updated_at: "2026-07-08T11:00:00.000Z",
        },
      ],
      assets: [],
      templates: [],
      property_schemas: [
        {
          id: "property-old",
          space_id: "local-space",
          name: "Old",
          property_type: "text",
          sort_order: 0,
        },
      ],
      property_values: [],
      tags: [{ id: "tag-old", space_id: "local-space", name: "Old", color: "#111827" }],
      doc_tags: [],
      search_index: [],
      sync_state: {
        state: "idle",
        updated_at: "2026-07-08T12:00:00.000Z",
        queued_operations: [],
      },
    });

    const vault = createLocalVaultAdapter("none");
    await vault.init();
    await vault.saveSnapshot({
      schema_version: 1,
      spaces: [],
      folders: [],
      documents: [
        {
          id: "doc-search",
          space_id: "local-space",
          title: "Searchable",
          content: "body",
          updated_at: "2026-07-08T12:05:00.000Z",
        },
      ],
      drafts: [
        {
          id: "draft-new",
          document_id: "doc-search",
          space_id: "local-space",
          owner_id: "user-1",
          title: "Searchable draft",
          content: "draft body",
          base_updated_at: "2026-07-08T12:00:00.000Z",
          created_at: "2026-07-08T12:00:00.000Z",
          updated_at: "2026-07-08T12:05:00.000Z",
        },
      ],
      assets: [],
      templates: [],
      property_schemas: [
        {
          id: "property-new",
          space_id: "local-space",
          name: "Status",
          property_type: "select",
          sort_order: 0,
        },
      ],
      property_values: [
        {
          id: "value-new",
          document_id: "doc-search",
          property_id: "property-new",
          value: "Ready",
        },
      ],
      tags: [{ id: "tag-new", space_id: "local-space", name: "Ready", color: "#22c55e" }],
      doc_tags: [{ document_id: "doc-search", tag_id: "tag-new" }],
      search_index: [
        {
          document_id: "doc-search",
          space_id: "local-space",
          title: "Searchable",
          content: "body",
          search_text: "searchable\nbody",
          updated_at: "2026-07-08T12:05:00.000Z",
        },
      ],
      sync_state: {
        state: "idle",
        updated_at: "2026-07-08T12:05:00.000Z",
        queued_operations: [],
      },
    });

    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "drafts", id: "draft-new" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord,
      expect.objectContaining({ collection: "drafts", id: "draft-old" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "property_schemas", id: "property-new" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "property_values", id: "value-new" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "tags", id: "tag-new" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "doc_tags", id: "doc-search:tag-new" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.objectContaining({ collection: "search_index", id: "doc-search" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord,
      expect.objectContaining({ collection: "property_schemas", id: "property-old" }),
    );
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord,
      expect.objectContaining({ collection: "tags", id: "tag-old" }),
    );
  });

  it("repairs invalid local page and folder structure without deleting records", () => {
    const checked = repairLocalVaultSnapshotStructure({
      schema_version: 1,
      spaces: [
        {
          id: "space-1",
          name: "Personal",
          created_at: "2026-07-14T00:00:00.000Z",
          updated_at: "2026-07-14T00:00:00.000Z",
        },
      ],
      folders: [{ id: "folder-1", space_id: "space-1", name: "Root", parent_id: "missing-folder" }],
      documents: [
        {
          id: "doc-parent",
          space_id: "space-1",
          title: "Parent",
          content: "",
          folder_id: "folder-1",
          updated_at: "2026-07-14T00:00:00.000Z",
        },
        {
          id: "doc-child",
          space_id: "space-1",
          title: "Child",
          content: "",
          folder_id: "missing-folder",
          parent_id: "doc-child",
          updated_at: "2026-07-14T00:00:00.000Z",
        },
      ],
      assets: [],
      templates: [],
      property_schemas: [],
      property_values: [],
      tags: [],
      doc_tags: [],
      search_index: [],
    });

    expect(checked.blockers).toEqual([]);
    expect(checked.repairs).toEqual([
      "Detached folder folder-1 from invalid parent",
      "Detached document doc-child from invalid folder",
      "Detached document doc-child from invalid parent page",
    ]);
    expect(checked.snapshot.folders).toHaveLength(1);
    expect(checked.snapshot.documents).toHaveLength(2);
    expect(checked.snapshot.folders[0].parent_id).toBeUndefined();
    expect(checked.snapshot.documents[1].folder_id).toBeUndefined();
    expect(checked.snapshot.documents[1].parent_id).toBeUndefined();
  });

  it("runs a startup storage check and persists safe structural repairs", async () => {
    const storageKey = "starlane_local_vault_v1_none";
    storage.set(
      storageKey,
      JSON.stringify({
        schema_version: 1,
        spaces: [
          {
            id: "space-1",
            name: "Personal",
            created_at: "2026-07-14T00:00:00.000Z",
            updated_at: "2026-07-14T00:00:00.000Z",
          },
        ],
        folders: [{ id: "folder-1", space_id: "space-1", name: "Root" }],
        documents: [
          {
            id: "doc-1",
            space_id: "space-1",
            title: "Doc",
            content: "",
            folder_id: "missing-folder",
            updated_at: "2026-07-14T00:00:00.000Z",
          },
        ],
        assets: [],
        templates: [],
      }),
    );

    const result = await runLocalVaultStartupCheck("none");
    const repaired = JSON.parse(storage.get(storageKey) ?? "{}") as LocalVaultSnapshot;

    expect(result.status).toBe("repaired");
    expect(result.counts.documents).toBe(1);
    expect(repaired.documents[0].folder_id).toBeUndefined();
  });
  it.each([4, 32, 256, 10_001])("retains valid %i-level page parent chains during startup normalization", depth => {
    const snapshot: LocalVaultSnapshot = { schema_version: 1, spaces: [{ id: "s", name: "Space", created_at: "old", updated_at: "old" }], folders: [], assets: [], templates: [], documents: Array.from({ length: depth }, (_, n) => ({ id: `p${n}`, space_id: "s", title: `Page ${n}`, content: "Raw\r\nbody", parent_id: n ? `p${n - 1}` : undefined, updated_at: "old" })) };
    const before = JSON.stringify(snapshot), checked = repairLocalVaultSnapshotStructure(snapshot);
    expect(checked.repairs).toEqual([]); expect(checked.blockers).toEqual([]);
    expect(checked.snapshot.documents).toEqual(snapshot.documents);
    expect(JSON.stringify(snapshot)).toBe(before);
  });
  it("still repairs actual page cycles and invalid parent edges without deleting source records", () => {
    const snapshot: LocalVaultSnapshot = { schema_version: 1, spaces: [{ id: "s", name: "Space", created_at: "old", updated_at: "old" }], folders: [], assets: [], templates: [], documents: [{ id: "a", space_id: "s", title: "A", content: "Raw A", updated_at: "old", parent_id: "b" }, { id: "b", space_id: "s", title: "B", content: "Raw B", updated_at: "old", parent_id: "a" }, { id: "orphan", space_id: "s", title: "O", content: "Keep", updated_at: "old", parent_id: "missing" }] };
    const checked = repairLocalVaultSnapshotStructure(snapshot);
    expect(checked.repairs.some(repair => repair.includes("cyclic parent chain"))).toBe(true);
    expect(checked.snapshot.documents.find(page => page.id === "orphan")?.parent_id).toBeUndefined();
    expect(checked.snapshot.documents.map(page => [page.id, page.content])).toEqual(snapshot.documents.map(page => [page.id, page.content]));
    expect(repairLocalVaultSnapshotStructure(checked.snapshot).repairs).toEqual([]);
  });
  it.each([4, 32, 256])("preserves deep page IDs/parents/raw source through the native atomic IPC contract and a cold adapter (%i)", async depth => {
    enableNativeLocalDatabase = true; enableAtomicNative = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ = { invoke };
    const adapter = createLocalVaultAdapter("none");
    const pages = Array.from({ length: depth }, (_, n) => ({ id: `native-page-${n}`, space_id: "native-space", title: `Page ${n}`, content: `Raw ${n}\r\n<!--retained-->`, parent_id: n ? `native-page-${n - 1}` : undefined, updated_at: "old" }));
    await adapter.commitSnapshotMutation(snapshot => ({ snapshot: { ...snapshot, spaces: [{ id: "native-space", name: "Native", created_at: "old", updated_at: "old" }], documents: pages }, operation: { id: "deep-native-operation", entity_type: "document", entity_id: pages[depth - 1].id, action: "create", payload: pages[depth - 1] } }));
    const cold = await createLocalVaultAdapter("none").loadSnapshot();
    expect(cold.documents).toEqual(pages);
    expect(cold.sync_state?.queued_operations.find(op => op.id === "deep-native-operation")?.payload).toEqual(pages[depth - 1]);
    expect(repairLocalVaultSnapshotStructure(cold).repairs).toEqual([]);
  });

  it("preserves reviewed draft transfer evidence through native atomic commit and cold reopen", async () => {
    enableNativeLocalDatabase = true; enableAtomicNative = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ = { invoke };
    const record = { schema_version: 1, operation_id: "transfer-native", state: "pending", server_base: "/api", server_account_id: "account", server_space_id: "space", server_space_name: "Space", server_document_id: "doc", server_document_title: "Page", request: { schema_version: 1, operation_id: "transfer-native", local_source: {}, server_source: {}, reviewed_draft: {}, selected_operation_fingerprints: ["fingerprint"] }, exact_manifest: { operation_count: 1, operations: [{ id: "asset-op", payload: { data_url: "data:text/plain;base64,SGVsbG8=" } }] }, prepared_at: "prepared", updated_at: "updated" } as unknown as NonNullable<LocalVaultSnapshot["reviewed_draft_transfer_records"]>[number];
    const adapter = createLocalVaultAdapter("none");

    await adapter.commitSnapshotMutation(snapshot => ({ snapshot: { ...snapshot, reviewed_draft_transfer_records: [record] } }));

    expect((await createLocalVaultAdapter("none").loadSnapshot()).reviewed_draft_transfer_records).toEqual([record]);
  });

  it("commits native values and queue together, rejects unsupported shells and preserves data on rejection", async () => {
    enableNativeLocalDatabase = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ =
      { invoke };
    const vault = createLocalVaultAdapter("none");
    const mutation = (snapshot: LocalVaultSnapshot) => {
      snapshot.property_values = [
        { id: "native-value", document_id: "doc", property_id: "field", value: "After" },
      ];
      return {
        snapshot,
        operation: {
          id: "atomic-native-operation",
          entity_type: "document_property_values",
          entity_id: "doc",
          action: "set",
          payload: {},
        },
      };
    };
    await expect(vault.commitSnapshotMutation(mutation)).rejects.toThrow("needs an update");
    enableAtomicNative = true;
    failNativeCommit = true;
    await expect(vault.commitSnapshotMutation(mutation)).rejects.toThrow("outcome is unknown");
    expect((await vault.loadSnapshot()).property_values).toEqual([]);
    expect(await vault.listQueuedSyncOperations()).toEqual([]);
    expect(invoke).not.toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.anything(),
    );
    failNativeCommit = false;
    failMirror = true;
    await vault.commitSnapshotMutation(mutation);
    expect((await vault.loadSnapshot()).property_values?.[0].value).toBe("After");
    expect((await vault.listQueuedSyncOperations()).map((operation) => operation.id)).toEqual([
      "atomic-native-operation",
    ]);
    expect(nativeSnapshots.get("native:none")?.mirror_pending).toBe(true);
    failMirror = false;
    const reopened = createLocalVaultAdapter("none");
    await reopened.init();
    expect(invoke).toHaveBeenCalledWith(NATIVE_LOCAL_DATABASE_COMMANDS.syncMirror, {
      provider: "none",
    });
    expect(nativeSnapshots.get("native:none")?.mirror_pending).toBe(false);
  });

  it("retains exact metadata intent nulls/source/draft context through atomic native queue and lost acknowledgement", async () => {
    enableNativeLocalDatabase = true; enableAtomicNative = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ = { invoke };
    const vault = createLocalVaultAdapter("none");
    await vault.init();
    loseNativeAcknowledgement = true;
    const payload = { id: "doc", space_id: "space", title: "Saved", icon: null, metadata_intent: { version: 1, fields: ["title", "icon"], source: { page_updated_at: "base", published_at: null }, draft: { mode: "exact", id: "draft", updated_at: "draft-source", base_updated_at: "base" } } };
    await vault.commitSnapshotMutation((snapshot) => {
      snapshot.documents = [{ id: "doc", space_id: "space", title: "Saved", content: "RAW retained", updated_at: "new" }];
      return { snapshot, operation: { id: "metadata-native", entity_type: "document", entity_id: "doc", action: "update", payload } };
    });
    loseNativeAcknowledgement = false;
    const saved = await createLocalVaultAdapter("none").loadSnapshot();
    expect(saved.documents[0]).toMatchObject({ title: "Saved", content: "RAW retained" });
    expect(saved.sync_state?.queued_operations).toHaveLength(1);
    expect(saved.sync_state?.queued_operations[0].payload).toEqual(payload);
    const previous = JSON.parse(JSON.stringify(nativeSnapshots.get("native:none")));
    failNativeCommit = true;
    await expect(vault.commitSnapshotMutation((snapshot) => {
      snapshot.documents[0].title = "Failed";
      return { snapshot, operation: { id: "metadata-failed", entity_type: "document", entity_id: "doc", action: "update", payload } };
    })).rejects.toThrow("Native commit failed or its outcome is unknown");
    expect(nativeSnapshots.get("native:none")).toEqual(previous);
    failNativeCommit = false;
    raceBootstrap = true;
    await expect(vault.commitSnapshotMutation((snapshot) => {
      snapshot.documents[0].title = "Stale";
      return { snapshot, operation: { id: "metadata-stale", entity_type: "document", entity_id: "doc", action: "update", payload } };
    })).rejects.toThrow("Native commit failed or its outcome is unknown");
    expect(nativeSnapshots.get("native:none")?.canonical_revision).toBe("external-revision");
    expect(nativeSnapshots.get("native:none")?.sync_state?.queued_operations.map((operation) => operation.id)).toEqual(["external-queue"]);
    expect(invoke).not.toHaveBeenCalledWith(NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot, expect.anything());
  });

  it("does not overwrite a concurrent native bootstrap with stale browser fallback data", async () => {
    const fallback = createLocalVaultAdapter("none");
    await fallback.commitSnapshotMutation((snapshot) => {
      snapshot.property_values = [
        { id: "fallback", document_id: "doc", property_id: "field", value: "Stale fallback" },
      ];
      return { snapshot };
    });
    enableNativeLocalDatabase = true;
    enableAtomicNative = true;
    raceBootstrap = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ =
      { invoke };
    const native = createLocalVaultAdapter("none");
    await native.init();
    expect((await native.loadSnapshot()).property_values?.[0].value).toBe("External");
    expect((await native.listQueuedSyncOperations()).map((operation) => operation.id)).toEqual([
      "external-queue",
    ]);
    expect(invoke).not.toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot,
      expect.anything(),
    );
    expect(nativeSnapshots.get("native:none")?.canonical_revision).toBe("external-revision");
  });

  it("fails closed on transient capability detection and retains known modern capability", async () => {
    enableNativeLocalDatabase = true;
    enableAtomicNative = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ =
      { invoke };
    failDescribe = true;
    const unknown = createLocalVaultAdapter("none");
    await expect(unknown.setSyncState("syncing")).rejects.toThrow("IPC disconnected");
    expect(invoke).not.toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot,
      expect.anything(),
    );
    expect(invoke).not.toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.anything(),
    );
    failDescribe = false;
    const known = createLocalVaultAdapter("none");
    expect(await known.supportsAtomicSnapshotCommit()).toBe(true);
    failDescribe = true;
    await known.setSyncState("syncing");
    expect(invoke).toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.commitSnapshot,
      expect.anything(),
    );
  });

  it("preserves identifier and original queue binding through native atomic CAS and rejects failed writes", async () => {
    enableNativeLocalDatabase = true; enableAtomicNative = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ = { invoke };
    const vault = createLocalVaultAdapter("none");
    await vault.commitSnapshotMutation((snapshot) => {
      snapshot.documents = [{ id: "identifier-page", space_id: "space", title: "Page", content: "Original body", updated_at: "2026-10-08T00:00:00Z", human_identifier: " ENG-1 " }];
      return { snapshot, operation: { id: "identifier-op", replay_provider: "none", server_context: { server_base: "/api/v1", account_id: "account", session_scope: "session" }, entity_type: "document", entity_id: "identifier-page", action: "update_identifier", payload: { id: "identifier-page", space_id: "space", human_identifier: "ENG-1", source: { page_updated_at: "2026-10-08T00:00:00Z", published_at: null, human_identifier: null } } } };
    });
    const before = await vault.loadSnapshot();
    expect(before.documents[0].human_identifier).toBe("ENG-1");
    const reopened = await createLocalVaultAdapter("none").loadSnapshot();
    expect(reopened.sync_state?.queued_operations[0]).toMatchObject({ replay_provider: "none", server_context: { account_id: "account" } });
    failNativeCommit = true;
    await expect(vault.commitSnapshotMutation((snapshot) => { snapshot.documents[0].human_identifier = "ENG-2"; return { snapshot }; })).rejects.toThrow("outcome is unknown");
    expect((await vault.loadSnapshot()).documents).toEqual(before.documents);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
    expect(invoke).toHaveBeenCalledWith(NATIVE_LOCAL_DATABASE_COMMANDS.commitSnapshot, expect.objectContaining({ expectedRevision: before.canonical_revision }));
  });
  it("preserves maintenance catalog and original queue binding through native atomic CAS and rejects failed writes", async () => {
    enableNativeLocalDatabase = true; enableAtomicNative = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ = { invoke };
    const reopenedCatalog = [{server_base:"/api/v1",account_id:"account",space_id:"space",targets:{users:[],groups:[{id:"team-a",name:"Team",managed_space_id:null}],groups_available:true}}];
    const vault = createLocalVaultAdapter("none");
    await vault.commitSnapshotMutation((snapshot) => {
      snapshot.maintenance_target_catalogs = reopenedCatalog;
      snapshot.documents = [{ id: "identifier-page", space_id: "space", title: "Page", content: "Original body", updated_at: "2026-10-08T00:00:00Z", maintenance_user_id: null, maintenance_group_id: "team-a" }];
      return { snapshot, operation: { id: "identifier-op", replay_provider: "none", server_context: { server_base: "/api/v1", account_id: "account", session_scope: "session" }, entity_type: "document", entity_id: "identifier-page", action: "update_maintenance", payload: { id: "identifier-page", space_id: "space", maintenance: {kind:"group",group_id:"team-a"}, source: { page_updated_at: "2026-10-08T00:00:00Z", published_at: null, maintenance_user_id:null,maintenance_group_id:null } } } };
    });
    const before = await vault.loadSnapshot();
    expect(before.documents[0].maintenance_group_id).toBe("team-a");
    expect(before.maintenance_target_catalogs).toEqual(reopenedCatalog);
    const reopened = await createLocalVaultAdapter("none").loadSnapshot();
    expect(reopened.sync_state?.queued_operations[0]).toMatchObject({ replay_provider: "none", server_context: { account_id: "account" } });
    expect(reopened.maintenance_target_catalogs).toEqual(reopenedCatalog);
    failNativeCommit = true;
    await expect(vault.commitSnapshotMutation((snapshot) => { snapshot.documents[0].maintenance_group_id = "team-b"; return { snapshot }; })).rejects.toThrow("outcome is unknown");
    expect((await vault.loadSnapshot()).documents).toEqual(before.documents);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).sync_state?.queued_operations).toEqual(before.sync_state?.queued_operations);
    expect(invoke).toHaveBeenCalledWith(NATIVE_LOCAL_DATABASE_COMMANDS.commitSnapshot, expect.objectContaining({ expectedRevision: before.canonical_revision }));
  });

  it("removes native asset bytes after canonical metadata commit and treats cleanup failure as advisory", async () => {
    enableNativeLocalDatabase = true;
    enableAtomicNative = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ =
      { invoke };
    const vault = createLocalVaultAdapter("none");
    const asset = {
      id: "cleanup-asset",
      name: "File",
      mime_type: "text/plain",
      size: 4,
      url: "asset://file",
      source: "tauri" as const,
      vault_path: "/tmp/cleanup-file",
      created_at: "now",
    };
    await vault.saveAsset(asset);
    await vault.deleteAsset(asset.id);
    expect(invoke).toHaveBeenCalledWith("delete_vault_asset", { vaultPath: asset.vault_path });
    expect(await vault.listAssets()).toEqual([]);
    await vault.saveAsset(asset);
    failAssetCleanup = true;
    const warning = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    try {
      await vault.deleteAsset(asset.id);
      expect(warning).toHaveBeenCalledOnce();
    } finally {
      warning.mockRestore();
    }
    expect(await vault.listAssets()).toEqual([]);
    expect(nativeSnapshots.get("native:none")?.assets).toEqual([]);
  });

  it("keeps newer native property commits when a stale window writes its queue and mirror refresh fails", async () => {
    enableNativeLocalDatabase = true;
    enableAtomicNative = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ =
      { invoke };
    const stale = createLocalVaultAdapter("none");
    await stale.init();
    const writer = createLocalVaultAdapter("none");
    await writer.commitSnapshotMutation((snapshot) => {
      snapshot.property_values = [
        { id: "kept", document_id: "doc", property_id: "field", value: "Newer" },
      ];
      return {
        snapshot,
        operation: {
          id: "newer-property-operation",
          entity_type: "document_property_values",
          entity_id: "doc",
          action: "set",
          payload: {},
        },
      };
    });
    failMirror = true;
    invoke.mockClear();
    await stale.enqueueSyncOperation({
      id: "stale-extra",
      entity_type: "document",
      entity_id: "other",
      action: "update",
      payload: {},
    });
    expect((await stale.loadSnapshot()).property_values?.[0].value).toBe("Newer");
    expect((await stale.listQueuedSyncOperations()).map((operation) => operation.id)).toEqual([
      "newer-property-operation",
      "stale-extra",
    ]);
    expect(invoke).not.toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot,
      expect.anything(),
    );
    expect(invoke).not.toHaveBeenCalledWith(
      NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord,
      expect.anything(),
    );
  });

  it("reconciles a lost native commit acknowledgement without duplicating its queued operation", async () => {
    enableNativeLocalDatabase = true;
    enableAtomicNative = true;
    loseNativeAcknowledgement = true;
    (window as unknown as { __TAURI_INTERNALS__: { invoke: typeof invoke } }).__TAURI_INTERNALS__ =
      { invoke };
    const vault = createLocalVaultAdapter("none");
    await vault.commitSnapshotMutation((snapshot) => {
      snapshot.property_values = [
        { id: "native-value", document_id: "doc", property_id: "field", value: "Saved" },
      ];
      return {
        snapshot,
        operation: {
          id: "lost-ack-operation",
          entity_type: "document_property_values",
          entity_id: "doc",
          action: "set",
          payload: {},
        },
      };
    });
    expect((await vault.loadSnapshot()).property_values?.[0].value).toBe("Saved");
    expect((await vault.listQueuedSyncOperations()).map((operation) => operation.id)).toEqual([
      "lost-ack-operation",
    ]);
  });
});

describe("atomic vault snapshot mutations", () => {
  const storage = new Map<string, string>();
  let failStorage = false;
  beforeEach(() => {
    storage.clear();
    failStorage = false;
    const localStorage = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        if (failStorage) throw new Error("QuotaExceededError");
        storage.set(key, value);
      },
      removeItem: (key: string) => storage.delete(key),
    };
    (globalThis as { window?: unknown }).window = { localStorage };
  });
  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
  });

  const mutation = (id: string) => (snapshot: LocalVaultSnapshot) => {
    snapshot.property_values = [
      ...(snapshot.property_values ?? []),
      { id, document_id: "doc", property_id: id, value: id },
    ];
    return {
      snapshot,
      operation: {
        id: `queue-${id}`,
        entity_type: "document_property_values",
        entity_id: "doc",
        action: "set",
        payload: { id },
      },
    };
  };

  it("preserves committed properties and queue after canonical storage rejection", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.commitSnapshotMutation(mutation("before"));
    const before = JSON.parse(JSON.stringify(await vault.loadSnapshot()));
    const diskBefore = new Map(storage);
    failStorage = true;
    await expect(vault.commitSnapshotMutation(mutation("failed"))).rejects.toThrow(
      "QuotaExceededError",
    );
    expect(await vault.loadSnapshot()).toMatchObject({
      property_values: before.property_values,
      sync_state: { queued_operations: before.sync_state.queued_operations },
    });
    expect(storage).toEqual(diskBefore);
    failStorage = false;
    const reopened = createLocalVaultAdapter("none");
    expect(await reopened.loadSnapshot()).toMatchObject({
      property_values: before.property_values,
      sync_state: { queued_operations: before.sync_state.queued_operations },
    });
  });

  it("serializes concurrent candidates and retains each value and queued operation on reopen", async () => {
    const vault = createLocalVaultAdapter("none");
    const other = createLocalVaultAdapter("none");
    await Promise.all([
      vault.commitSnapshotMutation(mutation("one")),
      other.commitSnapshotMutation(mutation("two")),
    ]);
    const reopened = await createLocalVaultAdapter("none").loadSnapshot();
    expect(reopened.property_values?.map((value) => value.id)).toEqual(["one", "two"]);
    expect(reopened.sync_state?.queued_operations.map((operation) => operation.id)).toEqual([
      "queue-one",
      "queue-two",
    ]);
  });

  it("applies stale browser queue and record changes as deltas without losing newer properties", async () => {
    const document = {
      id: "other-doc",
      space_id: "space",
      title: "Other",
      content: "body",
      created_at: "now",
      updated_at: "now",
    };
    const changes: Array<(vault: ReturnType<typeof createLocalVaultAdapter>) => Promise<unknown>> =
      [
        (vault) => vault.setSyncState("syncing"),
        (vault) =>
          vault.enqueueSyncOperation({
            id: "extra",
            entity_type: "document",
            entity_id: "other",
            action: "update",
            payload: {},
          }),
        (vault) =>
          vault.updateQueuedSyncOperations([
            { id: "queue-kept", attempts: 2, last_error: "retry" },
          ]),
        (vault) => vault.clearQueuedSyncOperations(["missing-queue-id"]),
        (vault) => vault.saveDocument(document),
        (vault) => vault.deleteDocument("missing-document"),
        (vault) =>
          vault.saveTemplate({
            id: "other-template",
            space_id: "space",
            name: "Other",
            content: "body",
            tags: [],
            created_at: "now",
            updated_at: "now",
          }),
        (vault) => vault.deleteTemplate("missing-template"),
      ];
    for (const change of changes) {
      storage.clear();
      const stale = createLocalVaultAdapter("none");
      await stale.init();
      await createLocalVaultAdapter("none").commitSnapshotMutation(mutation("kept"));
      await change(stale);
      const reopened = await createLocalVaultAdapter("none").loadSnapshot();
      expect(reopened.property_values?.map((value) => value.id)).toEqual(["kept"]);
      expect(reopened.sync_state?.queued_operations.map((operation) => operation.id)).toContain(
        "queue-kept",
      );
    }
  });

  it("rejects an IndexedDB abort without publishing and tolerates a postcommit cache failure", async () => {
    const indexed = new Map<string, LocalVaultSnapshot>();
    let abortWrite = false;
    const db = {
      transaction: (_store: string, mode: string) => {
        const tx = {
          oncomplete: null,
          onerror: null,
          onabort: null,
          error: null,
        } as unknown as IDBTransaction;
        Object.assign(tx, { abort: () => queueMicrotask(() => tx.onabort?.({} as Event)) });
        const objectStore = {
          get: (key: string) => {
            const request = { result: indexed.get(key), onsuccess: null } as unknown as IDBRequest;
            queueMicrotask(() => request.onsuccess?.({} as Event));
            return request;
          },
          put: (snapshot: LocalVaultSnapshot, key: string) => {
            queueMicrotask(() => {
              if (abortWrite) tx.onabort?.({} as Event);
              else {
                indexed.set(key, JSON.parse(JSON.stringify(snapshot)));
                tx.oncomplete?.({} as Event);
              }
            });
          },
        };
        Object.assign(tx, { objectStore: () => objectStore });
        expect(["readonly", "readwrite"]).toContain(mode);
        return tx;
      },
    };
    Object.assign(window, {
      indexedDB: {
        open: () => {
          const request = { result: db, onsuccess: null } as unknown as IDBOpenDBRequest;
          queueMicrotask(() => request.onsuccess?.({} as Event));
          return request;
        },
      },
    });
    const vault = createLocalVaultAdapter("none");
    await vault.commitSnapshotMutation(mutation("before"));
    await expect(
      vault.commitSnapshotMutation(() => {
        throw new Error("Invalid candidate");
      }),
    ).rejects.toThrow("Invalid candidate");
    expect((await vault.loadSnapshot()).property_values?.map((value) => value.id)).toEqual([
      "before",
    ]);
    abortWrite = true;
    await expect(vault.commitSnapshotMutation(mutation("failed"))).rejects.toThrow("aborted");
    expect((await vault.loadSnapshot()).property_values?.map((value) => value.id)).toEqual([
      "before",
    ]);
    abortWrite = false;
    failStorage = true;
    await createLocalVaultAdapter("none").commitSnapshotMutation(mutation("committed"));
    expect(
      (await createLocalVaultAdapter("none").loadSnapshot()).property_values?.map(
        (value) => value.id,
      ),
    ).toEqual(["before", "committed"]);
  });
});
