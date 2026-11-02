import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LocalVaultSnapshot, VaultQueuedSyncOperation } from "./localVault";
import type { ProviderRemoteImportApplyPlan } from "./providerAdapters";
import { createLocalMoveSubmission } from "@/components/layout/LocalReviewedMoveDialog";
import type { LocalReviewedMoveDecisions, LocalMoveMetadataChoice } from "./localReviewedMove";
let client: typeof import("./dataClient");
let vault: typeof import("./localVault");
let api: typeof import("./api").api;
let launch: typeof import("./launchMode");
let providers: typeof import("./storageProviders");
let sessions: typeof import("./providerSession");
let remote: typeof import("./providerAdapters");
const decisions = (): LocalReviewedMoveDecisions => ({ properties: [], workflows: [], tags: [], relations: [], identifiers: [], maintenance: [], detach_hierarchy: [], preserve_previous_draft_placement: true });
const intent = { source_space_id: "source", root: { kind: "document" as const, id: "page" }, destination: { space_id: "destination", folder_id: null, parent_id: null } };
const input = () => ({ operation_id: "12345678-1234-1234-1234-123456789abc", moved_at: "2026-10-09T12:00:00Z", decisions: decisions() });
function fixture(): LocalVaultSnapshot {
  return {
    schema_version: 1,
    spaces: ["source", "destination"].map(id => ({ id, name: id, created_at: "2026-10-09T10:00:00Z", updated_at: "2026-10-09T10:00:00Z" })), folders: [],
    documents: [{ id: "page", space_id: "source", title: "Page", content: "exact A starlane://asset/owned", created_by: "Local User", updated_at: "2026-10-09T10:00:00Z", published_at: "2026-10-09T10:00:00Z" }, { id: "child", space_id: "source", parent_id: "page", title: "Child", content: "child exact", updated_at: "2026-10-09T10:00:00Z", published_at: "2026-10-09T10:00:00Z" }],
    drafts: [{ id: "draft", document_id: "page", space_id: "source", owner_id: "local-user", title: "Draft", content: "draft exact", base_updated_at: "2026-10-09T10:00:00Z", created_at: "2026-10-09T10:00:00Z", updated_at: "2026-10-09T10:00:00Z" }],
    assets: [{ id: "owned", document_id: "page", space_id: "source", name: "owned.png", size: 4, url: "data:image/png;base64,Ynl0ZQ==", source: "browser", created_at: "2026-10-09T10:00:00Z" }], templates: [],
    sync_state: { state: "idle", updated_at: "2026-10-09T10:00:00Z", queued_operations: [] },
  };
}

