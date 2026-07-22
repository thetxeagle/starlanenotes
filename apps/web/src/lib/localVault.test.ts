import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createLocalVaultAdapter,
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
  snapshotRecords[collection] = records.filter(
    (record) => testRecordId(collection, record) !== id,
  );
}

describe("localVault adapter", () => {
  const storage = new Map<string, string>();
  const nativeSnapshots = new Map<string, LocalVaultSnapshot>();
  let enableNativeLocalDatabase = false;
  let failNativeRecordCommands = false;
  const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
    const provider = String(args?.provider ?? "none");
    switch (cmd) {
      case NATIVE_LOCAL_DATABASE_COMMANDS.describe:
        if (!enableNativeLocalDatabase) throw new Error("native local database unavailable");
        return {
          schema_version: 1,
          provider,
          store_kind: "sqlite",
          database_path: "/tmp/starlane-vault.sqlite3",
          supports_snapshot_bridge: true,
          supports_operation_queue: true,
          replay_backends: ["sqlite", "postgresql"],
        };
      case NATIVE_LOCAL_DATABASE_COMMANDS.readSnapshot:
        return nativeSnapshots.get(`native:${provider}`) ?? null;
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
      default:
        throw new Error(`Unexpected command: ${cmd}`);
    }
  });

  beforeEach(() => {
    storage.clear();
    nativeSnapshots.clear();
    enableNativeLocalDatabase = false;
    failNativeRecordCommands = false;
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

  it("starts empty after reset", async () => {
    const vault = createLocalVaultAdapter("none");
    await vault.reset();
    await vault.init();
    const spaces = await vault.listSpaces();
    expect(spaces.length).toBe(0);
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
      url: "data:image/png;base64,abc",
      source: "browser",
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
      folders: [
        { id: "folder-1", space_id: "space-1", name: "Root", parent_id: "missing-folder" },
      ],
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
});
