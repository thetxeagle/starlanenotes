import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cacheServerResponse,
  getOfflineServerWorkspaceStatus,
  OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT,
  hasCachedServerWorkspace,
  listOfflineServerAttachments,
  listOfflineServerMutations,
  queueOfflineServerAttachment,
  queueOfflineServerMutation,
  updateOfflineServerMutation,
  updateOfflineServerAttachment,
  serverCacheScopeForToken,
} from "./offlineServerCache";

function stubLocalStorage() {
  const storage = new Map<string, string>();
  const localStorage = {
    get length() {
      return storage.size;
    },
    key: vi.fn((index: number) => [...storage.keys()][index] ?? null),
    getItem: vi.fn((key: string) => storage.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => storage.set(key, value)),
    removeItem: vi.fn((key: string) => storage.delete(key)),
  };
  vi.stubGlobal("localStorage", localStorage);
  return localStorage;
}

describe("offline server workspace cache", () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    stubLocalStorage();
  });

  it("requires the same signed-in session before an offline workspace can open", () => {
    const server = "https://notes.example.com/api/v1";
    const firstSession = serverCacheScopeForToken("first-session-token");

    cacheServerResponse(server, "/auth/config", { site_name: "Starlane Notes" });
    cacheServerResponse(server, "/auth/me", { id: "user-1" }, firstSession);

    expect(hasCachedServerWorkspace(server, firstSession)).toBe(true);
    expect(hasCachedServerWorkspace(server, serverCacheScopeForToken("other-session-token"))).toBe(
      false,
    );
  });

  it("summarizes cached workspace readiness and queued writes for the linked server only", () => {
    const server = "https://notes.example.com/api/v1";
    const scope = serverCacheScopeForToken("first-session-token");
    cacheServerResponse(server, "/auth/config", { site_name: "Starlane Notes" });
    cacheServerResponse(server, "/auth/me", { id: "user-1" }, scope);
    queueOfflineServerMutation(
      server,
      "/spaces/space-1/folders",
      "POST",
      { name: "Offline" },
      scope,
    );
    queueOfflineServerMutation(
      "https://other.example.com/api/v1",
      "/spaces/space-2/folders",
      "POST",
      {
        name: "Other server",
      },
    );

    expect(getOfflineServerWorkspaceStatus(server, scope)).toEqual({
      ready: true,
      queuedMutationCount: 1,
    });
  });

  it("returns a stable status snapshot until the visible workspace state changes", () => {
    const server = "https://notes.example.com/api/v1";
    const scope = serverCacheScopeForToken("first-session-token");

    const first = getOfflineServerWorkspaceStatus(server, scope);
    const second = getOfflineServerWorkspaceStatus(server, scope);

    expect(second).toBe(first);

    queueOfflineServerMutation(server, "/spaces/space-1/folders", "POST", { name: "Offline" });
    const changed = getOfflineServerWorkspaceStatus(server, scope);

    expect(changed).not.toBe(first);
    expect(changed.queuedMutationCount).toBe(first.queuedMutationCount + 1);
  });

  it("notifies the current app shell when cache or queue state changes", () => {
    const dispatchEvent = vi.fn();
    vi.stubGlobal("window", { dispatchEvent });
    const server = "https://notes.example.com/api/v1";

    cacheServerResponse(server, "/auth/config", { site_name: "Starlane Notes" });
    queueOfflineServerMutation(server, "/spaces/space-1/folders", "POST", { name: "Offline" });

    expect(dispatchEvent).toHaveBeenCalledTimes(2);
    expect(dispatchEvent).toHaveBeenLastCalledWith(
      expect.objectContaining({ type: OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT }),
    );
  });

  it("stores attachment bytes in a durable server-scoped queue", () => {
    const queued = queueOfflineServerAttachment({
      serverBase: "https://notes.example.com/api/v1",
      path: "/spaces/space-1/documents/doc-1/attachments",
      spaceId: "space-1",
      documentId: "doc-1",
      name: "diagram.png",
      mimeType: "image/png",
      dataUrl: "data:image/png;base64,ZmFrZQ==",
    });

    expect(listOfflineServerAttachments("https://notes.example.com/api/v1")).toEqual([queued]);
  });

  it("fails explicitly when local storage is unavailable instead of claiming a queued mutation", () => {
    vi.stubGlobal("localStorage", undefined);

    expect(() =>
      queueOfflineServerMutation(
        "https://notes.example.com/api/v1",
        "/spaces/space-1/documents/doc-1/drafts/draft-1",
        "PUT",
        { title: "Offline edit" },
      ),
    ).toThrow("Offline server mutations could not be queued because local storage is unavailable.");
  });

  it("preserves the existing mutation queue when a quota write fails", () => {
    const localStorage = stubLocalStorage();
    const server = "https://notes.example.com/api/v1";
    const prior = queueOfflineServerMutation(server, "/spaces/space-1/folders", "POST", {
      name: "Already queued",
    });
    localStorage.setItem.mockImplementation(() => {
      throw new DOMException("Quota exceeded", "QuotaExceededError");
    });

    expect(() =>
      queueOfflineServerMutation(server, "/spaces/space-1/documents/doc-1/drafts/draft-1", "PUT", {
        title: "Could not queue",
      }),
    ).toThrow("Quota exceeded");
    expect(listOfflineServerMutations(server)).toEqual([prior]);
  });

  it("surfaces storage access denial without replacing the mutation queue", () => {
    const localStorage = stubLocalStorage();
    const server = "https://notes.example.com/api/v1";
    const prior = queueOfflineServerMutation(server, "/spaces/space-1/folders", "POST", {
      name: "Already queued",
    });
    localStorage.setItem.mockImplementation(() => {
      throw new DOMException("Access denied", "SecurityError");
    });

    expect(() =>
      queueOfflineServerMutation(server, "/spaces/space-1/documents/doc-1/drafts/draft-1", "PUT", {
        title: "Could not queue",
      }),
    ).toThrow("Access denied");
    expect(listOfflineServerMutations(server)).toEqual([prior]);
  });
  it("reads legacy queues and acknowledges individual IDs without rewriting their arrays", async () => {
    const { acknowledgeOfflineServerMutation } = await import("./offlineServerCache");
    const legacy = {
      id: "legacy-1",
      serverBase: "/api/v1",
      path: "/spaces/s/folders",
      method: "POST",
      body: { name: "Legacy" },
      queuedAt: "2026-01-01T00:00:00Z",
      attempts: 0,
    };
    localStorage.setItem("starlane_server_cache_v1:mutation_queue", JSON.stringify([legacy]));
    const added = queueOfflineServerMutation("/api/v1", "/spaces/s/folders", "POST", {
      name: "New",
    });
    expect(listOfflineServerMutations()).toEqual([legacy, added]);
    acknowledgeOfflineServerMutation(legacy.id);
    expect(listOfflineServerMutations()).toEqual([added]);
    expect(JSON.parse(localStorage.getItem("starlane_server_cache_v1:mutation_queue")!)).toEqual([
      legacy,
    ]);
  });

  it("surfaces unavailable attachment storage instead of claiming durable bytes", () => {
    vi.stubGlobal("localStorage", undefined);
    expect(() =>
      queueOfflineServerAttachment({
        serverBase: "/api/v1",
        path: "/attachments",
        spaceId: "s",
        documentId: "d",
        name: "a",
        mimeType: "text/plain",
        dataUrl: "data:text/plain,a",
      }),
    ).toThrow("local storage is unavailable");
  });
  it("preserves sequential mutation order with a frozen clock and reversed Storage keys", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
    const target = stubLocalStorage();
    const originalKey = target.key.getMockImplementation()!;
    target.key.mockImplementation((index) => originalKey(target.length - 1 - index));
    const create = queueOfflineServerMutation("/api/v1", "/spaces/s/documents", "POST", {
      source_id: "a",
      content: "Create",
    });
    const update = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", {
      content: "Update",
    });
    expect(create.queuedAt).toBe(update.queuedAt);
    expect(update.queueOrder).toBeGreaterThan(create.queueOrder!);
    expect(listOfflineServerMutations()).toEqual([create, update]);
    vi.useRealTimers();
  });

  it("preserves attachment insertion order independent of Storage key enumeration", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
    const target = stubLocalStorage();
    const originalKey = target.key.getMockImplementation()!;
    target.key.mockImplementation((index) => originalKey(target.length - 1 - index));
    const input = {
      serverBase: "/api/v1",
      path: "/attachments",
      spaceId: "s",
      documentId: "a",
      name: "a.txt",
      mimeType: "text/plain",
      dataUrl: "data:text/plain,a",
    };
    const first = queueOfflineServerAttachment(input);
    const second = queueOfflineServerAttachment({ ...input, name: "b.txt" });
    expect(first.queuedAt).toBe(second.queuedAt);
    expect(second.queueOrder).toBeGreaterThan(first.queueOrder!);
    expect(listOfflineServerAttachments()).toEqual([first, second]);
    vi.useRealTimers();
  });

  it("retains the original dispatched UUID when preserving an unknown outcome",()=>{const id=crypto.randomUUID();const input={serverBase:"/api/v1",path:"/spaces/s/documents/a/attachments",spaceId:"s",documentId:"a",name:"same.bin",mimeType:"application/octet-stream",dataUrl:"data:application/octet-stream;base64,QQ==",uploadOperationId:id,replayState:"unknown" as const};expect(queueOfflineServerAttachment(input).uploadOperationId).toBe(id);expect(listOfflineServerAttachments()[0].uploadOperationId).toBe(id);expect(()=>queueOfflineServerAttachment({...input,uploadOperationId:"not-an-operation"})).toThrow("identity is invalid");});
  it("persists caller operation identity at enqueue and forbids retry rebinding", () => {
    const queued=queueOfflineServerAttachment({serverBase:"/api/v1",path:"/spaces/s/documents/a/attachments",spaceId:"s",documentId:"a",name:"same.bin",mimeType:"application/octet-stream",dataUrl:"data:application/octet-stream;base64,QQ=="});
    expect(queued.uploadOperationId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    expect(listOfflineServerAttachments()[0].uploadOperationId).toBe(queued.uploadOperationId);
    updateOfflineServerAttachment({...queued,replayState:"unknown"},queued);
    expect(listOfflineServerAttachments()[0].uploadOperationId).toBe(queued.uploadOperationId);
    expect(()=>updateOfflineServerAttachment({...queued,uploadOperationId:crypto.randomUUID()})).toThrow("identity cannot change");
  });

  it("preserves legacy dependency order despite nonmonotonic timestamps and shuffled keys", async () => {
    const target = stubLocalStorage();
    const originalKey = target.key.getMockImplementation()!;
    target.key.mockImplementation((index) => originalKey(target.length - 1 - index));
    const first = {
      id: "legacy-create",
      serverBase: "/api/v1",
      path: "/spaces/s/documents",
      method: "POST",
      body: { source_id: "a" },
      queuedAt: "2026-01-02T00:00:00Z",
      attempts: 0,
    };
    const second = {
      ...first,
      id: "legacy-update",
      path: "/spaces/s/documents/a/content",
      method: "PUT",
      queuedAt: "2026-01-01T00:00:00Z",
    };
    target.setItem("starlane_server_cache_v1:mutation_queue", JSON.stringify([first, second]));
    const newer = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", {
      content: "New",
    });
    expect(listOfflineServerMutations()).toEqual([first, second, newer]);
    const { updateOfflineServerMutation } = await import("./offlineServerCache");
    updateOfflineServerMutation({ ...listOfflineServerMutations()[0], attempts: 1 });
    expect(listOfflineServerMutations().map((item) => item.id)).toEqual([
      first.id,
      second.id,
      newer.id,
    ]);
  });
  it("uses a persisted maximum when a fresh tab enqueues another mutation", async () => {
    const first = queueOfflineServerMutation("/api/v1", "/spaces/s/documents", "POST", {
      source_id: "a",
    });
    vi.resetModules();
    const otherTab = await import("./offlineServerCache");
    const second = otherTab.queueOfflineServerMutation(
      "/api/v1",
      "/spaces/s/documents/a/content",
      "PUT",
      { content: "Next" },
    );
    expect(second.queueOrder).toBeGreaterThan(first.queueOrder!);
    expect(otherTab.listOfflineServerMutations()).toEqual([first, second]);
  });

  it("breaks concurrent order ties deterministically across shuffled storage reads", () => {
    const target = stubLocalStorage();
    const first = {
      id: "tie-b",
      serverBase: "/api/v1",
      path: "/spaces/s/folders",
      method: "POST",
      queuedAt: "2026-01-01T00:00:00Z",
      queueOrder: 1,
      attempts: 0,
    };
    const second = { ...first, id: "tie-a" };
    target.setItem("starlane_server_cache_v1:mutation_queue:item:tie-b", JSON.stringify(first));
    target.setItem("starlane_server_cache_v1:mutation_queue:item:tie-a", JSON.stringify(second));
    expect(listOfflineServerMutations().map((item) => item.id)).toEqual(["tie-a", "tie-b"]);
    const originalKey = target.key.getMockImplementation()!;
    target.key.mockImplementation((index) => originalKey(target.length - 1 - index));
    expect(listOfflineServerMutations().map((item) => item.id)).toEqual(["tie-a", "tie-b"]);
  });
  it("rejects immutable origin replacement and stale acknowledgement without dropping current bytes", async () => {
    const origin = {
      version: 1 as const,
      serverBase: "/api/v1",
      accountId: "a",
      sessionId: "session-a",
      cacheScope: "session-partition",
      storageMode: "server" as const,
      provider: "none" as const,
    };
    const item = queueOfflineServerMutation(
      "/api/v1",
      "/spaces/s/documents/a/content",
      "PUT",
      { content: "Original" },
      origin.cacheScope,
      origin,
    );
    expect(() =>
      updateOfflineServerMutation({ ...item, origin: { ...origin, accountId: "b" } }),
    ).toThrow("origin changed");
    const changed = { ...item, body: { content: "New current bytes" } };
    updateOfflineServerMutation(changed, item);
    const { acknowledgeOfflineServerMutation } = await import("./offlineServerCache");
    expect(() => acknowledgeOfflineServerMutation(item.id, item)).toThrow(
      "changed before acknowledgement",
    );
    expect(listOfflineServerMutations()).toEqual([changed]);
  });

  it("acknowledges one original-context mutation without deleting a later enqueue", async () => {
    const origin = {
      version: 1 as const,
      serverBase: "/api/v1",
      accountId: "a",
      sessionId: "session-a",
      cacheScope: "session-partition",
      storageMode: "server" as const,
      provider: "none" as const,
    };
    const first = queueOfflineServerMutation(
      "/api/v1",
      "/spaces/s/documents/a/content",
      "PUT",
      { content: "A" },
      origin.cacheScope,
      origin,
    );
    const second = queueOfflineServerMutation(
      "/api/v1",
      "/spaces/s/documents/a/content",
      "PUT",
      { content: "B" },
      origin.cacheScope,
      origin,
    );
    const { acknowledgeOfflineServerMutation } = await import("./offlineServerCache");
    acknowledgeOfflineServerMutation(first.id, first);
    expect(listOfflineServerMutations()).toEqual([second]);
  });

  it("retains concurrent attachment bytes when an older source acknowledgement is stale", async () => {
    const origin = {
      version: 1 as const,
      serverBase: "/api/v1",
      accountId: "a",
      sessionId: "session-a",
      cacheScope: "session-partition",
      storageMode: "server" as const,
      provider: "none" as const,
    };
    const input = {
      serverBase: "/api/v1",
      path: "/spaces/s/documents/a/attachments",
      spaceId: "s",
      documentId: "a",
      name: "a.txt",
      mimeType: "text/plain",
      dataUrl: "data:text/plain,A",
      origin,
    };
    const first = queueOfflineServerAttachment(input);
    const second = queueOfflineServerAttachment({
      ...input,
      name: "b.txt",
      dataUrl: "data:text/plain,B",
    });
    const { updateOfflineServerAttachment, acknowledgeOfflineServerAttachment } =
      await import("./offlineServerCache");
    const failed = { ...first, attempts: 1, lastError: "Review required" };
    updateOfflineServerAttachment(failed, first);
    expect(() => acknowledgeOfflineServerAttachment(first.id, first)).toThrow(
      "changed before acknowledgement",
    );
    expect(listOfflineServerAttachments()).toEqual([failed, second]);
    expect(() =>
      updateOfflineServerAttachment({ ...failed, dataUrl: "data:text/plain,Other" }, failed),
    ).toThrow("identity changed");
    expect(listOfflineServerAttachments()).toEqual([failed, second]);
    acknowledgeOfflineServerAttachment(first.id, failed);
    expect(listOfflineServerAttachments()).toEqual([second]);
  });

  it("retains dispatched evidence separately from newer queued payloads and cached workspace", async () => {
    const origin = {
      version: 1 as const,
      serverBase: "/api/v1",
      accountId: "a",
      sessionId: "session-a",
      cacheScope: "session-partition",
      storageMode: "server" as const,
      provider: "none" as const,
    };
    const path = "/spaces/s/documents/a/content";
    cacheServerResponse("/api/v1", path, { content: "Baseline" }, origin.cacheScope);
    const original = queueOfflineServerMutation(
      "/api/v1",
      path,
      "PUT",
      { content: "Dispatched A" },
      origin.cacheScope,
      origin,
    );
    const newer = { ...original, body: { content: "Current B" } };
    updateOfflineServerMutation(newer, original);
    cacheServerResponse("/api/v1", path, { content: "Current B" }, origin.cacheScope);
    const { retainUnknownOfflineServerMutation, readCachedServerResponse } =
      await import("./offlineServerCache");
    retainUnknownOfflineServerMutation(original, "Original response authority changed");
    const queue = listOfflineServerMutations();
    expect(queue[0]).toEqual(newer);
    expect(queue[1]).toMatchObject({
      body: original.body,
      origin,
      replayState: "unknown",
      lastError: "Original response authority changed",
    });
    expect(queue[1].id).not.toBe(original.id);
    expect(queue[1].queueOrder).toBeGreaterThan(original.queueOrder!);
    expect(readCachedServerResponse("/api/v1", path, origin.cacheScope)).toEqual({
      content: "Current B",
    });
  });
});

