import { runDraftOwnerSelection } from "./draftOwnerSelection";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { api, captureOfflineServerContext } from "./api";
import { dataClient, __resetLocalDataForTests, captureDraftSaveSelection } from "./dataClient";
import { setCurrentLaunchMode } from "./launchMode";
import { setStorageMode, setSyncProvider } from "./storageProviders";
import { listOfflineServerMutations, queueOfflineServerMutation } from "./offlineServerCache";
const token = (sub = "owner-a") => `e30.${btoa(JSON.stringify({ sub, sid: `session-${sub}`, token_type: "access" }))}.fixture`;
describe("actual whole-space draft purge original transport", () => {
  beforeEach(() => {
    vi.unstubAllGlobals(); const storage = new Map<string, string>();
    const local = { get length() { return storage.size; }, key: (i: number) => [...storage.keys()][i] ?? null, getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => { storage.set(k, v); }, removeItem: (k: string) => { storage.delete(k); } };
    vi.stubGlobal("localStorage", local); vi.stubGlobal("window", { localStorage: local }); vi.stubGlobal("navigator", { onLine: true });
    api.logout(); setCurrentLaunchMode("server"); setStorageMode("server"); setSyncProvider("none"); api.setToken(token());
  });
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
  for (const count of [0, 7]) it(`exact200 accepts confirmed count${count} without changing older queued evidence`, async () => {
    const origin = captureOfflineServerContext().origin;
    queueOfflineServerMutation("/api/v1", "/spaces/s/drafts", "DELETE", undefined, origin.cacheScope, origin, "unknown");
    const before = listOfflineServerMutations();
    const fetcher = vi.fn(async (_url: unknown, _init?: RequestInit) => new Response(JSON.stringify({ deleted: count }), { status: 200 })); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.purgeDocumentDrafts("s")).resolves.toBe(count);
    expect(fetcher).toHaveBeenCalledTimes(1); expect(fetcher.mock.calls[0][0]).toBe("/api/v1/spaces/s/drafts");
    expect(listOfflineServerMutations()).toEqual(before);
  });
  it("offline purge stays undispatched and creates no queued DELETE", async () => {
    vi.stubGlobal("navigator", { onLine: false }); const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.purgeDocumentDrafts("s")).rejects.toThrow("not dispatched"); expect(fetcher).not.toHaveBeenCalled(); expect(listOfflineServerMutations()).toEqual([]);
  });
  it("actual403 retains work without refresh or queued retry", async () => {
    const fetcher = vi.fn(async () => new Response(JSON.stringify({ message: "Manager access denied" }), { status: 403 })); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.purgeDocumentDrafts("s")).rejects.toMatchObject({ status: 403 }); expect(fetcher).toHaveBeenCalledTimes(1); expect(listOfflineServerMutations()).toEqual([]);
  });
  it("lost response is unknown rather than saved or queued", async () => {
    const fetcher = vi.fn().mockRejectedValue(new Error("lost after possible commit")); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.purgeDocumentDrafts("s")).rejects.toThrow("could not be confirmed"); expect(fetcher).toHaveBeenCalledTimes(1); expect(listOfflineServerMutations()).toEqual([]);
  });
  for (const [status, value] of [[204, null], [201, { deleted: 1 }], [200, { deleted: -1 }], [200, { deleted: 1.5 }], [200, { deleted: "1" }], [200, { deleted: Number.MAX_SAFE_INTEGER + 1 }]] as const)
    it(`unproven status${status} count${JSON.stringify(value)} never returns a count`, async () => {
      vi.stubGlobal("fetch", vi.fn(async () => new Response(status === 204 ? null : JSON.stringify(value), { status })));
      await expect(dataClient.purgeDocumentDrafts("s")).rejects.toMatchObject({ code: "outcome_unproven" }); expect(listOfflineServerMutations()).toEqual([]);
    });
  for (const drift of ["logout", "account", "provider"] as const) it(`held real200 cannot confirm purge after ${drift}`, async () => {
    let release!: () => void; const hold = new Promise<void>(resolve => { release = resolve; }); let reached!: () => void; const ready = new Promise<void>(resolve => { reached = resolve; });
    vi.stubGlobal("fetch", vi.fn(async () => { reached(); await hold; return new Response('{"deleted":3}', { status: 200 }); }));
    const pending = expect(dataClient.purgeDocumentDrafts("s")).rejects.toThrow("could not be confirmed"); await ready;
    if (drift === "logout") api.logout(); else if (drift === "account") api.setToken(token("owner-b")); else setSyncProvider("google-drive"); release(); await pending;
    expect(listOfflineServerMutations()).toEqual([]);
  });
  it("provider change during actual response text blocks decoded count", async () => {
    let release!: () => void; const hold = new Promise<void>(resolve => { release = resolve; }); let reached!: () => void; const ready = new Promise<void>(resolve => { reached = resolve; });
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, status: 200, text: async () => { reached(); await hold; return '{"deleted":2}'; } })));
    const pending = expect(dataClient.purgeDocumentDrafts("s")).rejects.toThrow("could not be confirmed"); await ready; setSyncProvider("google-drive"); release(); await pending;
  });
  it("consumed original list selection prevents A purge dispatch under B", async () => {
    const original = captureDraftSaveSelection(); api.setToken(token("owner-b")); const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    await expect(runDraftOwnerSelection(original, () => dataClient.purgeDocumentDrafts("s"))).rejects.toThrow(); expect(fetcher).not.toHaveBeenCalled();
  });
  it("local purge preserves foreign-space drafts and uses no transport", async () => {
    setCurrentLaunchMode("local"); setStorageMode("local"); await __resetLocalDataForTests();
    const space = await dataClient.createSpace("Local purge fixture");
    const other = await dataClient.createSpace("Foreign retained fixture");
    expect((await dataClient.listSpaces()).map(row => row.id)).toEqual(expect.arrayContaining([space.id, other.id]));
    const page = await dataClient.createDocument(space.id, { title: "Local" }); await dataClient.createDocumentDraft(space.id, page.id);
    const foreign = await dataClient.createDocument(other.id, { title: "Other" }); const retained = await dataClient.createDocumentDraft(other.id, foreign.id);
    const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.purgeDocumentDrafts(space.id)).resolves.toBe(1);
    expect(await dataClient.listDocumentDrafts(space.id)).toEqual([]);
    expect((await dataClient.listDocumentDrafts(other.id, foreign.id)).some(row => row.id === retained.id)).toBe(true); expect(fetcher).not.toHaveBeenCalled();
  });
});
