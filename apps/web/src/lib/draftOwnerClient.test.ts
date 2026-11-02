import { runDraftOwnerSelection } from "./draftOwnerSelection";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { api, captureOfflineServerContext } from "./api";
import { dataClient, __resetLocalDataForTests, captureDraftSaveSelection } from "./dataClient";
import { setCurrentLaunchMode } from "./launchMode";
import { setStorageMode, setSyncProvider } from "./storageProviders";
import { cacheServerResponse, listOfflineServerMutations, queueOfflineServerMutation } from "./offlineServerCache";
const token = (sub = "owner-a") => `e30.${btoa(JSON.stringify({ sub, sid: `session-${sub}`, token_type: "access" }))}.fixture`;
const actions = {
  delete: () => dataClient.deleteDocumentDraft("s", "p", "d"),
  grant: () => dataClient.grantDocumentDraftAccess("s", "p", "d", "person-a", "editor"),
  revoke: () => dataClient.revokeDocumentDraftAccess("s", "p", "d", "person-a"),
};
describe("actual owner draft caller original transport", () => {
  beforeEach(() => {
    vi.unstubAllGlobals(); const storage = new Map<string, string>();
    const local = { get length() { return storage.size; }, key: (i: number) => [...storage.keys()][i] ?? null, getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => { storage.set(k, v); }, removeItem: (k: string) => { storage.delete(k); } };
    vi.stubGlobal("localStorage", local); vi.stubGlobal("window", { localStorage: local }); vi.stubGlobal("navigator", { onLine: true });
    api.logout(); setCurrentLaunchMode("server"); setStorageMode("server"); setSyncProvider("none"); api.setToken(token());
  });
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
  for (const [name, action] of Object.entries(actions)) {
    it(`${name} accepts only actual204 using original bearer and leaves legacy queue untouched`, async () => {
      const origin = captureOfflineServerContext().origin;
      queueOfflineServerMutation("/api/v1", "/spaces/s/documents/p/drafts/legacy", "DELETE", undefined, origin.cacheScope, origin, "unknown");
      const original = listOfflineServerMutations();
      const fetcher = vi.fn(async (_url: unknown, _init?: RequestInit) => new Response(null, { status: 204 })); vi.stubGlobal("fetch", fetcher);
      await expect(action()).resolves.toBeUndefined(); expect(fetcher).toHaveBeenCalledTimes(1);
      expect(fetcher.mock.calls[0][1]).toMatchObject({ headers: { Authorization: `Bearer ${token()}` }, method: name === "grant" ? "PUT" : "DELETE" });
      expect(listOfflineServerMutations()).toEqual(original);
    });
    it(`${name} offline is not dispatched or queued as a false acknowledgment`, async () => {
      vi.stubGlobal("navigator", { onLine: false }); const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
      await expect(action()).rejects.toThrow("not dispatched"); expect(fetcher).not.toHaveBeenCalled(); expect(listOfflineServerMutations()).toEqual([]);
    });
    it(`${name} propagates403 without refreshing or changing queue`, async () => {
      const fetcher = vi.fn(async () => new Response(JSON.stringify({ message: "Owner access denied" }), { status: 403 })); vi.stubGlobal("fetch", fetcher);
      await expect(action()).rejects.toMatchObject({ status: 403 }); expect(fetcher).toHaveBeenCalledTimes(1); expect(listOfflineServerMutations()).toEqual([]);
    });
    it(`${name} network uncertainty rejects without false success or automatic replay`, async () => {
      const fetcher = vi.fn().mockRejectedValue(new Error("lost after possible commit")); vi.stubGlobal("fetch", fetcher);
      await expect(action()).rejects.toThrow("could not be confirmed"); expect(fetcher).toHaveBeenCalledTimes(1); expect(listOfflineServerMutations()).toEqual([]);
    });
  }
  for (const drift of ["account", "logout", "provider"] as const) it(`held successful grant under ${drift} drift cannot return an ACK`, async () => {
    let release!: () => void; const hold = new Promise<void>(resolve => { release = resolve; }); let reached!: () => void; const ready = new Promise<void>(resolve => { reached = resolve; });
    const fetcher = vi.fn(async () => { reached(); await hold; return new Response(null, { status: 204 }); }); vi.stubGlobal("fetch", fetcher);
    const pending = expect(actions.grant()).rejects.toThrow("could not be confirmed"); await ready;
    if (drift === "account") api.setToken(token("owner-b")); else if (drift === "logout") api.logout(); else setSyncProvider("google-drive"); release(); await pending;
    expect(fetcher).toHaveBeenCalledTimes(1); expect(listOfflineServerMutations()).toEqual([]);
  });
  it("held response-text decode also fences provider before204 acknowledgment", async () => {
    let release!: () => void; const hold = new Promise<void>(resolve => { release = resolve; }); let reached!: () => void; const ready = new Promise<void>(resolve => { reached = resolve; });
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, status: 204, text: async () => { reached(); await hold; return ""; } })));
    const pending = expect(actions.delete()).rejects.toThrow("could not be confirmed"); await ready; setSyncProvider("google-drive"); release(); await pending;
  });
  it("valid-looking200 mutation is not a bodyless204 ACK", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 200 }))); await expect(actions.grant()).rejects.toMatchObject({ code: "outcome_unproven" });
  });
  it("both actual owner catalogs use fresh exact200 without creating queued work", async () => {
    const row = { user_id: "person-a", display_name: "Fictional Person", email: "person@example.invalid", role: "viewer" };
    const fetcher = vi.fn(async () => new Response(JSON.stringify([row]), { status: 200 })); vi.stubGlobal("fetch", fetcher);
    expect(await dataClient.listDocumentDraftAccess("s", "p", "d")).toEqual([row]); expect(await dataClient.listDocumentDraftAccessCandidates("s", "p", "d")).toEqual([row]); expect(fetcher).toHaveBeenCalledTimes(2); expect(listOfflineServerMutations()).toEqual([]);
  });
  it("catalog network failure never returns an old cached roster", async () => {
    const origin = captureOfflineServerContext().origin; const old = [{ user_id: "old", display_name: "Old", email: "old@example.invalid", role: "editor" }];
    cacheServerResponse("/api/v1", "/spaces/s/documents/p/drafts/d/access", old, origin.cacheScope);
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("offline"))); await expect(dataClient.listDocumentDraftAccess("s", "p", "d")).rejects.toThrow("could not be inspected");
  });
  it("malformed fresh catalogs cannot masquerade as valid participant choices", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify([{ user_id: "person-a", display_name: "Bad", email: "x", role: "admin" }]), { status: 200 })));
    await expect(dataClient.listDocumentDraftAccess("s", "p", "d")).rejects.toThrow("participant response");
  });
  it("fresh owner catalogs reject successful201 instead of treating it as an inspection", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("[]", { status: 201 })));
    await expect(dataClient.listDocumentDraftAccessCandidates("s", "p", "d")).rejects.toMatchObject({ code: "outcome_unproven" });
  });
  it("a held fresh roster cannot return A labels after switching to B", async () => {
    let release!: () => void; const hold = new Promise<void>(resolve => { release = resolve; }); let reached!: () => void; const ready = new Promise<void>(resolve => { reached = resolve; });
    vi.stubGlobal("fetch", vi.fn(async () => { reached(); await hold; return new Response(JSON.stringify([{ user_id: "person-a", display_name: "A labels", email: "a@example.invalid", role: "viewer" }]), { status: 200 }); }));
    const pending = expect(dataClient.listDocumentDraftAccess("s", "p", "d")).rejects.toThrow("could not be inspected"); await ready; api.setToken(token("owner-b")); release(); await pending;
  });
  for (const [name, action] of Object.entries(actions)) it(`accepted A selection cannot dispatch ${name} under B even with identical route IDs`, async () => {
    const original = captureDraftSaveSelection(); api.setToken(token("owner-b"));
    const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    await expect(runDraftOwnerSelection(original, action)).rejects.toThrow(); expect(fetcher).not.toHaveBeenCalled(); expect(listOfflineServerMutations()).toEqual([]);
  });
  for (const drift of ["provider", "server"] as const) it(`accepted same-account selection cannot be rebound after ${drift} changes`, async () => {
    if (drift === "server") vi.stubGlobal("window", { localStorage, __TAURI_INTERNALS__: {} });
    const original = captureDraftSaveSelection();
    if (drift === "provider") setSyncProvider("google-drive"); else localStorage.setItem("server_url", "http://fictional-b.invalid");
    const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher); await expect(runDraftOwnerSelection(original, actions.grant)).rejects.toThrow(); expect(fetcher).not.toHaveBeenCalled();
  });
  it("missing accepted selection is not replaced by a freshly captured authority", async () => {
    const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher); await expect(runDraftOwnerSelection(null, actions.revoke)).rejects.toThrow("Reload Drafts"); expect(fetcher).not.toHaveBeenCalled();
  });
  it("actual consumed original-selection seam admits unchanged owner204", async () => {
    const original = captureDraftSaveSelection(); const fetcher = vi.fn(async () => new Response(null, { status: 204 })); vi.stubGlobal("fetch", fetcher);
    await expect(runDraftOwnerSelection(original, actions.grant)).resolves.toBeUndefined(); expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("local owned draft deletion keeps the existing single-owner storage path without transport", async () => {
    setCurrentLaunchMode("local"); setStorageMode("local"); await __resetLocalDataForTests();
    const page = await dataClient.createDocument("local-space", { title: "Local" }); const draft = await dataClient.createDocumentDraft("local-space", page.id);
    const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher); await dataClient.deleteDocumentDraft("local-space", page.id, draft.id);
    expect((await dataClient.listDocumentDrafts("local-space", page.id)).some(row => row.id === draft.id)).toBe(false); expect(fetcher).not.toHaveBeenCalled();
  });
});