describe("actual browser Google reviewed move producer and explicit remote hold", () => {
  const storage = new Map<string, string>();
  let fetchMock: ReturnType<typeof vi.fn<typeof globalThis.fetch>>;
  let targetCurrent: boolean;
  const lockTails = new Map<string, Promise<void>>();
  let held: { reached: () => void; resume: Promise<void>; storageOnly: boolean } | undefined;
  const holdNext = (storageOnly = false) => { let reached!: () => void, release!: () => void; const arrived = new Promise<void>(resolve => { reached = resolve; }); const resume = new Promise<void>(resolve => { release = resolve; }); held = { reached, resume, storageOnly }; return { arrived, release }; };
  const disk = () => [...storage.entries()];
  const canonical = () => vault.createLocalVaultAdapter("google-drive").loadSnapshot();
  beforeEach(async () => {
    storage.clear(); held = undefined; targetCurrent = true; lockTails.clear();
    const ls = { get length() { return storage.size; }, key: (n: number) => [...storage.keys()][n] ?? null, getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => { storage.set(key, value); }, removeItem: (key: string) => { storage.delete(key); } };
    vi.stubGlobal("window", { addEventListener: vi.fn(), localStorage: ls, navigator: { locks: { request: async (key: string, callback: () => unknown) => { const prior = lockTails.get(key) ?? Promise.resolve(); let release!: () => void; const tail = new Promise<void>(resolve => { release = resolve; }); lockTails.set(key, tail); await prior; try { const h = held && (!held.storageOnly || key.startsWith("starlane-vault-")) ? held : undefined; if (h) held = undefined; if (h) { h.reached(); await h.resume; } return await callback(); } finally { release(); if (lockTails.get(key) === tail) lockTails.delete(key); } } } } }); vi.stubGlobal("localStorage", ls);
    vi.resetModules();
    launch = await import("./launchMode"); providers = await import("./storageProviders");
    launch.setCurrentLaunchMode("local"); providers.setStorageMode("local"); providers.setSyncProvider("google-drive");
    // Import the actual application graph only after the persisted workspace is selected.
    client = await import("./dataClient"); vault = await import("./localVault");
    api = (await import("./api")).api; sessions = await import("./providerSession"); remote = await import("./providerAdapters");
    api.setToken(null); api.setRefreshToken(null);
    sessions.writeProviderSession("google-drive", sessions.createGoogleDriveManualProviderSession({ access_token: "fake-original-google-bearer", expires_in: 3600, scope: "drive.file", provider_root_id: "original-root" }));
    fetchMock = vi.fn<typeof globalThis.fetch>().mockImplementation(async url => new Response(JSON.stringify(String(url).includes("/about?") ? { user: { me: true, permissionId: "permission-A" } } : { id: "original-root", mimeType: "application/vnd.google-apps.folder", trashed: false, capabilities: { canAddChildren: true, canListChildren: true } }), { status: 200 })); vi.stubGlobal("fetch", fetchMock);
    await client.__resetLocalDataForTests(); await client.dataClient.importLocalVaultSnapshot(fixture());
  });
  afterEach(() => { api.setToken(null); api.setRefreshToken(null); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
  const review = () => client.dataClient.reviewGoogleDriveMove(intent, () => targetCurrent);
  it("commits relocation, schema2 full history and one exact envelope together; preserves raw incumbent queue and asset ownership on fresh reopen", async () => {
    const original = await canonical(); original.sync_state!.queued_operations = [{ id: "incumbent", entity_type: "future", entity_id: "unknown", action: "later", queued_at: "original", payload: { exact: [null, 2] }, future_raw: { retained: true } } as VaultQueuedSyncOperation];
    await client.dataClient.importLocalVaultSnapshot(original); const session = await review(); const result = await session.commit(input()); const reopened = await canonical();
    expect(result.provider_status).toBe("pending_provider"); expect(result.record).toMatchObject({ schema_version: 2, provider: "google-drive", origin: { account_permission_id: "permission-A", root_id: "original-root" } });
    expect(reopened.reviewed_move_records).toEqual([result.record]); expect(reopened.documents.map(row => row.space_id)).toEqual(["destination", "destination"]); expect(reopened.documents[0].content).toBe(original.documents[0].content); expect(reopened.drafts![0]).toEqual({ ...original.drafts![0], space_id: "destination" });
    expect(reopened.sync_state!.queued_operations[0]).toEqual(original.sync_state!.queued_operations[0]); expect(reopened.sync_state!.queued_operations).toHaveLength(2); const queued = reopened.sync_state!.queued_operations[1];
    expect(queued).toMatchObject({ id: input().operation_id, entity_type: "reviewed_move", entity_id: "page", action: "commit", replay_provider: "google-drive", payload: { kind: "reviewed_move", record: result.record, portable_asset_requirements: original.assets } });
    expect(JSON.stringify(queued)).not.toMatch(/fake-original-google-bearer/); expect(reopened.assets).toEqual([{ ...original.assets[0], space_id: "destination" }]); expect(await client.dataClient.listReusableAssets("destination")).toEqual([{ ...original.assets[0], space_id: "destination" }]); expect(await client.dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).toEqual([result.record]);
  });
  it.each(["map", "create", "preserve", "remove"] as const)("consumes complete %s property/tag/workflow decisions with exact bodies and raw inactive evidence", async kind => {
    const snapshot = fixture(); snapshot.property_schemas = [{ id: "property", space_id: "source", name: "Property", property_type: "text", sort_order: 0 }, { id: "target-property", space_id: "destination", name: "Target property", property_type: "text", sort_order: 0 }]; snapshot.property_values = [{ id: "value", document_id: "page", property_id: "property", value: "exact-value" }]; snapshot.tags = [{ id: "tag", space_id: "source", name: "Tag", color: "blue" }, { id: "target-tag", space_id: "destination", name: "Target tag", color: "blue" }]; snapshot.doc_tags = [{ document_id: "page", tag_id: "tag" }];
    snapshot.workflow_templates = ["source", "destination"].map(space_id => ({ id: `${space_id}-workflow`, space_id, name: "Workflow", stages: [{ id: "start", name: "Start", sort_order: 0 }, { id: "done", name: "Done", sort_order: 1 }], approvers: [], created_at: "old", updated_at: "old" })); snapshot.document_workflows = [{ id: "workflow", document_id: "page", workflow_template_id: "source-workflow", current_stage_id: "start", status: "active", created_at: "old", updated_at: "old" }]; snapshot.document_workflow_events = [{ id: "event", document_workflow_id: "workflow", document_id: "page", to_stage_id: "start", action: "enable", created_at: "old" }];
    await client.dataClient.importLocalVaultSnapshot(snapshot); const session = await review(); expect(() => session.validate(decisions())).toThrow();
    const choice = (id: string): LocalMoveMetadataChoice => kind === "map" || kind === "create" ? { kind, destination_id: id } : { kind };
    const chosen = decisions(); chosen.properties = [{ source_id: "property", choice: choice(kind === "map" ? "target-property" : "new-property") }]; chosen.tags = [{ source_id: "tag", choice: choice(kind === "map" ? "target-tag" : "new-tag") }]; chosen.workflows = [{ source_id: "source-workflow", choice: choice(kind === "map" ? "destination-workflow" : "new-workflow") }];
    session.validate(chosen); const result = await session.commit({ ...input(), decisions: chosen }); expect(result.record.decisions).toEqual(chosen); expect(result.record.before.document_workflow_events).toEqual(snapshot.document_workflow_events); expect((await canonical()).documents[0].content).toBe(snapshot.documents[0].content);
    if (kind === "preserve" || kind === "remove") expect(result.record[kind === "preserve" ? "preserved" : "removed"].some(row => row.collection === "property_values")).toBe(true);
  });
  it("cancels before dispatch with no storage change or queued request", async () => { const session = await review(); const before = disk(); const submission = createLocalMoveSubmission(session, () => true); submission.cancel(); expect(disk()).toEqual(before); expect((await canonical()).sync_state!.queued_operations).toEqual([]); });
  it.each(["root", "token", "provider", "mode", "selection"])("waits in actual storage lock then rejects %s drift without relocation or queue", async kind => {
    const session = await review(); const before = disk(); const hold = holdNext(); const saving = session.commit(input()); const rejected = expect(saving).rejects.toThrow(); await hold.arrived;
    if (kind === "root" || kind === "token") { const prior = sessions.readProviderSession("google-drive"); sessions.writeProviderSession("google-drive", { ...prior, auth: { ...prior.auth!, ...(kind === "root" ? { provider_root_id: "foreign" } : { access_token: "foreign-token" }) } }); }
    if (kind === "provider") providers.setSyncProvider("none"); if (kind === "mode") launch.setCurrentLaunchMode("server"); if (kind === "selection") targetCurrent = false;
    hold.release(); await rejected; const oldVault = storage.get("starlane_local_vault_v1_google-drive"); expect(JSON.parse(oldVault!).documents[0].space_id).toBe("source"); expect(JSON.parse(oldVault!).sync_state.queued_operations).toEqual([]); expect(before.find(([key]) => key === "starlane_local_vault_v1_google-drive")![1]).toBe(oldVault);
  });
  it("requires reload before move reads or writes when the incumbent adapter belongs to another partition", async () => {
    vi.resetModules(); providers = await import("./storageProviders"); providers.setSyncProvider("none");
    launch = await import("./launchMode"); launch.setCurrentLaunchMode("local");
    client = await import("./dataClient"); vault = await import("./localVault"); api = (await import("./api")).api;
    sessions = await import("./providerSession"); remote = await import("./providerAdapters");
    providers.setSyncProvider("google-drive");
    const before = disk();
    await expect(review()).rejects.toThrow("Reload the selected Google workspace");
    expect(disk()).toEqual(before);
    expect(JSON.parse(storage.get("starlane_local_vault_v1_google-drive")!).reviewed_move_records).toBeUndefined();
    expect(storage.has("starlane_local_vault_v1")).toBe(false);
  });
  it("rejects native Google before enrollment HTTP or native commands", async () => { const invoke = vi.fn(); (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = { invoke }; fetchMock.mockClear(); await expect(review()).rejects.toThrow("Native Google"); expect(fetchMock).not.toHaveBeenCalled(); expect(invoke).not.toHaveBeenCalled(); });
  it.each(["clear", "reset", "import", "alter queue", "alter history"])("retains exact original pending footprint when %s would discard it", async action => {
    await (await review()).commit(input()); const before = disk(); const adapter = vault.createLocalVaultAdapter("google-drive");
    const invoke = async () => { if (action === "clear") return adapter.clearQueuedSyncOperations(); if (action === "reset") return adapter.reset(); if (action === "import") return client.dataClient.importLocalVaultSnapshot(fixture()); return adapter.commitSnapshotMutation(snapshot => { if (action === "alter queue") snapshot.sync_state!.queued_operations[0].payload = { changed: true }; else snapshot.reviewed_move_records = []; return { snapshot }; }); };
    await expect(invoke()).rejects.toThrow(/Pending Google|Reviewed Google/); expect(disk()).toEqual(before);
  });
  it("actual source-free import retains selected Google pending history, RAM and the foreign standalone vault", async () => {
    await (await review()).commit(input());
    const googleBefore = await canonical();
    // Reproduce a genuinely incumbent standalone module graph, then select Google.
    vi.resetModules(); providers = await import("./storageProviders"); providers.setSyncProvider("none");
    launch = await import("./launchMode"); launch.setCurrentLaunchMode("local");
    client = await import("./dataClient"); vault = await import("./localVault"); api = (await import("./api")).api;
    sessions = await import("./providerSession"); remote = await import("./providerAdapters");
    await client.dataClient.importLocalVaultSnapshot(fixture());
    const ramBefore = await client.dataClient.getDocumentContent("source", "page");
    providers.setSyncProvider("google-drive");
    const noneBefore = await vault.createLocalVaultAdapter("none").loadSnapshot();
    const rawBefore = disk();
    const incoming = fixture(); incoming.documents[0].content = "foreign imported body";
    expect(incoming.reviewed_move_records).toBeUndefined();
    expect(incoming.documents.every(row => row.published_source === undefined)).toBe(true);
    await expect(client.dataClient.importLocalVaultSnapshot(incoming)).rejects.toThrow(/Pending Google|Reviewed Google/);
    expect(disk()).toEqual(rawBefore);
    expect(await canonical()).toMatchObject({ documents: googleBefore.documents, reviewed_move_records: googleBefore.reviewed_move_records, sync_state: { queued_operations: googleBefore.sync_state!.queued_operations } });
    expect(await client.dataClient.getDocumentContent("source", "page")).toEqual(ramBefore);
    expect(await vault.createLocalVaultAdapter("none").loadSnapshot()).toMatchObject({ documents: noneBefore.documents, spaces: noneBefore.spaces });
  });
  it.each(["push", "pull", "import"] as const)("refuses selected Google pending %s with an incumbent standalone adapter before state or HTTP", async action => {
    await (await review()).commit(input());
    vi.resetModules(); providers = await import("./storageProviders"); providers.setSyncProvider("none");
    launch = await import("./launchMode"); launch.setCurrentLaunchMode("local");
    client = await import("./dataClient"); vault = await import("./localVault"); api = (await import("./api")).api;
    sessions = await import("./providerSession"); remote = await import("./providerAdapters");
    await client.dataClient.importLocalVaultSnapshot(fixture());
    providers.setSyncProvider("google-drive");
    const before = disk(); fetchMock.mockClear();
    if (action === "import") {
      const adapter = remote.createProviderAdapter("google-drive"); const status = await adapter.getStatus();
      const envelope = remote.buildProviderRemoteMutationEnvelope(remote.buildProviderRemoteMutationPlan("google-drive", adapter.label, "pull", status.remote_contract));
      const manifest = remote.buildProviderRemoteSyncManifest(envelope, { operations: [] });
      await expect(client.dataClient.applyProviderRemoteImportManifest(manifest)).rejects.toThrow("pending provider");
    } else await expect(client.dataClient.syncWithProvider(action)).rejects.toThrow("pending provider");
    expect(fetchMock).not.toHaveBeenCalled(); expect(disk()).toEqual(before);
    expect((await canonical()).sync_state!.queued_operations).toHaveLength(1);
    expect(await client.dataClient.getSyncState()).toBe("idle");
  });
  it.each(["push", "pull"] as const)("holds manual %s before sync state or HTTP and retains full request", async direction => { await (await review()).commit(input()); const before = disk(); fetchMock.mockClear(); await expect(client.dataClient.syncWithProvider(direction)).rejects.toThrow(/pending provider|not ready/); expect(fetchMock).not.toHaveBeenCalled(); expect(disk()).toEqual(before); });
  it("holds whole received dedicated move before ordinary per-item apply and before raw provider transport", async () => {
    const result = await (await review()).commit(input()); const operations = (await canonical()).sync_state!.queued_operations; const adapter = remote.createProviderAdapter("google-drive"); const status = await adapter.getStatus(); const envelope = remote.buildProviderRemoteMutationEnvelope(remote.buildProviderRemoteMutationPlan("google-drive", adapter.label, "push", status.remote_contract)); const manifest = remote.buildProviderRemoteSyncManifest(envelope, { schema_version: 1, operations }); const before = disk(); fetchMock.mockClear();
    expect(() => remote.buildProviderRemoteImportDryRun(manifest)).toThrow("pending provider"); await expect(client.dataClient.applyProviderRemoteImportManifest(manifest)).rejects.toThrow("pending provider"); await expect(remote.executeProviderRemoteMutationEnvelopeWithTransport(envelope, undefined, { providerSyncPayload: { operations }, apiTransport: { fetch: fetchMock } })).rejects.toThrow("pending provider"); expect(fetchMock).not.toHaveBeenCalled(); expect(disk()).toEqual(before); expect(result.record.provider).toBe("google-drive");
  });
  it("same operation historical ACK requires both exact pinned record and original envelope, while later canonical B stays untouched", async () => {
    const session = await review(); const result = await session.commit(input()); const adapter = vault.createLocalVaultAdapter("google-drive"); await adapter.commitSnapshotMutation(snapshot => { snapshot.spaces.push({ id: "third", name: "Third", created_at: "old", updated_at: "old" }); snapshot.documents[0].space_id = "third"; snapshot.documents[0].content = "canonical B"; return { snapshot }; }); const before = disk(); const retry = await session.commit(input()); expect(retry.record).toEqual(result.record); expect((await client.dataClient.getDocumentContent("third", "page")).content).toBe("canonical B"); expect(disk()).toEqual(before);
  });
  it("lost ACK reconciles exact operation without new relocation or queue", async () => {
    const probe = vault.createLocalVaultAdapter("google-drive"); const prototype = Object.getPrototypeOf(probe) as typeof probe; const original = prototype.commitSnapshotMutation;
    const session = await review(); const spy = vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function(this: typeof probe, ...args: Parameters<typeof probe.commitSnapshotMutation>) { await original.apply(this, args); throw new Error("lost ACK"); }); await expect(session.commit(input())).rejects.toThrow("lost ACK"); spy.mockRestore();
    const before = disk(); const result = await session.commit(input()); expect(result.provider_status).toBe("pending_provider"); expect((await canonical()).sync_state!.queued_operations).toHaveLength(1); expect(disk()).toEqual(before);

  });
  it("retains RAM B created during the actual canonical move wait, without changing frozen queued A", async () => {
    const session = await review(); const hold = holdNext(true); const saving = session.commit(input()); await hold.arrived;
    const pendingB = client.dataClient.updateDocumentContent("source", "page", "newer RAM B"); await vi.waitFor(async () => expect((await client.dataClient.getDocumentContent("source", "page")).content).toBe("newer RAM B"));
    hold.release(); const result = await saving; await pendingB;
    expect(result.newer_local_changes_retained).toBe(true); expect((await canonical()).documents[0].content).toBe("newer RAM B");
    expect(result.record.after.documents).toContainEqual(expect.objectContaining({ id: "page", content: fixture().documents[0].content }));
    expect(((await canonical()).sync_state!.queued_operations[0].payload as { record: unknown }).record).toEqual(result.record);
  });
  it.each(["before", "after", "preserved", "removed", "unknown", "envelope"])("historical retry refuses changed %s while keeping canonical evidence unchanged", async field => {
    const session = await review(); await session.commit(input()); const snapshot = await canonical();
    if (field === "envelope") (snapshot.sync_state!.queued_operations[0].payload as unknown as Record<string, unknown>).future_unknown = ["tampered"];
    else if (field === "unknown") (snapshot.reviewed_move_records![0] as unknown as Record<string, unknown>).future_unknown = ["tampered"];
    else if (field === "before" || field === "after") (snapshot.reviewed_move_records![0][field].documents![0] as Record<string, unknown>).content = "tampered";
    else if (field === "preserved" || field === "removed") snapshot.reviewed_move_records![0][field].push({ collection: "documents", records: [{ id: "tampered" }] });
    storage.set("starlane_local_vault_v1_google-drive", JSON.stringify(snapshot)); const before = disk(); await expect(session.commit(input())).rejects.toThrow("evidence does not match"); expect(disk()).toEqual(before);
  });
  it("whole-plan executor holds a skipped dedicated move before an earlier ordinary selected mutation", async () => {
    await (await review()).commit(input()); const operations = (await canonical()).sync_state!.queued_operations;
    const merge = vi.fn(); const { executeProviderRemoteImportApplyPlan } = await import("./providerAdapters");
    const plan: ProviderRemoteImportApplyPlan = { schema_version: 1, provider: "google-drive", status: "blocked", apply_ready: false, selected_operation_count: 1, skipped_operation_count: 1, blockers: [], operations: [{ operation_id: "ordinary", entity_type: "document", entity_id: "page", action: "update", source_status: "would_apply", apply_status: "selected", message: "ordinary", payload: {} }, { operation_id: "move", entity_type: "reviewed_move", entity_id: "page", action: "commit", source_status: "unsupported", apply_status: "skipped_unsupported", message: "unsupported", payload: operations[0].payload }] };
    await expect(executeProviderRemoteImportApplyPlan(plan, { allowLocalMutation: true, mergeExecutor: { applyOperation: merge } })).rejects.toThrow("pending provider"); expect(merge).not.toHaveBeenCalled();
  });

  it("does not enable a Google move on IDB alone without the domain Web Lock", async () => {
    (window as unknown as { navigator: unknown }).navigator = {};
    await expect(review()).rejects.toThrow("Web Locks"); expect((await canonical()).reviewed_move_records).toBeUndefined();
  });

  it("manual sync waits behind a move's shared domain, then holds the exact committed envelope before state/HTTP", async () => {
    const session = await review(); const hold = holdNext(true); const moving = session.commit(input()); await hold.arrived;
    const syncing = client.dataClient.syncWithProvider("pull"); const denied = expect(syncing).rejects.toThrow(/pending provider|not ready/);
    hold.release(); const result = await moving; const before = disk(); fetchMock.mockClear(); await denied;
    expect(fetchMock).not.toHaveBeenCalled(); expect(disk()).toEqual(before); expect((await canonical()).reviewed_move_records).toEqual([result.record]);
  });
  it("whole manifest import waits behind a move's shared domain, then refuses state and partial application", async () => {
    const session = await review(); const adapter = remote.createProviderAdapter("google-drive"); const status = await adapter.getStatus(); const envelope = remote.buildProviderRemoteMutationEnvelope(remote.buildProviderRemoteMutationPlan("google-drive", adapter.label, "pull", status.remote_contract)); const manifest = remote.buildProviderRemoteSyncManifest(envelope, { operations: [] });
    const hold = holdNext(true); const moving = session.commit(input()); await hold.arrived;
    const importing = client.dataClient.applyProviderRemoteImportManifest(manifest); const denied = expect(importing).rejects.toThrow("pending provider");
    hold.release(); await moving; const before = disk(); await denied; expect(disk()).toEqual(before);
  });

  it("resolves actual incoming/outgoing edges against final spaces and retains inaccessible-target history without mutation", async () => {
    const source = fixture(); source.documents.push({ ...source.documents[0], id: "outside", title: "Outside", content: "outside" }, { ...source.documents[0], id: "target", title: "Target", space_id: "destination", content: "target" });
    source.property_schemas = [{ id: "relation", space_id: "source", name: "Related", property_type: "relation", sort_order: 0 }, { id: "destination-relation", space_id: "destination", name: "Related destination", property_type: "relation", sort_order: 0 }];
    source.property_values = [{ id: "out", document_id: "page", property_id: "relation", value: "outside" }, { id: "in", document_id: "outside", property_id: "relation", value: "page" }]; await client.dataClient.importLocalVaultSnapshot(source);
    const session = await review(); const chosen = decisions(); chosen.properties = [{ source_id: "relation", choice: { kind: "map", destination_id: "destination-relation" } }]; chosen.relations = [{ value_id: "out", choice: { kind: "map", document_id: "target" } }, { value_id: "in", choice: { kind: "preserve" } }];
    expect(() => session.validate({ ...chosen, relations: [] })).toThrow(); expect(() => session.validate({ ...chosen, relations: [{ value_id: "out", choice: { kind: "map", document_id: "outside" } }, chosen.relations[1]] })).toThrow();
    const result = await session.commit({ ...input(), decisions: chosen }); const saved = await canonical(); expect(saved.property_values).toEqual([{ ...source.property_values[0], property_id: "destination-relation", value: "target" }]); expect(result.record.preserved).toContainEqual({ collection: "property_values", records: [source.property_values[1]] });
    expect(await client.dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).toEqual([result.record]);
    await vault.createLocalVaultAdapter("google-drive").commitSnapshotMutation(snapshot => { snapshot.documents.find(row => row.id === "target")!.archived_at = "later"; return { snapshot }; }); const before = disk(); await expect(client.dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).rejects.toThrow("unavailable"); expect(disk()).toEqual(before);
  });

});
