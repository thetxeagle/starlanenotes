import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { api, captureOfflineServerContext } from "./api";
import { dataClient, type DraftRecord } from "./dataClient";
import { setCurrentLaunchMode } from "./launchMode";
import { setStorageMode, setSyncProvider } from "./storageProviders";
import { listOfflineServerMutations, readCachedServerResponse } from "./offlineServerCache";
import { selectedDraftFork } from "./draftBirth";

const token = (sub = "actor-a") => `e30.${btoa(JSON.stringify({ sub, sid: `session-${sub}`, token_type: "access" }))}.fixture`;
const records = () => Array.from({ length: localStorage.length }, (_, i) => localStorage.key(i)!).filter(k => k.startsWith("starlane_draft_birth_v1:")).map(k => JSON.parse(localStorage.getItem(k)!));
function result(operationId?: string): DraftRecord {
  return { id: "draft-a", document_id: "page-a", space_id: "space-a", owner_id: "actor-a", owner_name: "Fictional actor A", document_title: "Original", access_role: "owner", can_publish: true, title: "Original", content: "Original body", source_digest: "a".repeat(64), created_at: "2026-10-09T00:00:00Z", updated_at: "2026-10-09T00:00:00Z", base_updated_at: "2026-10-09T00:00:00Z", ...(operationId ? { birth_operation_id: operationId } : {}) };
}
const requestBody = (request: RequestInit | undefined) => JSON.parse(String(request?.body));