describe("queued attachment original draft scope",()=>{
 beforeEach(()=>{vi.unstubAllGlobals();stubLocalStorage();});
 const draft="11111111-1111-4111-8111-111111111111",other="22222222-2222-4222-8222-222222222222";
 const input={serverBase:"/api/v1",path:"/spaces/s/documents/p/attachments",spaceId:"s",documentId:"p",name:"original.txt",mimeType:"text/plain",dataUrl:"data:text/plain;base64,QQ=="};
 it("leaves legacy serialized payload unscoped and retains new scope across reload",()=>{const old=queueOfflineServerAttachment(input),current=queueOfflineServerAttachment({...input,scopeVersion:2,draftId:draft});expect(Object.prototype.hasOwnProperty.call(old,"draftId")).toBe(false);expect(Object.prototype.hasOwnProperty.call(old,"scopeVersion")).toBe(false);expect(listOfflineServerAttachments()).toEqual([old,current]);expect(()=>updateOfflineServerAttachment({...current,draftId:other},current)).toThrow("identity changed");expect(listOfflineServerAttachments()).toEqual([old,current]);});
 it("rejects a new partial or malformed draft scope before enqueue",()=>{expect(()=>queueOfflineServerAttachment({...input,draftId:draft})).toThrow("draft scope invalid");expect(()=>queueOfflineServerAttachment({...input,scopeVersion:2,draftId:"bad"})).toThrow("draft scope invalid");expect(listOfflineServerAttachments()).toEqual([]);});
});
