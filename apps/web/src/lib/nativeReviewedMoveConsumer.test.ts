import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LocalVaultSnapshot } from "./localVault";
import type { NativeLocalDatabaseDescriptor } from "./nativeLocalDatabase";
import type { LocalReviewedMoveDecisions } from "./localReviewedMove";
const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
// Native serde_json::Value serializes maps in key order; emulate its decoded JSON.
function nativeRoundtrip<T>(value: T): T {
  const order = (item: unknown): unknown => Array.isArray(item) ? item.map(order) : item && typeof item === "object" ? Object.fromEntries(Object.keys(item).sort().map(key => [key, order((item as Record<string, unknown>)[key])])) : item;
  return order(copy(value)) as T;
}
const intent = { source_space_id: "source", root: { kind: "document" as const, id: "page" }, destination: { space_id: "destination", folder_id: null, parent_id: null } };
const request = () => ({ operation_id: "12345678-1234-1234-1234-123456789abc", moved_at: "2026-10-09T12:00:00Z", decisions: { properties: [], workflows: [], tags: [], relations: [], identifiers: [], maintenance: [], detach_hierarchy: [], preserve_previous_draft_placement: true } satisfies LocalReviewedMoveDecisions });
function fixture(): LocalVaultSnapshot {
  return { schema_version: 1, canonical_revision: "r1", spaces: ["source", "destination", "third"].map(id => ({ id, name: id, created_at: "old", updated_at: "old" })), documents: [{ id: "page", space_id: "source", title: "Page", content: "original A", published_at: "old", updated_at: "old" }], folders: [], drafts: [], assets: [], templates: [], sync_state: { state: "idle", updated_at: "old", queued_operations: [] } };
}