describe("actual draft birth caller and captured transport", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    const storage = new Map<string, string>();
    const local = { get length() { return storage.size; }, key: (i: number) => [...storage.keys()][i] ?? null, getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => { storage.set(k, v); }, removeItem: (k: string) => { storage.delete(k); } };
    vi.stubGlobal("localStorage", local); vi.stubGlobal("window", { localStorage: local });
    vi.stubGlobal("navigator", { onLine: true });
    api.logout(); setCurrentLaunchMode("server"); setStorageMode("server"); setSyncProvider("none"); api.setToken(token());
  });
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

  it("inspects only exact retained rejected birth in current origin without dispatch or mutation", async () => {
    const options = { force_new: true, source_draft_id: "selected-a", source_digest: "b".repeat(64) };
    const fetcher = vi.fn(async () => new Response(JSON.stringify({ message: "source_conflict: changed" }), { status: 409 }));
    vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.createDocumentDraft("space-a", "page-a", options)).rejects.toThrow();
    const original = records()[0];
    const proof = dataClient.inspectDocumentDraftBirth("space-a", "page-a", original.operation_id);
    expect(proof).toEqual({ operationId: original.operation_id, state: "rejected", options });
    proof!.options.source_digest = "c".repeat(64);
    expect(records()[0]).toEqual(original);
    expect(dataClient.inspectDocumentDraftBirth("space-a", "other-page", original.operation_id)).toBeNull();
    api.setToken(token("actor-b"));
    expect(dataClient.inspectDocumentDraftBirth("space-a", "page-a", original.operation_id)).toBeNull();
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("journals UUID and copied private source before the actual POST, without bearer or native/cache side effects", async () => {
    const options = { force_new: true, source_draft_id: "selected-a", source_digest: "b".repeat(64) };
    const fetcher = vi.fn(async (_url: unknown, init?: RequestInit) => {
      const body = requestBody(init); expect(records()).toEqual([expect.objectContaining({ operation_id: body.operation_id, body, state: "dispatched" })]);
      options.source_digest = "c".repeat(64);
      expect(init?.headers).toMatchObject({ Authorization: `Bearer ${token()}` });
      return new Response(JSON.stringify(result(body.operation_id)), { status: 201 });
    }); vi.stubGlobal("fetch", fetcher);
    const saved = await dataClient.createDocumentDraft("space-a", "page-a", options);
    expect(saved.birth_operation_id).toBe(records()[0].operation_id);
    expect(records()[0].body.source_digest).toBe("b".repeat(64));
    expect(JSON.stringify(records())).not.toContain(token()); expect(listOfflineServerMutations()).toEqual([]);
    expect(readCachedServerResponse("/api/v1", "/spaces/space-a/documents/page-a/drafts")).toBeNull();
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("retains an uncertain original request and forbids a second force-new; explicit retry sends identical UUID/body", async () => {
    const fetcher = vi.fn().mockRejectedValueOnce(new Error("lost response")).mockImplementationOnce(async (_url: unknown, init?: RequestInit) => new Response(JSON.stringify(result(requestBody(init).operation_id)), { status: 201 })); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("unknown");
    const original = records()[0];
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("previous draft request");
    expect(fetcher).toHaveBeenCalledTimes(1);
    await dataClient.retryDocumentDraftBirth("space-a", "page-a", original.operation_id);
    expect(fetcher.mock.calls[1][1].body).toBe(fetcher.mock.calls[0][1].body);
    expect(records()[0].state).toBe("confirmed");
  });

  it("retains known-unsent intent while offline and dispatches only an explicit same-origin retry", async () => {
    vi.stubGlobal("navigator", { onLine: false }); const fetcher = vi.fn(async (_url: unknown, init?: RequestInit) => new Response(JSON.stringify(result(requestBody(init).operation_id)), { status: 201 })); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.createDocumentDraft("space-a", "page-a")).rejects.toThrow("without dispatch");
    expect(records()[0].state).toBe("prepared"); expect(fetcher).not.toHaveBeenCalled();
    vi.stubGlobal("navigator", { onLine: true }); await dataClient.retryDocumentDraftBirth("space-a", "page-a", records()[0].operation_id); expect(fetcher).toHaveBeenCalledTimes(1);
  });

  for (const drift of ["logout", "account", "provider", "decode"] as const) it(`keeps original bytes unknown after held ${drift} drift and never returns a foreign result`, async () => {
    let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; }); let reached!: () => void; const ready = new Promise<void>(resolve => { reached = resolve; });
    const fetcher = vi.fn(async (_url: unknown, init?: RequestInit) => {
      const body = requestBody(init);
      if (drift === "decode") return { status: 201, ok: true, text: async () => { reached(); await held; return JSON.stringify(result(body.operation_id)); } };
      reached(); await held; return new Response(JSON.stringify(result(body.operation_id)), { status: 201 });
    }); vi.stubGlobal("fetch", fetcher);
    const pending = dataClient.createDocumentDraft("space-a", "page-a", { force_new: true }); const rejection = expect(pending).rejects.toThrow("unknown");
    await ready; if (drift === "logout") api.logout(); else if (drift === "provider") setSyncProvider("google-drive"); else api.setToken(token("actor-b")); release(); await rejection;
    expect(records()[0]).toMatchObject({ origin: { accountId: "actor-a" }, state: "unknown", body: { force_new: true } });
    expect(fetcher).toHaveBeenCalledTimes(1); expect(listOfflineServerMutations()).toEqual([]);
    await expect(dataClient.retryDocumentDraftBirth("space-a", "page-a", records()[0].operation_id)).rejects.toThrow(); expect(fetcher).toHaveBeenCalledTimes(1);
  });

  for (const status of [202, 204]) it(`does not confirm or cache ${status}`, async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(status === 204 ? null : JSON.stringify(result()), { status })));
    await expect(dataClient.createDocumentDraft("space-a", "page-a")).rejects.toThrow("unknown"); expect(records()[0].state).toBe("unknown");
  });
  it("rejects a foreign or unbound 201 response while permitting ordinary exact 200 resume", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(result()), { status: 201 })));
    await expect(dataClient.createDocumentDraft("space-a", "page-a")).rejects.toThrow("unknown");
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(result()), { status: 200 })));
    await expect(dataClient.retryDocumentDraftBirth("space-a", "page-a", records()[0].operation_id)).resolves.toMatchObject({ id: "draft-a" });
  });
  it("keeps backend commit-unknown409 held, blocks repeated force-new, and retries identical original bytes", async () => {
    const fetcher = vi.fn().mockImplementationOnce(async () => new Response(JSON.stringify({ message: "Draft birth outcome is unknown; retained evidence requires review" }), { status: 409 })).mockImplementationOnce(async (_url: unknown, init?: RequestInit) => new Response(JSON.stringify(result(requestBody(init).operation_id)), { status: 201 })); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("unknown");
    const original = records()[0]; expect(original.state).toBe("unknown");
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("previous draft request"); expect(fetcher).toHaveBeenCalledTimes(1);
    await dataClient.retryDocumentDraftBirth("space-a", "page-a", original.operation_id);
    expect(fetcher.mock.calls[1][1].body).toBe(fetcher.mock.calls[0][1].body); expect(records()[0].state).toBe("confirmed");
  });
  for (const status of [401, 403, 404, 409]) it(`keeps a previously uncertain birth held after retry rejection${status}`, async () => {
    const fetcher = vi.fn().mockRejectedValueOnce(new Error("response lost after possible commit")).mockImplementationOnce(async () => new Response(JSON.stringify({ message: status === 409 ? "source_conflict: original result changed" : "Original result unavailable" }), { status })); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("unknown");
    const original = records()[0];
    await expect(dataClient.retryDocumentDraftBirth("space-a", "page-a", original.operation_id)).rejects.toThrow("unknown");
    expect(records()[0]).toMatchObject({ state: "unknown", operation_id: original.operation_id, body: original.body });
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("previous draft request"); expect(fetcher).toHaveBeenCalledTimes(2);
    expect(fetcher.mock.calls[1][1].body).toBe(fetcher.mock.calls[0][1].body);
  });
  it("keeps an uncertain forced birth held when retry returns valid-looking ordinary200", async () => {
    const fetcher = vi.fn().mockRejectedValueOnce(new Error("lost original result")).mockImplementationOnce(async () => new Response(JSON.stringify(result()), { status: 200 })); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("unknown"); const original = records()[0];
    await expect(dataClient.retryDocumentDraftBirth("space-a", "page-a", original.operation_id)).rejects.toThrow("unknown");
    expect(records()[0]).toMatchObject({ state: "unknown", operation_id: original.operation_id, body: original.body });
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("previous draft request"); expect(fetcher).toHaveBeenCalledTimes(2);
  });
  it("does not confirm a first forced request from ordinary200 even with matching operation label", async () => {
    vi.stubGlobal("fetch", vi.fn(async (_url: unknown, init?: RequestInit) => new Response(JSON.stringify(result(requestBody(init).operation_id)), { status: 200 })));
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("unknown"); expect(records()[0].state).toBe("unknown");
  });
  it("allows independent B birth while preserving A unknown and denies B retrying A", async () => {
    const fetcher = vi.fn().mockRejectedValueOnce(new Error("A lost response")).mockImplementationOnce(async (_url: unknown, init?: RequestInit) => new Response(JSON.stringify({ ...result(requestBody(init).operation_id), owner_id: "actor-b", id: "draft-b" }), { status: 201 })); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("unknown");
    const original = JSON.stringify(records()[0]); const operationA = records()[0].operation_id;
    api.setToken(token("actor-b"));
    await expect(dataClient.retryDocumentDraftBirth("space-a", "page-a", operationA)).rejects.toThrow("cannot be retried under this session"); expect(fetcher).toHaveBeenCalledTimes(1);
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).resolves.toMatchObject({ owner_id: "actor-b", id: "draft-b" });
    expect(JSON.stringify(records().find(record => record.operation_id === operationA))).toBe(original);
    const operationB = records().find(record => record.origin.accountId === "actor-b"); expect(operationB.state).toBe("confirmed"); expect(operationB.operation_id).not.toBe(operationA); expect(fetcher).toHaveBeenCalledTimes(2);
  });
  it("refuses dispatch when the original journal cannot be persisted", async () => {
    const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    captureOfflineServerContext(); // Enroll actual session scope before failing the owned journal write.
    const originalSetItem = localStorage.setItem.bind(localStorage);
    vi.spyOn(localStorage, "setItem").mockImplementation((key: string, value: string) => {
      if (key.startsWith("starlane_draft_birth_v1:")) throw new Error("quota");
      originalSetItem(key, value);
    });
    await expect(dataClient.createDocumentDraft("space-a", "page-a", { force_new: true })).rejects.toThrow("Could not retain");
    expect(fetcher).not.toHaveBeenCalled();
  });
  it("does not turn a definite source conflict into a second unknown birth", async () => {
    const fetcher = vi.fn(async () => new Response(JSON.stringify({ message: "source_required: select an editable private draft" }), { status: 409 })); vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.createDocumentDraft("space-a", "page-a")).rejects.toThrow("source_required");
    expect(records()[0].state).toBe("rejected"); expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("copies a confirmed editable private source and refuses viewers or missing digest", () => {
    const draft = result(); expect(selectedDraftFork(draft)).toEqual({ source_draft_id: draft.id, source_digest: draft.source_digest });
    expect(() => selectedDraftFork({ ...draft, access_role: "viewer" })).toThrow(); expect(() => selectedDraftFork({ ...draft, source_digest: undefined })).toThrow();
  });
  it("reports durable prepared identity before dispatch and aborts if caller cannot retain it", async () => {
    const fetcher=vi.fn(async (_url:unknown, init?:RequestInit)=>new Response(JSON.stringify(result(requestBody(init).operation_id)),{status:201})); vi.stubGlobal("fetch",fetcher);
    let associated:string|undefined;
    await dataClient.createDocumentDraft("space-a","page-a",{force_new:true},id=> {associated=id; expect(records()[0]).toMatchObject({operation_id:id,state:"prepared"});expect(fetcher).not.toHaveBeenCalled();});
    expect(associated).toBe(records()[0].operation_id);
    await expect(dataClient.createDocumentDraft("space-a","other-page",{force_new:true},()=>{throw new Error("caller retention failed");})).rejects.toThrow("caller retention failed");
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("retries a confirmed original birth by immutable receipt after caller interruption", async () => {
    const fetcher=vi.fn(async (_url:unknown, init?:RequestInit)=>new Response(JSON.stringify(result(requestBody(init).operation_id)),{status:201})); vi.stubGlobal("fetch",fetcher);
    const first=await dataClient.createDocumentDraft("space-a","page-a",{force_new:true});
    const original=records()[0]; expect(original.state).toBe("confirmed");
    await expect(dataClient.retryDocumentDraftBirth("space-a","page-a",original.operation_id)).resolves.toEqual(first);
    expect(fetcher.mock.calls[1][1]!.body).toBe(fetcher.mock.calls[0][1]!.body); expect(records()).toHaveLength(1);
  });

});
