import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { dataClient, __resetLocalDataForTests } from "./dataClient";
import { createLocalVaultAdapter, type LocalVaultSnapshot } from "./localVault";
import { api } from "./api";
import { setCurrentLaunchMode } from "./launchMode";
import { setStorageMode, setSyncProvider } from "./storageProviders";
import type { LocalReviewedMoveDecisions } from "./localReviewedMove";

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

describe("actual standalone reviewed move writer and recovery persistence", () => {
  const storage = new Map<string, string>();
  let held: { reached: () => void; release: Promise<void> } | undefined;
  let failWrite = false;
  const holdNext = () => {
    let reached!: () => void, release!: () => void;
    const arrived = new Promise<void>(resolve => { reached = resolve; });
    const released = new Promise<void>(resolve => { release = resolve; });
    held = { reached, release: released };
    return { arrived, release };
  };
  beforeEach(async () => {
    storage.clear(); held = undefined; failWrite = false;
    const localStorageMock = { get length() { return storage.size; }, key: (index: number) => [...storage.keys()][index] ?? null, getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => { if (failWrite && key.startsWith("starlane")) throw new Error("quota"); storage.set(key, value); }, removeItem: (key: string) => { storage.delete(key); } };
    vi.stubGlobal("window", { localStorage: localStorageMock, navigator: { locks: { request: async (_key: string, callback: () => unknown) => { const pending = held; held = undefined; if (pending) { pending.reached(); await pending.release; } return callback(); } } } });
    vi.stubGlobal("localStorage", localStorageMock);
    setCurrentLaunchMode("local"); setStorageMode("local"); setSyncProvider("none"); api.setToken(null); api.setRefreshToken(null);
    await __resetLocalDataForTests();
    await dataClient.importLocalVaultSnapshot(fixture());
  });
  afterEach(() => { failWrite = false; api.setToken(null); api.setRefreshToken(null); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

  it("commits actual subtree plus raw recovery atomically, exposes destination-owned reusable asset, and survives a fresh adapter reopen", async () => {
    const before = await dataClient.exportLocalVaultSnapshot();
    const session = await dataClient.reviewLocalMove(intent, () => true);
    expect(session.preview.commit_available).toBe(false);
    const result = await session.commit(input());
    expect(result.canonical_committed).toBe(true);
    const reopened = await createLocalVaultAdapter("none").loadSnapshot();
    expect(reopened.documents.map(row => row.space_id)).toEqual(["destination", "destination"]);
    expect(reopened.documents[0].content).toBe(before.documents[0].content);
    expect(reopened.drafts![0]).toEqual({ ...before.drafts![0], space_id: "destination" });
    expect(reopened.sync_state!.queued_operations).toEqual(before.sync_state!.queued_operations);
    expect(reopened.reviewed_move_records).toEqual([result.record]);
    expect(await dataClient.listReusableAssets("destination")).toEqual([{ ...before.assets[0], space_id: "destination" }]);
    expect(await dataClient.listReusableAssets("source")).toEqual([]);
  });
  it("preserves raw future recovery fields through explicit import, adapter reopen, ordinary edit, and export", async () => {
    const session = await dataClient.reviewLocalMove(intent, () => true); await session.commit(input());
    const exported = await dataClient.exportLocalVaultSnapshot();
    const record = { ...exported.reviewed_move_records![0], future_evidence: { numeric: { "2": "exact", "10": "raw" }, nested: [null, "bytes"] } };
    exported.reviewed_move_records = [record];
    await dataClient.importLocalVaultSnapshot(exported);
    await dataClient.updateDocumentContent("destination", "page", "later content B");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).reviewed_move_records).toEqual([record]);
    expect((await dataClient.exportLocalVaultSnapshot()).reviewed_move_records).toEqual([record]);
  });
  it("rejects an original selection closed while waiting inside the actual storage lock, with no relocation or evidence", async () => {
    let selected = true; const session = await dataClient.reviewLocalMove(intent, () => selected); const before = await dataClient.exportLocalVaultSnapshot();
    const lock = holdNext(); const pending = session.commit(input()); const rejected = expect(pending).rejects.toThrow("selection changed");
    await lock.arrived; selected = false; lock.release(); await rejected;
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(before.documents);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).reviewed_move_records).toBeUndefined();
  });
  it.each(["provider", "disk token", "launch mode"])("rejects %s drift before the actual canonical callback", async drift => {
    const session = await dataClient.reviewLocalMove(intent, () => true); const before = await dataClient.exportLocalVaultSnapshot();
    const lock = holdNext(); const pending = session.commit(input()); const rejected = expect(pending).rejects.toThrow("workspace or selection");
    await lock.arrived;
    if (drift === "provider") setSyncProvider("google-drive"); else if (drift === "disk token") localStorage.setItem("access_token", "foreign"); else setCurrentLaunchMode("server");
    lock.release(); await rejected;
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(before.documents);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).reviewed_move_records).toBeUndefined();
  });
  it("retains later B body fields while publishing canonical relocation after the actual delayed commit", async () => {
    const session = await dataClient.reviewLocalMove(intent, () => true); const lock = holdNext(); const pendingMove = session.commit(input());
    await lock.arrived;
    const pendingB = dataClient.updateDocumentContent("source", "page", "later B remains open");
    expect((await dataClient.getDocumentContent("source", "page")).content).toBe("later B remains open");
    lock.release(); const result = await pendingMove; await pendingB;
    expect(result.newer_local_changes_retained).toBe(true);
    const reopened = await createLocalVaultAdapter("none").loadSnapshot();
    expect(reopened.documents.find(row => row.id === "page")!.space_id).toBe("destination");
    expect(reopened.documents.find(row => row.id === "page")!.content).toBe("later B remains open");
    expect(result.record.before.documents).toContainEqual(expect.objectContaining({ id: "page", content: "exact A starlane://asset/owned" }));
    expect(result.record.after.documents).toContainEqual(expect.objectContaining({ id: "page", content: "exact A starlane://asset/owned", space_id: "destination" }));
  });
  it("refuses native runtime adoption by an already initialized browser adapter", async () => {
    const invoke = vi.fn(() => { throw new Error("unexpected native command"); });
    (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = { invoke };
    await expect(dataClient.reviewLocalMove(intent, () => true)).rejects.toThrow("Original adapter changed");
    expect(invoke).not.toHaveBeenCalled();
  });
  it("rechecks changed canonical source rather than the accepted cached preview", async () => {
    const session = await dataClient.reviewLocalMove(intent, () => true);
    await createLocalVaultAdapter("none").commitSnapshotMutation(snapshot => { snapshot.documents[0].content = "different window B"; return { snapshot }; });
    await expect(session.commit(input())).rejects.toThrow("source changed");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents[0].content).toBe("different window B");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).reviewed_move_records).toBeUndefined();
  });
  it("does not publish relocation or recovery when the canonical storage write fails", async () => {
    const session = await dataClient.reviewLocalMove(intent, () => true); const before = await dataClient.exportLocalVaultSnapshot();
    failWrite = true; await expect(session.commit(input())).rejects.toThrow("quota"); failWrite = false;
    expect((await createLocalVaultAdapter("none").loadSnapshot()).documents).toEqual(before.documents);
    expect((await dataClient.getDocument("source", "page")).space_id).toBe("source");
    expect((await dataClient.exportLocalVaultSnapshot()).reviewed_move_records).toBeUndefined();
  });
  it("refuses localStorage movement without Web Locks instead of claiming cross-tab atomicity", async () => {
    delete (window.navigator as unknown as { locks?: unknown }).locks;
    await expect(dataClient.reviewLocalMove(intent, () => true)).rejects.toThrow("transactional local storage");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).reviewed_move_records).toBeUndefined();
  });
  it("fails closed on a malformed retained recovery record instead of adopting an empty vault", async () => {
    const keys = [...storage.keys()].filter(key => key.includes("vault"));
    expect(keys).toHaveLength(1);
    const corrupt = { ...fixture(), reviewed_move_records: null };
    storage.set(keys[0], JSON.stringify(corrupt));
    await expect(createLocalVaultAdapter("none").loadSnapshot()).rejects.toThrow("recovery records are malformed");
    expect(JSON.parse(storage.get(keys[0])!).reviewed_move_records).toBeNull();
  });
  it("reads persisted recovery after reload without writes, then refuses a newly unavailable participant", async () => {
    const session = await dataClient.reviewLocalMove(intent, () => true); const result = await session.commit(input());
    const beforeRead = [...storage.entries()];
    expect(await dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).toEqual([result.record]);
    expect([...storage.entries()]).toEqual(beforeRead);
    await createLocalVaultAdapter("none").commitSnapshotMutation(snapshot => { snapshot.documents.find(row => row.id === "child")!.archived_at = "2026-10-09T13:00:00Z"; return { snapshot }; });
    await expect(dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).rejects.toThrow("evidence is unavailable");
    expect((await createLocalVaultAdapter("none").loadSnapshot()).reviewed_move_records).toEqual([result.record]);
  });
  it("does not return persisted evidence into a provider switched while its read waited", async () => {
    const session = await dataClient.reviewLocalMove(intent, () => true); await session.commit(input());
    const pending = dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true); const rejected = expect(pending).rejects.toThrow("workspace or selection");
    setSyncProvider("google-drive"); await rejected;
  });
  it("uses the captured source for readiness and projects readable final-space relation targets in both directions", async () => {
    const source = fixture();
    source.documents.push({ id: "outside", space_id: "source", title: "Outside", content: "outside", updated_at: "2026-10-09T10:00:00Z", published_at: "2026-10-09T10:00:00Z" }, { id: "target", space_id: "destination", title: "Target", content: "target", updated_at: "2026-10-09T10:00:00Z", published_at: "2026-10-09T10:00:00Z" }, { id: "hidden", space_id: "destination", title: "Private foreign title", content: "hidden", updated_at: "2026-10-09T10:00:00Z" });
    source.drafts!.push({ ...source.drafts![0], id: "foreign", document_id: "hidden", space_id: "destination", owner_id: "foreign", title: "Private foreign title" });
    source.property_schemas = [{ id: "relation", space_id: "source", name: "Related", property_type: "relation", sort_order: 0 }, { id: "target-relation", space_id: "destination", name: "Related destination", property_type: "relation", sort_order: 0 }];
    source.property_values = [{ id: "out", document_id: "page", property_id: "relation", value: "outside" }, { id: "in", document_id: "outside", property_id: "relation", value: "page" }];
    await dataClient.importLocalVaultSnapshot(source);
    const session = await dataClient.reviewLocalMove(intent, () => true); const before = [...storage.entries()];
    expect(session.relationTargets("out").map(row => row.id).sort()).toEqual(["child", "page", "target"]);
    expect(session.relationTargets("in")).toEqual([{ id: "outside", title: "Outside" }]);
    expect(JSON.stringify(session.relationTargets("out"))).not.toContain("Private foreign title");
    expect(() => session.validate(decisions())).toThrow("every metadata");
    const chosen = decisions(); chosen.properties = [{ source_id: "relation", choice: { kind: "map", destination_id: "target-relation" } }]; chosen.relations = [{ value_id: "out", choice: { kind: "map", document_id: "target" } }, { value_id: "in", choice: { kind: "preserve" } }];
    expect(() => session.validate(chosen)).not.toThrow(); expect([...storage.entries()]).toEqual(before);
  });
  it.each([
    ["incoming", "map", "archive"], ["incoming", "map", "private"],
    ["outgoing", "map", "archive"], ["outgoing", "map", "private"],
    ["outgoing", "preserve", "archive"], ["outgoing", "preserve", "private"],
    ["outgoing", "remove", "archive"], ["outgoing", "remove", "private"],
  ] as const)("rechecks %s %s relation endpoint after %s without changing durable history", async (direction, disposition, unavailable) => {
    const source = fixture();
    source.documents.push(
      { ...source.documents[0], id: "outside", parent_id: undefined, title: "Outside", content: "outside" },
      { ...source.documents[0], id: "target", space_id: direction === "incoming" ? "source" : "destination", parent_id: undefined, title: "Target", content: "target" },
    );
    source.property_schemas = [{ id: "relation", space_id: "source", name: "Relation", property_type: "relation", sort_order: 0 }];
    if (direction === "outgoing") source.property_schemas.push({ id: "destination-relation", space_id: "destination", name: "Destination relation", property_type: "relation", sort_order: 0 });
    source.property_values = [{ id: "edge", document_id: direction === "incoming" ? "outside" : "page", property_id: "relation", value: direction === "incoming" ? "page" : "outside" }];
    await dataClient.importLocalVaultSnapshot(source);
    const session = await dataClient.reviewLocalMove(intent, () => true);
    const chosen = decisions();
    if (direction === "outgoing") chosen.properties = [{ source_id: "relation", choice: { kind: "map", destination_id: "destination-relation" } }];
    chosen.relations = [{ value_id: "edge", choice: disposition === "map" ? { kind: "map", document_id: "target" } : { kind: disposition } }];
    const result = await session.commit({ ...input(), decisions: chosen });
    expect(result.record.before.property_schemas).toContainEqual(source.property_schemas[0]);
    const beforeRead = [...storage.entries()];
    expect(await dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).toEqual([result.record]);
    expect([...storage.entries()]).toEqual(beforeRead);
    const endpoint = disposition === "map" ? "target" : "outside";
    await createLocalVaultAdapter("none").commitSnapshotMutation(snapshot => {
      const page = snapshot.documents.find(row => row.id === endpoint)!;
      if (unavailable === "archive") page.archived_at = "2026-10-09T13:00:00Z";
      else {
        page.published_at = undefined;
        snapshot.drafts = [...(snapshot.drafts ?? []), { ...snapshot.drafts![0], id: "foreign-endpoint", document_id: endpoint, space_id: page.space_id, owner_id: "foreign" }];
      }
      return { snapshot };
    });
    const afterChange = [...storage.entries()];
    await expect(dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).rejects.toThrow("evidence is unavailable");
    expect([...storage.entries()]).toEqual(afterChange);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).reviewed_move_records).toEqual([result.record]);
  });
  it("retains legacy evidence missing original incident schema proof and refuses to infer it from current schemas", async () => {
    const source = fixture();
    source.documents.push({ ...source.documents[0], id: "outside", title: "Outside" });
    source.property_schemas = [{ id: "relation", space_id: "source", name: "Relation", property_type: "relation", sort_order: 0 }];
    source.property_values = [{ id: "edge", document_id: "outside", property_id: "relation", value: "page" }];
    await dataClient.importLocalVaultSnapshot(source);
    const session = await dataClient.reviewLocalMove(intent, () => true); const chosen = decisions();
    chosen.relations = [{ value_id: "edge", choice: { kind: "preserve" } }];
    await session.commit({ ...input(), decisions: chosen });
    await createLocalVaultAdapter("none").commitSnapshotMutation(snapshot => {
      snapshot.reviewed_move_records![0].before.property_schemas = [];
      snapshot.reviewed_move_records![0].after.property_schemas = [];
      return { snapshot };
    });
    const legacy = await createLocalVaultAdapter("none").loadSnapshot(); const beforeRead = [...storage.entries()];
    await expect(dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).rejects.toThrow("evidence is unavailable");
    expect([...storage.entries()]).toEqual(beforeRead);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).reviewed_move_records).toEqual(legacy.reviewed_move_records);
  });
  it("reconciles actual committed storage after lost ACK without rewriting later B or canonical revision", async () => {
    const session = await dataClient.reviewLocalMove(intent, () => true);
    const probe = createLocalVaultAdapter("none"); const prototype = Object.getPrototypeOf(probe) as typeof probe;
    const original: typeof probe.commitSnapshotMutation = prototype.commitSnapshotMutation;
    let loseAck = true;
    vi.spyOn(prototype, "commitSnapshotMutation").mockImplementation(async function (this: typeof probe, ...args: Parameters<typeof probe.commitSnapshotMutation>) {
      const saved = await original.apply(this, args);
      if (loseAck && saved.reviewed_move_records?.some(row => row.id === input().operation_id)) { loseAck = false; throw new Error("lost acknowledgment"); }
      return saved;
    });
    await expect(session.commit(input())).rejects.toThrow("lost acknowledgment");
    const receipt = (await createLocalVaultAdapter("none").loadSnapshot()).reviewed_move_records![0];
    await createLocalVaultAdapter("none").commitSnapshotMutation(snapshot => {
      snapshot.documents.find(row => row.id === "page")!.content = "canonical later B";
      return { snapshot };
    });
    const canonical = await createLocalVaultAdapter("none").loadSnapshot(); const beforeRetry = [...storage.entries()];
    const result = await session.commit(input());
    expect(result.canonical_committed).toBe(true); expect(result.record).toEqual(receipt);
    expect([...storage.entries()]).toEqual(beforeRetry);
    const reread = await createLocalVaultAdapter("none").loadSnapshot();
    // loadSnapshot generates this read-time field; disk bytes/revision remain exact above.
    expect({ ...reread, sync_state: { ...reread.sync_state, updated_at: canonical.sync_state!.updated_at } }).toEqual(canonical);
    expect((await dataClient.getDocumentContent("destination", "page")).content).toBe("canonical later B");
    expect(result.record.after.documents).toContainEqual(expect.objectContaining({ id: "page", content: "exact A starlane://asset/owned" }));
  });
  it("retains a committed unknown request and denies mismatched historical operation evidence", async () => {
    const session = await dataClient.reviewLocalMove(intent, () => true);
    const probe = createLocalVaultAdapter("none"); const prototype = Object.getPrototypeOf(probe) as typeof probe;
    const original: typeof probe.commitSnapshotMutation = prototype.commitSnapshotMutation;
    let loseAck = true;
    vi.spyOn(prototype, "commitSnapshotMutation").mockImplementation(async function (this: typeof probe, ...args: Parameters<typeof probe.commitSnapshotMutation>) {
      const saved = await original.apply(this, args);
      if (loseAck && saved.reviewed_move_records?.length) { loseAck = false; throw new Error("lost acknowledgment"); }
      return saved;
    });
    await expect(session.commit(input())).rejects.toThrow("lost acknowledgment");
    await createLocalVaultAdapter("none").commitSnapshotMutation(snapshot => {
      snapshot.reviewed_move_records![0].moved_at = "2026-10-09T14:00:00Z";
      return { snapshot };
    });
    const beforeRetry = [...storage.entries()];
    await expect(session.commit(input())).rejects.toThrow("does not match");
    expect([...storage.entries()]).toEqual(beforeRetry);
  });
  it.each([false, true])("browser original retry rejects full evidence tamper after confirmed=%s", async confirmed => {
    const session = await dataClient.reviewLocalMove(intent, () => true);
    const probe = createLocalVaultAdapter("none"); const prototype = Object.getPrototypeOf(probe) as typeof probe;
    const original: typeof probe.commitSnapshotMutation = prototype.commitSnapshotMutation;
    if (!confirmed) vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (this: typeof probe, ...args: Parameters<typeof probe.commitSnapshotMutation>) {
      await original.apply(this, args); throw new Error("lost acknowledgment");
    });
    if (confirmed) await session.commit(input());
    else await expect(session.commit(input())).rejects.toThrow("lost acknowledgment");
    await probe.commitSnapshotMutation(snapshot => {
      Object.assign(snapshot.reviewed_move_records![0].before.documents![0] as Record<string, unknown>, { content: "foreign evidence" });
      return { snapshot };
    });
    const beforeRetry = [...storage.entries()];
    await expect(session.commit(input())).rejects.toThrow("does not match");
    expect([...storage.entries()]).toEqual(beforeRetry);
  });
  it("cannot reroll the original operation after a dispatched uncertain failure", async () => {
    const session = await dataClient.reviewLocalMove(intent, () => true);
    const probe = createLocalVaultAdapter("none"); const prototype = Object.getPrototypeOf(probe) as typeof probe;
    const original: typeof probe.commitSnapshotMutation = prototype.commitSnapshotMutation;
    vi.spyOn(prototype, "commitSnapshotMutation").mockImplementationOnce(async function (this: typeof probe, ...args: Parameters<typeof probe.commitSnapshotMutation>) {
      await original.apply(this, args); throw new Error("lost acknowledgment");
    });
    await expect(session.commit(input())).rejects.toThrow("lost acknowledgment");
    const beforeRetry = [...storage.entries()];
    await expect(session.commit({ ...input(), operation_id: "22345678-1234-1234-1234-123456789abc" })).rejects.toThrow("retry cannot change");
    expect([...storage.entries()]).toEqual(beforeRetry);
  });
  it.each([null, ""])("retains a valid unassigned relation %s through actual commit and readonly inspection", async value => {
    const source = fixture();
    source.property_schemas = [{ id: "relation", space_id: "source", name: "Relation", property_type: "relation", sort_order: 0 }, { id: "destination-relation", space_id: "destination", name: "Destination relation", property_type: "relation", sort_order: 0 }];
    source.property_values = [{ id: "empty-edge", document_id: "page", property_id: "relation", value }];
    await dataClient.importLocalVaultSnapshot(source);
    const session = await dataClient.reviewLocalMove(intent, () => true); const chosen = decisions();
    chosen.properties = [{ source_id: "relation", choice: { kind: "map", destination_id: "destination-relation" } }];
    const result = await session.commit({ ...input(), decisions: chosen });
    const beforeRead = [...storage.entries()];
    expect(await dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).toEqual([result.record]);
    expect([...storage.entries()]).toEqual(beforeRead);
    expect((await createLocalVaultAdapter("none").loadSnapshot()).property_values).toEqual([{ id: "empty-edge", document_id: "page", property_id: "destination-relation", value }]);
  });
  it.each([false, true])("does not overlay its own A publication onto later canonical B, pending RAM edit=%s", async pendingRam => {
    const session = await dataClient.reviewLocalMove(intent, () => true);
    const first = await session.commit(input());
    await createLocalVaultAdapter("none").commitSnapshotMutation(snapshot => {
      snapshot.spaces.push({ ...snapshot.spaces[0], id: "third", name: "Third" });
      const page = snapshot.documents.find(row => row.id === "page")!;
      page.space_id = "third"; page.content = "canonical B body";
      return { snapshot };
    });
    const beforeRetry = [...storage.entries()];
    let release!: () => void, reached!: () => void;
    const arrived = new Promise<void>(resolve => { reached = resolve; });
    const gate = new Promise<void>(resolve => { release = resolve; });
    const probe = createLocalVaultAdapter("none"); const prototype = Object.getPrototypeOf(probe) as typeof probe;
    const original: typeof probe.loadSnapshot = prototype.loadSnapshot;
    if (pendingRam) vi.spyOn(prototype, "loadSnapshot").mockImplementationOnce(async function (this: typeof probe) {
      const canonical = await original.call(this); reached(); await gate; return canonical;
    });
    const retry = session.commit(input());
    let pendingEdit: Promise<unknown> | undefined;
    if (pendingRam) {
      await arrived;
      pendingEdit = dataClient.updateDocumentContent("destination", "page", "unsaved RAM B body");
      release();
    }
    const ack = await retry;
    expect(ack.record).toEqual(first.record);
    if (!pendingRam) expect([...storage.entries()]).toEqual(beforeRetry);
    if (pendingEdit) await pendingEdit;
    expect((await dataClient.getDocumentContent("third", "page")).content).toBe(pendingRam ? "unsaved RAM B body" : "canonical B body");
    await dataClient.updateDocumentContent("third", "page", "ordinary later save");
    const reopened = await createLocalVaultAdapter("none").loadSnapshot();
    expect(reopened.documents.find(row => row.id === "page")).toEqual(expect.objectContaining({ space_id: "third", content: "ordinary later save" }));
    expect(reopened.reviewed_move_records).toEqual([first.record]);
  });
  it("rejects malformed imported recovery before changing the original vault", async () => {
    const before = await dataClient.exportLocalVaultSnapshot(); const corrupt = { ...fixture(), reviewed_move_records: null };
    await expect(dataClient.importLocalVaultSnapshot(corrupt)).rejects.toThrow("recovery records are malformed");
    expect((await dataClient.exportLocalVaultSnapshot()).documents).toEqual(before.documents);
  });
});
