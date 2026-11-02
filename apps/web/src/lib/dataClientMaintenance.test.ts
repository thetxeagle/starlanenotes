import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { dataClient, __resetLocalDataForTests } from "./dataClient";
import { api, ApiError } from "./api";
import { createLocalVaultAdapter } from "./localVault";
import { setCurrentLaunchMode } from "./launchMode";
import { setSyncProvider } from "./storageProviders";
import type { VaultDocument } from "./localVault";
const token = (id: string) => `header.${btoa(JSON.stringify({ sub: id }))}.test-signature`;
const source = (row: VaultDocument) => ({
  page_updated_at: row.updated_at,
  published_at: row.published_at ?? null,
  maintenance_user_id: row.maintenance_user_id ?? null,
  maintenance_group_id: row.maintenance_group_id ?? null,
});
describe("maintenance client durability", () => {
  const storage = new Map<string, string>();
  beforeEach(async () => {
    storage.clear();
    const mock = {
      getItem: (k: string) => storage.get(k) ?? null,
      setItem: (k: string, v: string) => {
        storage.set(k, v);
      },
      removeItem: (k: string) => {
        storage.delete(k);
      },
    };
    (globalThis as { window?: unknown }).window = { localStorage: mock };
    (globalThis as { localStorage?: unknown }).localStorage = mock;
    setCurrentLaunchMode("local");
    setSyncProvider("none");
    await __resetLocalDataForTests();
  });
  afterEach(() => {
    vi.restoreAllMocks();
    api.setToken(null);
    delete (globalThis as { window?: unknown }).window;
    delete (globalThis as { localStorage?: unknown }).localStorage;
  });
  it("commits local person with source and queue without draft/attribution changes; duplicate clears", async () => {
    const page = await dataClient.createDocument("local-space", {
      title: "Responsible",
      content: "Unchanged private body",
    });
    const before = await dataClient.exportLocalVaultSnapshot(),
      row = before.documents.find((d) => d.id === page.id)!;
    expect(await dataClient.getDocumentMaintenanceTargets("local-space", page.id)).toEqual({
      users: [{ id: "local-user", name: "Local User", active: true }],
      groups: [],
      groups_available: false,
    });
    const result = await dataClient.setDocumentMaintenance(
      "local-space",
      page.id,
      { kind: "user", user_id: "local-user" },
      source(row),
    );
    expect(result.disposition).toBe("applied");
    const saved = await createLocalVaultAdapter("none").loadSnapshot();
    expect(saved.documents.find((d) => d.id === page.id)).toMatchObject({
      maintenance_user_id: "local-user",
      maintenance_group_id: null,
      content: row.content,
      created_by: row.created_by,
    });
    expect(saved.drafts).toEqual(before.drafts);
    expect(
      saved.sync_state!.queued_operations.find((op) => op.id === result.operation_id),
    ).toMatchObject({
      action: "update_maintenance",
      replay_provider: "none",
      payload: {
        id: page.id,
        space_id: "local-space",
        maintenance: { kind: "user", user_id: "local-user" },
        source: source(row),
      },
    });
    expect(
      (await dataClient.searchDocuments("", 25, { maintenance_user: "local-user" })).map(
        (d) => d.document_id,
      ),
    ).toContain(page.id);
    expect(
      (await dataClient.searchDocuments("", 25, { maintenance_user: "LOCAL-USER" })).map(
        (d) => d.document_id,
      ),
    ).not.toContain(page.id);
    expect(
      (await dataClient.duplicateDocument("local-space", page.id)).maintenance_user_id,
    ).toBeUndefined();
  });
  it("preserves unresolved unchanged pair, rejects fake team and same-timestamp SETNULL source", async () => {
    const page = await dataClient.createDocument("local-space", {
      title: "Imported",
      maintenance_user_id: "inactive",
    });
    const before = await dataClient.exportLocalVaultSnapshot(),
      row = before.documents.find((d) => d.id === page.id)!;
    await dataClient.setDocumentMaintenance(
      "local-space",
      page.id,
      { kind: "user", user_id: "inactive" },
      source(row),
    );
    await expect(
      dataClient.setDocumentMaintenance(
        "local-space",
        page.id,
        { kind: "group", group_id: "fake" },
        source(row),
      ),
    ).rejects.toThrow(/catalog/);
    const cleared = await dataClient.exportLocalVaultSnapshot();
    cleared.documents.find((d) => d.id === page.id)!.maintenance_user_id = null;
    await dataClient.importLocalVaultSnapshot(cleared);
    await expect(
      dataClient.setDocumentMaintenance(
        "local-space",
        page.id,
        { kind: "user", user_id: "local-user" },
        source(row),
      ),
    ).rejects.toThrow(/source changed/);
    const bad = structuredClone(cleared);
    Object.assign(bad.documents.find((d) => d.id === page.id)!, {
      maintenance_user_id: "person",
      maintenance_group_id: "team",
    });
    await expect(dataClient.importLocalVaultSnapshot(bad)).rejects.toThrow(/both/);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(
      cleared.documents,
    );
  });
  it("retains cache/queue/snapshot on persistence failure", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Failure" });
    const before = await dataClient.exportLocalVaultSnapshot(),
      row = before.documents.find((d) => d.id === page.id)!;
    const original = window.localStorage.setItem;
    window.localStorage.setItem = () => {
      throw new Error("QuotaExceededError");
    };
    try {
      await expect(
        dataClient.setDocumentMaintenance(
          "local-space",
          page.id,
          { kind: "user", user_id: "local-user" },
          source(row),
        ),
      ).rejects.toThrow("QuotaExceededError");
      expect(
        (await dataClient.getDocument("local-space", page.id)).maintenance_user_id,
      ).toBeUndefined();
    } finally {
      window.localStorage.setItem = original;
    }
    const after = await dataClient.exportLocalVaultSnapshot();
    expect(after.documents).toEqual(before.documents);
    expect(after.sync_state!.queued_operations).toEqual(before.sync_state!.queued_operations);
  });
  it("retains account-bound offline intent and retries exact original operation without historical projection", async () => {
    const page = await dataClient.createDocument("local-space", {
      title: "Downloaded",
      maintenance_user_id: "old",
    });
    const before = await dataClient.exportLocalVaultSnapshot(),
      row = before.documents.find((d) => d.id === page.id)!;
    setCurrentLaunchMode("server");
    api.setToken(token("account-a"));
    let lost = true;
    const bodies: unknown[] = [];
    vi.spyOn(api, "fetch").mockImplementation(async (path, options) => {
      if (path === "/auth/me") return { id: "account-a" };
      if (path === `/spaces/local-space/documents/${page.id}`) return row;
      if (path === `/spaces/local-space/documents/${page.id}/maintenance`) {
        bodies.push(options?.body);
        if (lost) throw new ApiError("Target requires review", 409);
        return {
          operation_id: (options!.body as { operation_id: string }).operation_id,
          maintenance_user_id: null,
          maintenance_group_id: null,
          page_updated_at: "2026-10-08T12:00:00Z",
          published_at: null,
          historical: true,
        };
      }
      throw new Error(path);
    });
    const pending = await dataClient.setDocumentMaintenance(
      "local-space",
      page.id,
      { kind: "unassigned" },
      source(row),
    );
    expect(pending).toMatchObject({
      disposition: "queued",
      pending_error: "Target requires review",
    });
    expect(await dataClient.getPendingDocumentMaintenance("local-space", page.id)).toMatchObject({
      operation_id: pending.operation_id,
      maintenance: { kind: "unassigned" },
      source: source(row),
    });
    const saved = await createLocalVaultAdapter("none").loadSnapshot();
    expect(saved.documents).toEqual(before.documents);
    expect(saved.drafts).toEqual(before.drafts);
    lost = false;
    expect(
      await dataClient.setDocumentMaintenance(
        "local-space",
        page.id,
        { kind: "unassigned" },
        source(row),
      ),
    ).toMatchObject({ operation_id: pending.operation_id, disposition: "historical" });
    expect(bodies[1]).toEqual(bodies[0]);
    expect(Object.keys(bodies[0] as object).sort()).toEqual([
      "maintenance",
      "operation_id",
      "provider",
      "source",
    ]);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(
      before.documents,
    );
  });
  it("rejects initial account switch before queue or PATCH, and caches exact targets", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Identity" });
    const before = await dataClient.exportLocalVaultSnapshot(),
      row = before.documents.find((d) => d.id === page.id)!;
    setCurrentLaunchMode("server");
    api.setToken(token("account-a"));
    const fetch = vi.spyOn(api, "fetch").mockImplementation(async (path) => {
      expect(path).toBe("/auth/me");
      api.setToken(token("account-b"));
      return { id: "account-a" };
    });
    await expect(
      dataClient.setDocumentMaintenance(
        "local-space",
        page.id,
        { kind: "unassigned" },
        source(row),
      ),
    ).rejects.toThrow(/account changed/);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(
      (await createLocalVaultAdapter("none").loadSnapshot()).sync_state!.queued_operations,
    ).toEqual(before.sync_state!.queued_operations);
    api.setToken(token("account-a"));
    const targets = {
      users: [{ id: "person", name: "Person", active: true }],
      groups: [{ id: "team", name: "Team", managed_space_id: null }],
      groups_available: true,
    };
    fetch.mockImplementation(async (path) => (path === "/auth/me" ? { id: "account-a" } : targets));
    expect(await dataClient.getDocumentMaintenanceTargets("local-space", page.id)).toEqual(targets);
    expect(
      (await createLocalVaultAdapter("none").loadSnapshot()).maintenance_target_catalogs,
    ).toEqual([
      { server_base: "/api/v1", account_id: "account-a", space_id: "local-space", targets },
    ]);
    setCurrentLaunchMode("local");
    await dataClient.setDocumentMaintenance(
      "local-space",
      page.id,
      { kind: "group", group_id: "team" },
      source(row),
    );
    expect((await dataClient.getDocument("local-space", page.id)).maintenance_group_id).toBe(
      "team",
    );
  });
  it("preflights all cross-server mapping before any server entity creation", async () => {
    await dataClient.createDocument("local-space", {
      title: "Foreign",
      maintenance_group_id: "source-team",
    });
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    setCurrentLaunchMode("server");
    api.setToken(token("destination"));
    const fetch = vi.spyOn(api, "fetch").mockImplementation(async (path) => {
      if (path === "/auth/me") return { id: "destination" };
      throw new Error(`Unexpected write ${path}`);
    });
    await expect(dataClient.importVaultSnapshotToServer(snapshot)).rejects.toThrow(/Map every/);
    expect(fetch).toHaveBeenCalledTimes(1);
    await expect(
      dataClient.importVaultSnapshotToServer(snapshot, {
        maintenance_identity_map: { users: {}, groups: { "source-team": "" } },
      }),
    ).rejects.toThrow(/Map every/);
    expect(fetch).toHaveBeenCalledTimes(2);
  });
  it.each(["before-callback", "during-ack"])(
    "preserves concurrent body/draft bases during maintenance commit (%s)",
    async (boundary) => {
      const page = await dataClient.createDocument("local-space", {
        title: "Concurrent",
        content: "Initial",
      });
      const draft = await dataClient.getDocumentDraft("local-space", page.id, page.draft_id!);
      const row = (await dataClient.exportLocalVaultSnapshot()).documents.find(
        (d) => d.id === page.id,
      )!;
      let release!: () => void, reached!: () => void;
      const gate = new Promise<void>((r) => {
          release = r;
        }),
        started = new Promise<void>((r) => {
          reached = r;
        });
      const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(
        createLocalVaultAdapter("none"),
      );
      const original = prototype.commitSnapshotMutation;
      vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (
        this: ReturnType<typeof createLocalVaultAdapter>,
        mutate,
      ) {
        if (boundary === "before-callback") {
          reached();
          await gate;
          return original.call(this, mutate);
        }
        const result = await original.call(this, mutate);
        reached();
        await gate;
        return result;
      });
      const assignment = dataClient.setDocumentMaintenance(
        "local-space",
        page.id,
        { kind: "user", user_id: "local-user" },
        source(row),
      );
      await started;
      const body = dataClient.updateDocumentContent("local-space", page.id, "Concurrent body");
      const stagedDraftBody = { content: "Concurrent draft", base_updated_at: "Concurrent base" };
      const draftBody = dataClient.updateDocumentDraft(
        "local-space",
        page.id,
        draft.id,
        stagedDraftBody,
      );
      try {
        await vi.waitFor(async () => {
          expect((await dataClient.getDocumentContent("local-space", page.id)).content).toContain(
            "Concurrent body",
          );
          expect(await dataClient.getDocumentDraft("local-space", page.id, draft.id)).toMatchObject(
            { content: "Concurrent draft", base_updated_at: "Concurrent base" },
          );
        });
      } finally {
        release();
      }
      await Promise.all([assignment, body, draftBody]);
      const saved = await createLocalVaultAdapter("none").loadSnapshot();
      expect(saved.documents.find((d) => d.id === page.id)).toMatchObject({
        maintenance_user_id: "local-user",
        content: "Concurrent body",
      });
      expect(saved.drafts!.find((d) => d.id === draft.id)).toMatchObject({
        owner_id: draft.owner_id,
        content: "Concurrent draft",
        base_updated_at: "Concurrent base",
      });
    },
  );
  it("imports exact remote maintenance without echo and retains malformed/stale actions unchanged", async () => {
    const page = await dataClient.createDocument("local-space", { title: "Import" });
    await dataClient.clearQueuedSyncOperations();
    const before = await dataClient.exportLocalVaultSnapshot(),
      row = before.documents.find((d) => d.id === page.id)!;
    const {
      buildProviderRemoteMutationEnvelope,
      buildProviderRemoteMutationPlan,
      buildProviderRemoteSyncManifest,
    } = await import("./providerAdapters");
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
      "2026-10-08T12:00:00Z",
      "maintenance-envelope",
    );
    const payload = {
      id: page.id,
      space_id: "local-space",
      maintenance: { kind: "user", user_id: "local-user" },
      source: source(row),
    };
    const manifest = (id: string, value: unknown) =>
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id,
            entity_type: "document",
            entity_id: page.id,
            action: "update_maintenance",
            payload_status: "available",
            payload: value,
          },
        ],
      });
    const applied = await dataClient.applyProviderRemoteImportManifest(
      manifest("maintenance-import", payload),
    );
    expect(applied.applied_operation_count, JSON.stringify(applied)).toBe(1);
    const saved = await dataClient.exportLocalVaultSnapshot();
    expect(saved.documents.find((d) => d.id === page.id)!.maintenance_user_id).toBe("local-user");
    expect(saved.sync_state!.queued_operations).toEqual(before.sync_state!.queued_operations);
    for (const bad of [
      { ...payload, owner_id: "hidden" },
      { ...payload, maintenance: { kind: "group", group_id: "fake" } },
      { ...payload, source: { ...payload.source, maintenance_user_id: undefined } },
    ]) {
      expect(
        (await dataClient.applyProviderRemoteImportManifest(manifest("maintenance-bad", bad)))
          .applied_operation_count,
      ).toBe(0);
      expect((await dataClient.exportLocalVaultSnapshot()).documents).toEqual(saved.documents);
    }
  });
  it("publishes ACK only onto full original maintenance source, including SETNULL without revision bump", async () => {
    const page = await dataClient.createDocument("local-space", {
      title: "Source",
      maintenance_user_id: "old-person",
    });
    const before = await dataClient.exportLocalVaultSnapshot(),
      row = before.documents.find((d) => d.id === page.id)!;
    setCurrentLaunchMode("server");
    api.setToken(token("account-a"));
    const { cacheServerResponse, readCachedServerResponse, serverCacheScopeForSession } =
      await import("./offlineServerCache");
    const path = `/spaces/local-space/documents/${page.id}`,
      scope = serverCacheScopeForSession(api.getToken(), api.getRefreshToken());
    cacheServerResponse("/api/v1", path, row, scope);
    vi.spyOn(api, "fetch").mockImplementation(async (request, options) => {
      if (request === "/auth/me") return { id: "account-a" };
      if (request === path) return row;
      if (request === `${path}/maintenance`) {
        cacheServerResponse("/api/v1", path, { ...row, maintenance_user_id: null }, scope);
        return {
          operation_id: (options!.body as { operation_id: string }).operation_id,
          maintenance_user_id: "new-person",
          maintenance_group_id: null,
          page_updated_at: "2026-10-08T12:00:00Z",
          published_at: null,
          historical: false,
        };
      }
      throw new Error(request);
    });
    expect(
      (
        await dataClient.setDocumentMaintenance(
          "local-space",
          page.id,
          { kind: "user", user_id: "new-person" },
          source(row),
        )
      ).disposition,
    ).toBe("applied");
    expect(
      readCachedServerResponse<{ maintenance_user_id: string | null }>("/api/v1", path, scope)!
        .maintenance_user_id,
    ).toBeNull();
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(
      before.documents,
    );
  });
  it("passes explicit cross-server target mapping into known create payload without rewriting historical attribution", async () => {
    const snapshot = {
      schema_version: 1,
      spaces: [
        {
          id: "source-space",
          name: "Imported",
          created_at: "2026-10-08T00:00:00Z",
          updated_at: "2026-10-08T00:00:00Z",
        },
      ],
      folders: [],
      documents: [
        {
          id: "source-page",
          space_id: "source-space",
          title: "Imported page",
          content: "Exact body",
          maintenance_user_id: null,
          maintenance_group_id: "source-team",
          created_by: "old-creator",
          updated_at: "2026-10-08T00:00:00Z",
        },
      ],
      assets: [],
      templates: [],
    };
    setCurrentLaunchMode("server");
    api.setToken(token("destination"));
    const writes: { path: string; body: unknown }[] = [];
    vi.spyOn(api, "fetch").mockImplementation(async (path, options) => {
      if (path === "/auth/me") return { id: "destination" };
      writes.push({ path, body: options?.body });
      if (path === "/spaces") return { id: "destination-space", name: "Imported" };
      if (path === "/spaces/destination-space/documents") return { id: "destination-page" };
      if (path === "/spaces/destination-space/documents/destination-page/content") return undefined;
      throw new Error(path);
    });
    await dataClient.importVaultSnapshotToServer(snapshot, {
      maintenance_identity_map: { users: {}, groups: { "source-team": "destination-team" } },
    });
    expect(
      writes.find((write) => write.path === "/spaces/destination-space/documents")!.body,
    ).toMatchObject({
      maintenance_user_id: null,
      maintenance_group_id: "destination-team",
      source_id: "source-page",
      content: "Exact body",
    });
    expect(snapshot.documents[0].maintenance_group_id).toBe("source-team");
  });
  it.each(["before-callback", "after-commit"])(
    "rejects catalog account drift during CAS %s",
    async (boundary) => {
      const page = await dataClient.createDocument("local-space", { title: "Catalog identity" });
      const before = await dataClient.exportLocalVaultSnapshot();
      setCurrentLaunchMode("server");
      api.setToken(token("account-a"));
      const targets = {
        users: [],
        groups: [
          { id: "private-team-a", name: "Account A management team", managed_space_id: null },
        ],
        groups_available: true,
      };
      vi.spyOn(api, "fetch").mockImplementation(async (path) => {
        if (path === "/auth/me") return { id: "account-a" };
        if (path.endsWith("/maintenance/targets")) return targets;
        throw new Error(path);
      });
      let release!: () => void, reached!: () => void;
      const gate = new Promise<void>((r) => {
        release = r;
      });
      const started = new Promise<void>((r) => {
        reached = r;
      });
      const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(
        createLocalVaultAdapter("none"),
      );
      const original = prototype.commitSnapshotMutation;
      vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (
        this: ReturnType<typeof createLocalVaultAdapter>,
        mutate,
      ) {
        if (boundary === "before-callback") {
          reached();
          await gate;
          return original.call(this, mutate);
        }
        const result = await original.call(this, mutate);
        reached();
        await gate;
        return result;
      });
      const loading = dataClient.getDocumentMaintenanceTargets("local-space", page.id);
      const rejected = expect(loading).rejects.toThrow(/account changed/);
      try {
        await started;
        api.setToken(token("account-b"));
      } finally {
        release();
      }
      await rejected;
      const saved = await createLocalVaultAdapter("none").loadSnapshot();
      expect(saved.documents).toEqual(before.documents);
      expect(saved.sync_state!.queued_operations).toEqual(before.sync_state!.queued_operations);
      expect(saved.sync_state!.state).toBe(before.sync_state!.state);
      expect(saved.maintenance_target_catalogs ?? []).toEqual(
        boundary === "before-callback"
          ? []
          : [
              expect.objectContaining({
                account_id: "account-a",
                space_id: "local-space",
                targets,
              }),
            ],
      );
      expect(
        saved.maintenance_target_catalogs?.some((record) => record.account_id === "account-b"),
      ).not.toBe(true);
    },
  );
  it.each(["queue-before", "queue-after", "ack-before", "ack-after"])(
    "rejects maintenance account drift at %s",
    async (boundary) => {
      const page = await dataClient.createDocument("local-space", {
        title: "CAS identity",
        content: "Original body",
      });
      const before = await dataClient.exportLocalVaultSnapshot();
      const row = before.documents.find((record) => record.id === page.id)!;
      setCurrentLaunchMode("server");
      api.setToken(token("account-a"));
      let patches = 0;
      vi.spyOn(api, "fetch").mockImplementation(async (path, options) => {
        if (path === "/auth/me") return { id: "account-a" };
        if (path === `/spaces/local-space/documents/${page.id}`) return row;
        if (path.endsWith("/maintenance")) {
          patches++;
          return {
            operation_id: (options!.body as { operation_id: string }).operation_id,
            maintenance_user_id: null,
            maintenance_group_id: null,
            page_updated_at: "2026-10-08T12:00:00Z",
            published_at: null,
            historical: false,
          };
        }
        throw new Error(path);
      });
      let release!: () => void, reached!: () => void;
      const gate = new Promise<void>((r) => {
        release = r;
      });
      const started = new Promise<void>((r) => {
        reached = r;
      });
      const prototype: ReturnType<typeof createLocalVaultAdapter> = Object.getPrototypeOf(
        createLocalVaultAdapter("none"),
      );
      const original = prototype.commitSnapshotMutation;
      let commits = 0;
      vi.spyOn(prototype, "commitSnapshotMutation").mockImplementation(async function (
        this: ReturnType<typeof createLocalVaultAdapter>,
        mutate,
      ) {
        commits++;
        const held = commits === (boundary.startsWith("queue") ? 1 : 2);
        if (!held) return original.call(this, mutate);
        if (boundary.endsWith("before")) {
          reached();
          await gate;
          return original.call(this, mutate);
        }
        const result = await original.call(this, mutate);
        reached();
        await gate;
        return result;
      });
      const saving = dataClient.setDocumentMaintenance(
        "local-space",
        page.id,
        { kind: "unassigned" },
        source(row),
      );
      const rejected = expect(saving).rejects.toThrow(/account changed/);
      try {
        await started;
        api.setToken(token("account-b"));
      } finally {
        release();
      }
      await rejected;
      const saved = await createLocalVaultAdapter("none").loadSnapshot();
      expect(saved.documents).toEqual(before.documents);
      expect(saved.drafts).toEqual(before.drafts);
      expect(patches).toBe(boundary.startsWith("queue") ? 0 : 1);
      const queued = saved.sync_state!.queued_operations.filter(
        (operation) => operation.action === "update_maintenance",
      );
      expect(queued).toHaveLength(boundary === "queue-before" || boundary === "ack-after" ? 0 : 1);
      for (const operation of queued)
        expect(operation.server_context).toMatchObject({ account_id: "account-a" });
      expect(queued.some((operation) => operation.server_context?.account_id === "account-b")).toBe(
        false,
      );
    },
  );
  it("rejects mapped maintenance import after paused space creation switches accounts before any assigned page", async () => {
    const page = await dataClient.createDocument("local-space", {
      title: "Import account",
      maintenance_user_id: "local-user",
    });
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    setCurrentLaunchMode("server");
    api.setToken(token("account-a"));
    let reached!: () => void, release!: () => void;
    const started = new Promise<void>((r) => {
      reached = r;
    });
    const gate = new Promise<void>((r) => {
      release = r;
    });
    const writes: string[] = [];
    vi.spyOn(api, "fetch").mockImplementation(async (path, options) => {
      if (path === "/auth/me") return { id: "account-a" };
      if (path === "/spaces" && options?.method === "POST") {
        writes.push(path);
        reached();
        await gate;
        return { id: "created-space", name: "Imported", slug: "imported" };
      }
      writes.push(path);
      throw new Error("Unexpected cross-account import " + path);
    });
    const importing = dataClient.importVaultSnapshotToServer(snapshot, {
      maintenance_identity_map: { users: { "local-user": "server-person-a" }, groups: {} },
    });
    const rejected = expect(importing).rejects.toThrow(/account changed/);
    try {
      await started;
      api.setToken(token("account-b"));
    } finally {
      release();
    }
    await rejected;
    expect(writes).toEqual(["/spaces"]);
    expect(snapshot.documents.find((record) => record.id === page.id)?.maintenance_user_id).toBe(
      "local-user",
    );
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(
      snapshot.documents,
    );
  });
  it("rejects maintenance export after paused body load switches accounts without returning mixed provenance", async () => {
    const page = await dataClient.createDocument("local-space", {
      title: "Export account",
      maintenance_user_id: "local-user",
    });
    const before = await dataClient.exportLocalVaultSnapshot();
    const row = before.documents.find((record) => record.id === page.id)!;
    setCurrentLaunchMode("server");
    api.setToken(token("account-a"));
    vi.spyOn(api, "fetch").mockImplementation(async (path) => {
      if (path === "/auth/me") return { id: "account-a" };
      if (path.endsWith("/maintenance/targets"))
        return {
          users: [{ id: "local-user", name: "A person", active: true }],
          groups: [],
          groups_available: false,
        };
      throw new Error(path);
    });
    vi.spyOn(dataClient, "listSpaces").mockResolvedValue([
      { id: "local-space", name: "Space", slug: "space" },
    ]);
    vi.spyOn(dataClient, "listFolders").mockResolvedValue([]);
    vi.spyOn(dataClient, "listDocuments").mockResolvedValue([row]);
    vi.spyOn(dataClient, "listTemplates").mockResolvedValue([]);
    vi.spyOn(dataClient, "listWorkflowTemplates").mockResolvedValue([]);
    vi.spyOn(dataClient, "listPropertySchemas").mockResolvedValue([]);
    vi.spyOn(dataClient, "listTags").mockResolvedValue([]);
    vi.spyOn(dataClient, "getDocumentWorkflow").mockResolvedValue(null);
    vi.spyOn(dataClient, "listDocumentWorkflowEvents").mockResolvedValue([]);
    vi.spyOn(dataClient, "listDocumentProperties").mockResolvedValue([]);
    vi.spyOn(dataClient, "listDocumentTags").mockResolvedValue([]);
    let reached!: () => void, release!: () => void;
    const started = new Promise<void>((r) => {
      reached = r;
    });
    const gate = new Promise<void>((r) => {
      release = r;
    });
    vi.spyOn(dataClient, "getDocumentContent").mockImplementation(async () => {
      reached();
      await gate;
      return { id: page.id, title: "B body", content: "Wrong-account body" };
    });
    const exporting = dataClient.exportServerVaultSnapshot();
    const rejected = expect(exporting).rejects.toThrow(/account changed/);
    try {
      await started;
      api.setToken(token("account-b"));
    } finally {
      release();
    }
    await rejected;
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(
      before.documents,
    );
  });
  it("rejects a numeric acknowledgement timestamp and retains exact original maintenance recovery", async () => {
    const page = await dataClient.createDocument("local-space", {
      title: "Strict ACK",
      content: "Preserved body",
    });
    const before = await dataClient.exportLocalVaultSnapshot();
    const row = before.documents.find((record) => record.id === page.id)!;
    setCurrentLaunchMode("server");
    api.setToken(token("account-a"));
    let patchBody: unknown;
    vi.spyOn(api, "fetch").mockImplementation(async (path, options) => {
      if (path === "/auth/me") return { id: "account-a" };
      if (path === `/spaces/local-space/documents/${page.id}`) return row;
      if (path.endsWith("/maintenance")) {
        patchBody = options?.body;
        return {
          operation_id: (options!.body as { operation_id: string }).operation_id,
          maintenance_user_id: null,
          maintenance_group_id: null,
          page_updated_at: 2026,
          published_at: null,
          historical: false,
        };
      }
      throw new Error(path);
    });
    await expect(
      dataClient.setDocumentMaintenance(
        "local-space",
        page.id,
        { kind: "unassigned" },
        source(row),
      ),
    ).rejects.toThrow(/acknowledgement is unproven/);
    const pending = await dataClient.getPendingDocumentMaintenance("local-space", page.id);
    expect(pending).toMatchObject({
      operation_id: (patchBody as { operation_id: string }).operation_id,
      maintenance: { kind: "unassigned" },
      source: source(row),
    });
    const saved = await createLocalVaultAdapter("none").loadSnapshot();
    expect(saved.documents).toEqual(before.documents);
    expect(saved.drafts).toEqual(before.drafts);
    expect(
      saved.sync_state!.queued_operations.filter(
        (operation) => operation.action === "update_maintenance",
      ),
    ).toHaveLength(1);
  });
});