describe("actual native reviewed move consumer uses original destination IPC", () => {
  let dataClient: typeof import("./dataClient").dataClient;
  let selected: string;
  let descriptor: NativeLocalDatabaseDescriptor;
  let canonical: LocalVaultSnapshot | null;
  let loseAck: boolean;
  let changeAfterCommit: boolean;
  let corruptReceipt: boolean;
  let afterCommit: (() => void) | undefined;
  let onRead: (() => Promise<void>) | undefined;
  let onDescribe: (() => Promise<void>) | undefined;
  let invoke: ReturnType<typeof vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>>;
  const disk = new Map<string, string>();
  const proof = () => ({ schema_version: 1, provider: "none", store_kind: "sqlite", database_path: "/vault/A/local.sqlite", supports_reviewed_move_evidence: true });
  beforeEach(async () => {
    vi.resetModules(); disk.clear(); selected = "/vault/A/local.sqlite"; canonical = fixture(); loseAck = false; changeAfterCommit = false; corruptReceipt = false; afterCommit = undefined; onRead = undefined; onDescribe = undefined;
    descriptor = { schema_version: 1, provider: "none", store_kind: "sqlite", database_path: selected, supports_snapshot_bridge: true, supports_atomic_snapshot_commit: true, supports_operation_queue: true, supports_lossless_replay_queue: true, supports_reviewed_move_evidence: true, supports_reviewed_move_destination_binding: true, replay_backends: ["sqlite"] };
    const storage = { getItem: (key: string) => disk.get(key) ?? null, setItem: (key: string, value: string) => { disk.set(key, value); }, removeItem: (key: string) => { disk.delete(key); }, key: (index: number) => [...disk.keys()][index] ?? null, get length() { return disk.size; } };
    invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
      if (command === "describe_native_local_database") { const hook = onDescribe; onDescribe = undefined; if (hook) await hook(); return nativeRoundtrip(descriptor); }
      if (command === "read_native_local_database_snapshot") {
        const hook = onRead; onRead = undefined; if (hook) await hook();
        expect(args?.expectedDestination).toEqual(proof());
        if (selected !== proof().database_path) throw new Error("Original destination changed");
        return nativeRoundtrip(canonical);
      }
      if (command === "commit_native_local_database_snapshot") {
        expect(args?.expectedDestination).toEqual(proof());
        if (selected !== proof().database_path) throw new Error("Original destination changed");
        expect(args?.expectedRevision).toBe(canonical?.canonical_revision ?? null);
        canonical = nativeRoundtrip(args?.snapshot as LocalVaultSnapshot); canonical.canonical_revision = "r2"; canonical.mirror_pending = true;
        if (changeAfterCommit) { canonical.documents[0].space_id = "third"; canonical.documents[0].content = "later canonical B"; canonical.canonical_revision = "r3"; }
        afterCommit?.();
        if (loseAck) throw new Error("lost ACK after actual commit");
        return { canonical_committed: corruptReceipt ? false : true, canonical_revision: canonical.canonical_revision, mirror_pending: true, mirror_error: "repair later" };
      }
      throw new Error(`Unexpected unbound or fallback command ${command}`);
    });
    vi.stubGlobal("window", { addEventListener: vi.fn(), localStorage: storage, __TAURI_INTERNALS__: { invoke }, navigator: {} }); vi.stubGlobal("localStorage", storage);
    const launch = await import("./launchMode"); const providers = await import("./storageProviders");
    launch.setCurrentLaunchMode("local"); providers.setStorageMode("local"); providers.setSyncProvider("none");
    dataClient = (await import("./dataClient")).dataClient;
  });
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
  it("compares complete JSON independent of object order without relaxing unknown fields, scalar types or array order", async () => {
    const { nativeJsonValueEquals } = await import("./nativeLocalDatabase");
    const original = { z: [{ future: { "10": "ten", "2": "two", "😀": null, "\ue000": false }, value: 1 }], a: "exact" };
    expect(nativeJsonValueEquals(original, nativeRoundtrip(original))).toBe(true);
    expect(nativeJsonValueEquals(original, { ...nativeRoundtrip(original), added: null })).toBe(false);
    expect(nativeJsonValueEquals(original, { ...original, z: [{ ...original.z[0], value: "1" }] })).toBe(false);
    expect(nativeJsonValueEquals([1, 2], [2, 1])).toBe(false);
    expect(nativeJsonValueEquals({ future: null }, {})).toBe(false);
  });
  it("captures descriptor before first preview read and binds every actual read and commit without mirror fallback", async () => {
    const original = await dataClient.reviewLocalMove(intent, () => true);
    const result = await original.commit(request());
    expect(result.canonical_committed).toBe(true); expect(canonical?.documents[0].space_id).toBe("destination");
    expect(canonical?.reviewed_move_records).toEqual([result.record]);
    expect(invoke.mock.calls[0][0]).toBe("describe_native_local_database");
    expect(invoke.mock.calls.filter(call => call[0] === "describe_native_local_database")).toHaveLength(1);
    expect(invoke.mock.calls.filter(call => call[0] === "commit_native_local_database_snapshot")).toHaveLength(1);
    expect(invoke.mock.calls.every(call => ["describe_native_local_database", "read_native_local_database_snapshot", "commit_native_local_database_snapshot"].includes(call[0]))).toBe(true);
    expect([...disk.keys()].some(key => key.includes("vault"))).toBe(false);
  });
  it.each(["supports_reviewed_move_destination_binding", "supports_reviewed_move_evidence", "supports_atomic_snapshot_commit", "supports_snapshot_bridge"] as const)("denies unsupported descriptor %s before canonical reads or mutations", async flag => {
    descriptor[flag] = false;
    await expect(dataClient.reviewLocalMove(intent, () => true)).rejects.toThrow("updated desktop shell");
    expect(invoke.mock.calls).toHaveLength(1); expect(canonical).toEqual(fixture());
  });
  it("denies mobile-style snapshot store and malformed original path", async () => {
    descriptor.store_kind = "snapshot-json"; await expect(dataClient.reviewLocalMove(intent, () => true)).rejects.toThrow("updated desktop shell");
    descriptor.store_kind = "sqlite"; descriptor.database_path = " "; await expect(dataClient.reviewLocalMove(intent, () => true)).rejects.toThrow("updated desktop shell");
    expect(invoke.mock.calls.every(call => call[0] === "describe_native_local_database")).toBe(true);
  });
  it("cannot adopt another selected root after preview even if it contains identical snapshot bytes", async () => {
    const original = await dataClient.reviewLocalMove(intent, () => true); selected = "/vault/B/local.sqlite";
    await expect(original.commit(request())).rejects.toThrow("Original destination changed");
    expect(canonical).toEqual(fixture()); expect(invoke.mock.calls.some(call => call[0] === "commit_native_local_database_snapshot")).toBe(false);
  });
  it("a missing original canonical DB cannot acknowledge a portable or browser copy", async () => {
    canonical = null; disk.set("starlane-vault-none", JSON.stringify(fixture()));
    await expect(dataClient.reviewLocalMove(intent, () => true)).rejects.toThrow("canonical snapshot is unavailable");
    expect(invoke.mock.calls.some(call => call[0] === "read_vault_snapshot")).toBe(false);
  });
  it.each([false, true])("lost ACK returns only the exact original operation and keeps later canonical B=%s", async laterB => {
    const original = await dataClient.reviewLocalMove(intent, () => true); loseAck = true; changeAfterCommit = laterB;
    const result = await original.commit(request());
    expect(result.record.after.documents).toContainEqual(expect.objectContaining({ id: "page", content: "original A", space_id: "destination" }));
    expect(canonical?.documents[0].content).toBe(laterB ? "later canonical B" : "original A");
    const beforeRetry = JSON.stringify(canonical); const calls = invoke.mock.calls.filter(call => call[0] === "commit_native_local_database_snapshot").length;
    expect((await original.commit(request())).record).toEqual(result.record);
    expect(JSON.stringify(canonical)).toBe(beforeRetry); expect(invoke.mock.calls.filter(call => call[0] === "commit_native_local_database_snapshot")).toHaveLength(calls);
    expect((await dataClient.getDocumentContent(laterB ? "third" : "destination", "page")).content).toBe(laterB ? "later canonical B" : "original A");
  });
  it("does not publish a successful native result after original account token drift", async () => {
    const original = await dataClient.reviewLocalMove(intent, () => true);
    onRead = async () => { localStorage.setItem("access_token", "foreign-account-token"); };
    await expect(original.commit(request())).rejects.toThrow("workspace or selection changed");
    expect(invoke.mock.calls.some(call => call[0] === "commit_native_local_database_snapshot")).toBe(false);
  });
  it("same-session historical retry after successful A returns original evidence without overlaying later canonical B", async () => {
    const original = await dataClient.reviewLocalMove(intent, () => true); const first = await original.commit(request());
    canonical!.documents[0].space_id = "third"; canonical!.documents[0].content = "later canonical B"; canonical!.canonical_revision = "r3";
    const beforeRetry = JSON.stringify(canonical);
    const result = await original.commit(request());
    expect(result.record).toEqual(first.record); expect(JSON.stringify(canonical)).toBe(beforeRetry);
    expect((await dataClient.getDocumentContent("third", "page")).content).toBe("later canonical B");
    expect(invoke.mock.calls.filter(call => call[0] === "commit_native_local_database_snapshot")).toHaveLength(1);
  });
  it("a malformed IPC receipt requires matching bound operation evidence before acknowledgment", async () => {
    const original = await dataClient.reviewLocalMove(intent, () => true); corruptReceipt = true;
    const result = await original.commit(request());
    expect(result.record.id).toBe(request().operation_id); expect(canonical?.reviewed_move_records).toEqual([result.record]);
    expect(invoke.mock.calls.filter(call => call[0] === "commit_native_local_database_snapshot")).toHaveLength(1);
  });
  it.each(["actor", "unknown field"])("lost ACK cannot use changed %s in retained operation evidence from the original database", async changed => {
    const original = await dataClient.reviewLocalMove(intent, () => true); loseAck = true;
    afterCommit = () => {
      if (changed === "actor") canonical!.reviewed_move_records![0].actor_id = "foreign";
      else Object.assign(canonical!.reviewed_move_records![0], { future_evidence: { extra: "changed" } });
    };
    await expect(original.commit(request())).rejects.toThrow("outcome is unknown");
    expect((await dataClient.getDocumentContent("source", "page")).content).toBe("original A");
    expect(canonical?.reviewed_move_records![0].id).toBe(request().operation_id);
    await expect(original.commit(request())).rejects.toThrow("does not match");
    expect(invoke.mock.calls.filter(call => call[0] === "commit_native_local_database_snapshot")).toHaveLength(1);
  });
  it.each(["unknown", "before", "after", "preserved", "removed", "array"])("same original retry rejects complete %s evidence tampering after lost ACK", async field => {
    if (field === "array") canonical!.documents.push({ ...canonical!.documents[0], id: "child", parent_id: "page", title: "Child" });
    const original = await dataClient.reviewLocalMove(intent, () => true); loseAck = true;
    afterCommit = () => {
      const record = canonical!.reviewed_move_records![0];
      if (field === "unknown") Object.assign(record, { future_evidence: ["changed", null] });
      if (field === "before") Object.assign(record.before.documents![0] as Record<string, unknown>, { content: "foreign before" });
      if (field === "after") Object.assign(record.after.documents![0] as Record<string, unknown>, { content: "foreign after" });
      if (field === "preserved") record.preserved.push({ collection: "documents", records: [{ id: "foreign" }] });
      if (field === "removed") record.removed.push({ collection: "documents", records: [{ id: "foreign" }] });
      if (field === "array") record.before.documents!.reverse();
    };
    await expect(original.commit(request())).rejects.toThrow("outcome is unknown");
    const retained = copy(canonical);
    await expect(original.commit(request())).rejects.toThrow("does not match");
    expect(canonical).toEqual(retained);
    expect((await dataClient.getDocumentContent("source", "page")).content).toBe("original A");
    expect(invoke.mock.calls.filter(call => call[0] === "commit_native_local_database_snapshot")).toHaveLength(1);
  });
  it("confirmed native A cannot later acknowledge a changed full receipt", async () => {
    const original = await dataClient.reviewLocalMove(intent, () => true); await original.commit(request());
    Object.assign(canonical!.reviewed_move_records![0], { future_evidence: { changed: true } });
    const retained = copy(canonical);
    await expect(original.commit(request())).rejects.toThrow("does not match");
    expect(canonical).toEqual(retained);
    expect(invoke.mock.calls.filter(call => call[0] === "commit_native_local_database_snapshot")).toHaveLength(1);
  });
  it("late token drift after canonical commit retains original evidence without publishing foreign RAM", async () => {
    const original = await dataClient.reviewLocalMove(intent, () => true);
    afterCommit = () => { localStorage.setItem("access_token", "foreign-account-token"); };
    await expect(original.commit(request())).rejects.toThrow("workspace or selection changed");
    expect(canonical?.reviewed_move_records![0].id).toBe(request().operation_id);
    localStorage.removeItem("access_token");
    (await import("./api")).api.setToken(null); // Restore original memory AND disk token.
    expect((await dataClient.getDocumentContent("source", "page")).content).toBe("original A");
    afterCommit = undefined;
    const retry = await original.commit(request()); expect(retry.record.id).toBe(request().operation_id);
    expect(invoke.mock.calls.filter(call => call[0] === "commit_native_local_database_snapshot")).toHaveLength(1);
  });
  it("Properties uses a fresh captured bound reader and refuses a root changed during its actual read", async () => {
    const original = await dataClient.reviewLocalMove(intent, () => true); const result = await original.commit(request());
    expect(await dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).toEqual([result.record]);
    onRead = async () => { selected = "/vault/B/local.sqlite"; };
    await expect(dataClient.inspectLocalReviewedMoveRecords("destination", "page", () => true)).rejects.toThrow("Original destination changed");
    expect(canonical?.reviewed_move_records).toEqual([result.record]);
  });
  it("late context drift while the original descriptor awaits prevents even the preview read", async () => {
    onDescribe = async () => { localStorage.setItem("refresh_token", "foreign"); };
    await expect(dataClient.reviewLocalMove(intent, () => true)).rejects.toThrow("workspace or selection changed");
    expect(invoke.mock.calls).toHaveLength(1);
  });
});
