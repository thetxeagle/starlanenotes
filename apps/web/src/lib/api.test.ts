import { setCurrentLaunchMode } from "./launchMode";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api, ApiError, isApiError, replayOfflineServerMutations, captureOfflineServerContext } from "./api";
import {
  listOfflineServerAttachments,
  listOfflineServerMutations,
  queueOfflineServerAttachment as rawQueueAttachment,
  queueOfflineServerMutation as rawQueueMutation,
  updateOfflineServerMutation,
  cacheServerResponse,
  readCachedServerResponse,
} from "./offlineServerCache";

const sourceBoundAckFields = { title:"Original", document_title:"Published", owner_id:"actor-a", owner_name:"Actor", access_role:"owner", can_publish:true, created_at:"created", updated_at:"updated", base_updated_at:"base" };
function replayToken(sub = "actor-a", sid = "session-a", stamp = 1) {
  return `e30.${btoa(JSON.stringify({ sub, sid, token_type: "access", exp: 9999999999, stamp }))}.fixture`;
}
function queueOfflineServerMutation(...args: Parameters<typeof rawQueueMutation>) {
  if (!api.getToken()) api.setToken(replayToken());
  const origin = captureOfflineServerContext().origin;
  return rawQueueMutation(args[0], args[1], args[2], args[3], origin.cacheScope, { ...origin, serverBase: args[0].replace(/\/+$/, "") });
}
function queueOfflineServerAttachment(input: Parameters<typeof rawQueueAttachment>[0]) {
  if (!api.getToken()) api.setToken(replayToken());
  const origin = captureOfflineServerContext().origin;
  return rawQueueAttachment({ ...input, origin: { ...origin, serverBase: input.serverBase.replace(/\/+$/, "") } });
}
function stubLocalStorage() {
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    get length() {
      return storage.size;
    },
    key: vi.fn((index: number) => [...storage.keys()][index] ?? null),
    getItem: vi.fn((key: string) => storage.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      storage.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      storage.delete(key);
    }),
  });
  vi.stubGlobal("window", { localStorage: globalThis.localStorage });
}

describe("api error envelopes", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    stubLocalStorage();
    api.logout();
    setCurrentLaunchMode("server");
    vi.stubGlobal("navigator", { locks: { request: vi.fn((_name: string, run: () => Promise<unknown>) => run()) } });
  });

  it("accepts committed storage-pending creation without dropping receipt IDs or queuing a retry", async () => {
    api.setToken("current-token");
    const payload = { id: "saved-page", draft_id: "saved-draft", op_id: "storage-op", storage_pending: true };
    const fetchMock = vi.fn(async () => new Response(JSON.stringify(payload), { status: 202, headers: { "Content-Type": "application/json" } }));
    vi.stubGlobal("fetch", fetchMock);
    await expect(api.fetch("/spaces/space-1/documents", { method: "POST", body: { source_id: "saved-page", title: "Saved" } })).resolves.toEqual(payload);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(listOfflineServerMutations()).toHaveLength(0);
  });

  for (const status of [201, 202, 204]) {
    it(`retains a draft save as unknown when its exact200 contract receives ${status}`, async () => {
      api.setToken(replayToken());
      const origin = captureOfflineServerContext().origin;
      const body = { content: "Original draft bytes", source_digest: "a".repeat(64) };
      const response = new Response(status === 204 ? null : JSON.stringify({ ...sourceBoundAckFields, id: "d", document_id: "a", space_id: "s", source_digest: "b".repeat(64) }), { status });
      const request = vi.fn(async () => response); vi.stubGlobal("fetch", request);
      await expect(api.fetch("/spaces/s/documents/a/drafts/d", { method: "PUT", body, expectedStatuses: [200] })).rejects.toMatchObject({ status, code: "outcome_unproven" });
      expect(request).toHaveBeenCalledTimes(1);
      expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ body, replayState: "unknown", origin })]);
      expect(readCachedServerResponse("/api/v1", "/spaces/s/documents/a/drafts/d", origin.cacheScope)).toBeNull();
    });
  }
  it("accepts exact200 draft outcome and preserves copied status admission through a held refresh", async () => {
    api.setToken(replayToken()); api.setRefreshToken("refresh-a");
    const statuses = [200];
    let release!: (response: Response) => void; let reached!: () => void;
    const waiting = new Promise<void>(resolve => { reached = resolve; });
    const request = vi.fn(async (url: string): Promise<Response> => {
      if (url.endsWith("/auth/refresh")) { reached(); return new Promise(resolve => { release = resolve; }); }
      if (request.mock.calls.length === 1) return new Response(JSON.stringify({ message: "Invalid or expired token" }), { status: 401 });
      return new Response(JSON.stringify({ id: "d", source_digest: "b".repeat(64) }), { status: 202 });
    }); vi.stubGlobal("fetch", request);
    const pending = api.fetch("/spaces/s/documents/a/drafts/d", { method: "PUT", body: { content: "Original" }, expectedStatuses: statuses });
    await waiting; statuses.push(202);
    release(new Response(JSON.stringify({ access_token: replayToken("actor-a", "session-a", 2), refresh_token: "refresh-a-2" }), { status: 200 }));
    await expect(pending).rejects.toMatchObject({ status: 202, code: "outcome_unproven" });
    expect(request).toHaveBeenCalledTimes(3); expect(listOfflineServerMutations()).toHaveLength(1);
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ source_digest: "c".repeat(64) }), { status: 200 })));
    await expect(api.fetch("/spaces/s/documents/other/drafts/other", { method: "PUT", body: { content: "Known" }, expectedStatuses: [200] })).resolves.toEqual({ source_digest: "c".repeat(64) });
    expect(listOfflineServerMutations()).toHaveLength(1);
  });

  it("preserves structured server error codes while keeping message compatibility", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              error: "2fa_required",
              message: "2FA code required",
            }),
            {
              status: 403,
              headers: { "Content-Type": "application/json" },
            },
          ),
      ),
    );

    await expect(api.fetch("/auth/login", { method: "POST" })).rejects.toMatchObject({
      name: "ApiError",
      message: "2FA code required",
      status: 403,
      code: "2fa_required",
    });
  });

  it("does not refresh or redirect public login failures even when stale tokens exist", async () => {
    api.setToken("stale-access-token");
    api.setRefreshToken("stale-refresh-token");
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) =>
        new Response(
          JSON.stringify({
            error: "unauthorized",
            message: "Invalid credentials",
          }),
          {
            status: 401,
            headers: { "Content-Type": "application/json" },
          },
        ),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(api.fetch("/auth/login", { method: "POST" })).rejects.toMatchObject({
      name: "ApiError",
      message: "Invalid credentials",
      status: 401,
      code: "unauthorized",
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0]?.[1]?.headers).not.toHaveProperty("Authorization");
  });

  it("refreshes expired bearer sessions for protected requests and retries once", async () => {
    api.setToken("expired-access-token");
    api.setRefreshToken("valid-refresh-token");
    const fetchMock = vi
      .fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response())
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            error: "unauthorized",
            message: "Invalid or expired token",
          }),
          {
            status: 401,
            headers: { "Content-Type": "application/json" },
          },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            access_token: "fresh-access-token",
            refresh_token: "fresh-refresh-token",
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          },
        ),
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ id: "user-1" }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
      );
    vi.stubGlobal("fetch", fetchMock);

    await expect(api.fetch("/auth/me")).resolves.toEqual({ id: "user-1" });

    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(fetchMock.mock.calls[0]?.[1]?.headers).toMatchObject({
      Authorization: "Bearer expired-access-token",
    });
    expect(fetchMock.mock.calls[1]?.[0]).toBe("/api/v1/auth/refresh");
    expect(fetchMock.mock.calls[2]?.[1]?.headers).toMatchObject({
      Authorization: "Bearer fresh-access-token",
    });
  });

  it("shares one rotating refresh across concurrent expired-token requests", async () => {
    api.setToken("expired-access-token");
    api.setRefreshToken("valid-refresh-token");
    let refreshCalls = 0;
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input).endsWith("/auth/refresh")) {
        refreshCalls += 1;
        await Promise.resolve();
        return new Response(
          JSON.stringify({
            access_token: "fresh-access-token",
            refresh_token: "fresh-refresh-token",
          }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        );
      }
      const authorization = (init?.headers as Record<string, string> | undefined)?.Authorization;
      if (authorization === "Bearer expired-access-token") {
        return new Response(JSON.stringify({ message: "Invalid or expired token" }), {
          status: 401,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(Promise.all([api.fetch("/spaces"), api.fetch("/auth/me")])).resolves.toEqual([
      { ok: true },
      { ok: true },
    ]);

    expect(refreshCalls).toBe(1);
    expect(api.getToken()).toBe("fresh-access-token");
    expect(api.getRefreshToken()).toBe("fresh-refresh-token");
  });

  it("deduplicates concurrent server reads during route transitions", async () => {
    let resolveRequest: ((response: Response) => void) | undefined;
    const fetchMock = vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          resolveRequest = resolve;
        }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const first = api.fetch<{ documents: string[] }>("/spaces/space-1/documents");
    const second = api.fetch<{ documents: string[] }>("/spaces/space-1/documents");

    expect(fetchMock).toHaveBeenCalledTimes(1);
    resolveRequest?.(
      new Response(JSON.stringify({ documents: ["doc-1"] }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    await expect(Promise.all([first, second])).resolves.toEqual([
      { documents: ["doc-1"] },
      { documents: ["doc-1"] },
    ]);
  });

  it("keeps app credentials when refresh is temporarily unavailable", async () => {
    api.setToken("expired-access-token");
    api.setRefreshToken("six-month-refresh-token");
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValueOnce(
          new Response(JSON.stringify({ message: "Invalid or expired token" }), {
            status: 401,
            headers: { "Content-Type": "application/json" },
          }),
        )
        .mockResolvedValueOnce(
          new Response(JSON.stringify({ message: "Try again later" }), {
            status: 503,
            headers: { "Content-Type": "application/json" },
          }),
        ),
    );

    await expect(api.fetch("/auth/me")).rejects.toMatchObject({
      status: 503,
      code: "session_refresh_unavailable",
    });
    expect(api.getToken()).toBe("expired-access-token");
    expect(api.getRefreshToken()).toBe("six-month-refresh-token");
  });

  it("clears credentials when the server explicitly rejects the refresh session", async () => {
    const location = { href: "/workspace" };
    Object.assign(window, { location });
    api.setToken("expired-access-token");
    api.setRefreshToken("revoked-refresh-token");
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValueOnce(
          new Response(JSON.stringify({ message: "Invalid or expired token" }), {
            status: 401,
            headers: { "Content-Type": "application/json" },
          }),
        )
        .mockResolvedValueOnce(
          new Response(JSON.stringify({ message: "Refresh session is no longer active" }), {
            status: 401,
            headers: { "Content-Type": "application/json" },
          }),
        ),
    );

    await expect(api.fetch("/auth/me")).rejects.toMatchObject({
      status: 401,
      code: "session_expired",
    });
    expect(api.getToken()).toBeNull();
    expect(api.getRefreshToken()).toBeNull();
    expect(location.href).toBe("/login");
    expect(localStorage.getItem("starlane_server_cache_v1:session-scope")).toBeNull();
  });

  it("identifies ApiError instances without matching generic errors", () => {
    expect(isApiError(new ApiError("Nope", 403, "forbidden"))).toBe(true);
    expect(isApiError(new Error("Nope"))).toBe(false);
  });

  it("uses a downloaded server response when the network becomes unavailable", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValueOnce(
          new Response(JSON.stringify({ id: "doc-1", content: "Downloaded" }), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }),
        )
        .mockRejectedValueOnce(new TypeError("Load failed")),
    );

    await expect(api.fetch("/spaces/space-1/documents/doc-1/content")).resolves.toEqual({
      id: "doc-1",
      content: "Downloaded",
    });
    await expect(api.fetch("/spaces/space-1/documents/doc-1/content")).resolves.toEqual({
      id: "doc-1",
      content: "Downloaded",
    });
  });

  it("refreshes the downloaded device copy after a successful online document edit", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValueOnce(
          new Response(JSON.stringify({ id: "doc-1", content: "Before edit" }), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }),
        )
        .mockResolvedValueOnce(new Response(null, { status: 204 }))
        .mockRejectedValueOnce(new TypeError("Load failed")),
    );

    await api.fetch("/spaces/space-1/documents/doc-1/content");
    await api.fetch("/spaces/space-1/documents/doc-1/content", {
      method: "PUT",
      body: { content: "Saved on device" },
    });

    await expect(api.fetch("/spaces/space-1/documents/doc-1/content")).resolves.toMatchObject({
      id: "doc-1",
      content: "Saved on device",
    });
  });

  it("queues offline document updates and replays them when the server returns", async () => {
    api.setToken(replayToken());
    Object.assign(navigator, { onLine: false });
    vi.stubGlobal("fetch", vi.fn().mockRejectedValueOnce(new TypeError("Load failed")));

    await expect(
      api.fetch("/spaces/space-1/documents/doc-1/content", {
        method: "PUT",
        body: { content: "Offline draft" },
      }),
    ).resolves.toBeUndefined();

    expect(listOfflineServerMutations("/api/v1")).toEqual([
      expect.objectContaining({
        path: "/spaces/space-1/documents/doc-1/content",
        method: "PUT",
        body: { content: "Offline draft" },
      }),
    ]);

    Object.assign(navigator, { onLine: true });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status: 204 })));
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 1,
      pending: 0,
      blocked: 0,
    });
    expect(listOfflineServerMutations("/api/v1")).toEqual([]);
  });

  it("replays an offline owned-draft update with the exact title and portable body", async () => {
    api.setToken(replayToken());
    Object.assign(navigator, { onLine: false });
    const path = "/spaces/space-1/documents/doc-1/drafts/draft-1";
    const body = {
      title: "Recovered briefing",
      content: "# Heading\n\n[[Same title|stable-id]]\n",
    };
    vi.stubGlobal("fetch", vi.fn().mockRejectedValueOnce(new TypeError("Load failed")));
    await expect(api.fetch(path, { method: "PUT", body })).resolves.toBeUndefined();
    expect(listOfflineServerMutations("/api/v1")).toEqual([
      expect.objectContaining({ path, method: "PUT", body }),
    ]);
    Object.assign(navigator, { onLine: true });
    const replayFetch = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", replayFetch);
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 1,
      pending: 0,
      blocked: 0,
    });
    expect(replayFetch).toHaveBeenCalledWith(
      `/api/v1${path}`,
      expect.objectContaining({ method: "PUT", body: JSON.stringify(body) }),
    );
    expect(listOfflineServerMutations("/api/v1")).toEqual([]);
  });

  it("rejects an offline publish request without queuing a later publication", async () => {
    api.setToken(replayToken());
    vi.stubGlobal("fetch", vi.fn().mockRejectedValueOnce(new TypeError("Load failed")));
    await expect(
      api.fetch("/spaces/space-1/documents/doc-1/drafts/draft-1/publish", { method: "POST" }),
    ).rejects.toThrow("Load failed");
    expect(listOfflineServerMutations("/api/v1")).toEqual([]);
  });

  it("accepts successful empty responses beyond 204", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status: 201 })));

    await expect(
      api.fetch("/spaces/space-1/documents/doc-1/tags", { method: "POST" }),
    ).resolves.toBeUndefined();
  });

  it("retains a missing queued draft for explicit recovery without creating another draft", async () => {
    const item = queueOfflineServerMutation("/api/v1", "/spaces/space-1/documents/doc-1/drafts/stale", "PUT", { content: "Exact recovered bytes" });
    const request = vi.fn(async () => new Response(null, { status: 404 })); vi.stubGlobal("fetch", request);
    await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 0, pending: 1, blocked: 1 });
    expect(request).toHaveBeenCalledTimes(1);
    expect(listOfflineServerMutations()).toEqual([{ ...item, attempts: 1, lastError: "Server returned 404; original intent requires review" }]);
  });

  it("retains a source-ID create conflict without claiming lost-ACK equality", async () => {
    api.setToken(replayToken());
    Object.assign(navigator, { onLine: false });
    vi.stubGlobal("fetch", vi.fn().mockRejectedValueOnce(new TypeError("Load failed")));

    await expect(
      api.fetch("/spaces/space-1/documents", {
        method: "POST",
        body: { source_id: "doc-offline-1", title: "Offline page", content: "Draft" },
      }),
    ).resolves.toBeUndefined();
    expect(listOfflineServerMutations("/api/v1")).toEqual([
      expect.objectContaining({ method: "POST", path: "/spaces/space-1/documents" }),
    ]);

    Object.assign(navigator, { onLine: true });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status: 409 })));
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 0,
      pending: 1,
      blocked: 1,
    });
  });

  it("replays queued attachment bytes before replacing their data URLs in document writes", async () => {
    api.setToken(replayToken());
    const dataUrl = "data:text/plain;base64,SGVsbG8=";
    const queuedUpload=queueOfflineServerAttachment({
      serverBase: "/api/v1",
      path: "/spaces/space-1/documents/doc-1/attachments",
      spaceId: "space-1",
      documentId: "doc-1",
      name: "notes.txt",
      mimeType: "text/plain",
      dataUrl,
    });
    queueOfflineServerMutation("/api/v1", "/spaces/space-1/documents/doc-1/content", "PUT", {
      content: `![Notes](${dataUrl})`,
    });

    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input).endsWith("/attachments")) {
        expect(init?.body).toBeInstanceOf(FormData);
        expect(new Headers(init?.headers).get("X-Starlane-Upload-Operation")).toBe(queuedUpload.uploadOperationId);
        return new Response(JSON.stringify({ url: "/api/v1/files/space-1/notes.txt" }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      expect(JSON.parse(String(init?.body))).toEqual({
        content: "![Notes](/api/v1/files/space-1/notes.txt)",
      });
      return new Response(null, { status: 204 });
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 2,
      pending: 0,
      blocked: 0,
    });
    expect(listOfflineServerAttachments("/api/v1")).toEqual([]);
    expect(listOfflineServerMutations("/api/v1")).toEqual([]);
  });
  it("preserves a new mutation enqueued while an earlier replay awaits the server", async () => {
    const first = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", {
      content: "A",
    });
    let finish!: (value: Response) => void;
    const fetchMock = vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          finish = resolve;
        }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const replay = replayOfflineServerMutations();
    const second = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", {
      content: "B",
    });
    finish(new Response(null, { status: 204 }));
    await expect(replay).resolves.toEqual({ applied: 1, pending: 1, blocked: 0 });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(listOfflineServerMutations()).toEqual([second]);
    expect(second.id).not.toBe(first.id);
  });

  it("coalesces overlapping replay calls without submitting an item twice", async () => {
    queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: "A" });
    let finish!: (value: Response) => void;
    const fetchMock = vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          finish = resolve;
        }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const first = replayOfflineServerMutations("/api/v1/");
    const second = replayOfflineServerMutations("/api/v1");
    expect(first).toBe(second);
    finish(new Response(null, { status: 204 }));
    await expect(first).resolves.toEqual({ applied: 1, pending: 0, blocked: 0 });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("preserves exact failed payloads and errors alongside concurrent new work", async () => {
    const failed = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", {
      title: "Keep",
      content: "Failed bytes",
    });
    const later = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/b/content", "PUT", {
      content: "Success",
    });
    let added: ReturnType<typeof queueOfflineServerMutation>;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input) => {
        if (String(input).includes("/a/")) {
          added = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/c/content", "PUT", {
            content: "New",
          });
          return new Response(null, { status: 422 });
        }
        return new Response(null, { status: 204 });
      }),
    );
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 0,
      pending: 3,
      blocked: 3,
    });
    expect(listOfflineServerMutations()).toEqual([
      { ...failed, attempts: 1, lastError: "Server returned 422; original intent requires review" },
      later,
      added!,
    ]);
  });

  it("counts unreachable retained mutations as blocked", async () => {
    const item = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", {
      content: "Keep",
    });
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Offline again")));
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 0,
      pending: 1,
      blocked: 1,
    });
    expect(listOfflineServerMutations()).toEqual([
      { ...item, replayState: "unknown", attempts: 1, lastError: "Offline again; server outcome unknown; explicit review required" },
    ]);
  });

  it("preserves attachments enqueued during upload and other-server reference writes", async () => {
    const input = {
      serverBase: "/api/v1",
      path: "/spaces/s/documents/a/attachments",
      spaceId: "s",
      documentId: "a",
      name: "a.txt",
      mimeType: "text/plain",
      dataUrl: "data:text/plain;base64,QQ==",
    };
    queueOfflineServerAttachment(input);
    const foreign = queueOfflineServerMutation(
      "https://other/api/v1",
      "/spaces/s/documents/a/content",
      "PUT",
      { content: input.dataUrl },
    );
    let added: ReturnType<typeof queueOfflineServerAttachment>;
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockImplementationOnce(async () => {
          added = queueOfflineServerAttachment({
            ...input,
            name: "b.txt",
            dataUrl: "data:text/plain;base64,Qg==",
          });
          queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", {
            content: input.dataUrl,
          });
          return new Response(JSON.stringify({ url: "/files/a.txt" }), { status: 200 });
        })
        .mockImplementationOnce(async (_input, init) => {
          expect(JSON.parse(String(init.body))).toEqual({ content: "/files/a.txt" });
          return new Response(null, { status: 204 });
        }),
    );
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 2,
      pending: 1,
      blocked: 0,
    });
    expect(listOfflineServerAttachments()).toEqual([added!]);
    expect(listOfflineServerMutations()).toEqual([foreign]);
  });

  it("retains uploaded bytes when the server omits the replacement URL", async () => {
    const item = queueOfflineServerAttachment({
      serverBase: "/api/v1",
      path: "/spaces/s/documents/a/attachments",
      spaceId: "s",
      documentId: "a",
      name: "a.txt",
      mimeType: "text/plain",
      dataUrl: "data:text/plain;base64,QQ==",
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 200 })));
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 0,
      pending: 1,
      blocked: 1,
    });
    expect(listOfflineServerAttachments()).toEqual([
      { ...item, replayState: "unknown", attempts: 1, lastError: "Attachment upload response did not include a URL; server outcome unknown; explicit review required" },
    ]);
  });

  it("serializes separate tab clients with the origin Web Lock", async () => {
    let tail: Promise<unknown> = Promise.resolve();
    const request = vi.fn((_name: string, run: () => Promise<unknown>) => {
      const next = tail.then(run);
      tail = next.catch(() => {});
      return next;
    });
    vi.stubGlobal("navigator", { locks: { request } });
    queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: "A" });
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    // A fresh module has its own in-realm map, just as a second tab does.
    vi.resetModules();
    const otherTab = await import("./api");
    const results = await Promise.all([
      replayOfflineServerMutations(),
      otherTab.replayOfflineServerMutations(),
    ]);
    expect(results.map((result) => result.applied).sort()).toEqual([0, 1]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(request.mock.calls.map(([name]) => name)).toEqual([
      "starlane:offline-replay:/api/v1",
      "starlane:offline-replay:/api/v1",
    ]);
  });

  it("retains browser work with an actionable error when Web Locks are unavailable", async () => {
    const item = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", {
      content: "Keep",
    });
    vi.stubGlobal("window", {});
    vi.stubGlobal("navigator", {});
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 0,
      pending: 1,
      blocked: 1,
    });
    expect(listOfflineServerMutations()).toEqual([
      { ...item, lastError: expect.stringContaining("Offline replay requires Web Locks") },
    ]);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("retains a failed upload and a concurrently enqueued attachment with exact bytes", async () => {
    const input = {
      serverBase: "/api/v1",
      path: "/spaces/s/documents/a/attachments",
      spaceId: "s",
      documentId: "a",
      name: "a.txt",
      mimeType: "text/plain",
      dataUrl: "data:text/plain;base64,QQ==",
    };
    const failed = queueOfflineServerAttachment(input);
    let added: ReturnType<typeof queueOfflineServerAttachment>;
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        added = queueOfflineServerAttachment({ ...input, name: "b.txt" });
        return new Response(null, { status: 403 });
      }),
    );
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 0,
      pending: 2,
      blocked: 2,
    });
    expect(listOfflineServerAttachments()).toEqual([
      { ...failed, attempts: 1, lastError: "Server returned 403" },
      added!,
    ]);
  });

  it("reports an empty browser queue without requiring Web Locks", async () => {
    vi.stubGlobal("window", {});
    vi.stubGlobal("navigator", {});
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 0,
      pending: 0,
      blocked: 0,
    });
  });
  it("replays create before update despite equal clocks and reversed Storage keys", async () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
      const keyMock = vi.mocked(localStorage.key);
      const originalKey = keyMock.getMockImplementation()!;
      keyMock.mockImplementation((index) => originalKey(localStorage.length - 1 - index));
      queueOfflineServerMutation("/api/v1", "/spaces/s/documents", "POST", { source_id: "a" });
      queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", {
        content: "Next",
      });
      const fetchMock = vi.fn(
        async (_input: RequestInfo | URL) => new Response(null, { status: 204 }),
      );
      vi.stubGlobal("fetch", fetchMock);
      await expect(replayOfflineServerMutations()).resolves.toEqual({
        applied: 2,
        pending: 0,
        blocked: 0,
      });
      expect(fetchMock.mock.calls.map(([input]) => input)).toEqual([
        "/api/v1/spaces/s/documents",
        "/api/v1/spaces/s/documents/a/content",
      ]);
    } finally {
      vi.useRealTimers();
    }
  });
  it("preserves unbound legacy work and never transmits it under a new account", async () => {
    api.setToken(replayToken("new-user", "new-session"));
    const item = rawQueueMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: "Original unknown owner" });
    const before = JSON.stringify(listOfflineServerMutations()); const request = vi.fn(); vi.stubGlobal("fetch", request);
    await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 0, pending: 1, blocked: 1 });
    expect(request).not.toHaveBeenCalled(); expect(JSON.stringify(listOfflineServerMutations())).toBe(before); expect(item.origin).toBeUndefined();
  });
  for (const change of ["account", "disk-token", "provider", "mode", "base", "refresh"] as const) {
    it(`does not dispatch after ${change} changes during an actual Web Lock wait`, async () => {
      const item = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: "Exact original bytes" });
      let release!: () => void;
      vi.stubGlobal("navigator", { locks: { request: (_name: string, run: () => Promise<unknown>) => new Promise(resolve => { release = () => { void run().then(resolve); }; }) } });
      const request = vi.fn(); vi.stubGlobal("fetch", request); const run = replayOfflineServerMutations();
      if (change === "account") api.setToken(replayToken("b", "b-session"));
      if (change === "disk-token") localStorage.setItem("access_token", replayToken("b", "b-session"));
      if (change === "provider") localStorage.setItem("starlane_sync_provider", "google-drive");
      if (change === "mode") setCurrentLaunchMode("local");
      if (change === "base") { (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}; localStorage.setItem("server_url", "https://other.invalid"); }
      if (change === "refresh") api.setRefreshToken("changed-refresh");
      release(); await expect(run).resolves.toEqual({ applied: 0, pending: 1, blocked: 1 });
      expect(request).not.toHaveBeenCalled(); expect(listOfflineServerMutations()).toEqual([item]);
    });
  }
  it("retains an old failed request under its original origin and reports account drift", async () => {
    api.setToken(replayToken()); let reject!: (error: Error) => void;
    vi.stubGlobal("fetch", vi.fn(() => new Promise((_resolve, fail) => { reject = fail; })));
    const write = api.fetch("/spaces/s/documents/a/content", { method: "PUT", body: { content: "A bytes" } });
    api.setToken(replayToken("b", "session-b")); reject(new TypeError("Offline"));
    await expect(write).rejects.toThrow("context changed"); expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ body: { content: "A bytes" }, origin: expect.objectContaining({ accountId: "actor-a", sessionId: "session-a" }), replayState: "unknown" })]);
  });
  it("marks a dispatched mutation unknown after account drift and never automatically retries it", async () => {
    const item = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: "Exact original" });
    const later = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/b/content", "PUT", { content: "Later" });
    const original = api.getToken();
    const request = vi.fn(async (_url, init) => { expect(init.headers.Authorization).toBe(`Bearer ${original}`); api.setToken(replayToken("b", "b-session")); return new Response(null, { status: 204 }); }); vi.stubGlobal("fetch", request);
    await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 0, pending: 2, blocked: 2 });
    expect(request).toHaveBeenCalledTimes(1); expect(listOfflineServerMutations()[0]).toMatchObject({ ...item, replayState: "unknown", attempts: 1 }); expect(listOfflineServerMutations()[1]).toEqual(later);
    api.setToken(original); await replayOfflineServerMutations(); expect(request).toHaveBeenCalledTimes(1);
  });
  it("keeps attachment bytes and page intents unchanged when context changes inside response JSON", async () => {
    const item = queueOfflineServerAttachment({ serverBase: "/api/v1", path: "/spaces/s/documents/a/attachments", spaceId: "s", documentId: "a", name: "a.png", mimeType: "image/png", dataUrl: "data:image/png;base64,QQ==" });
    const page = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: item.dataUrl });
    const originalCache = { content: "Unrelated current cache bytes" };
    cacheServerResponse("/api/v1", page.path, originalCache, page.origin!.cacheScope);
    const request = vi.fn(async () => ({ status: 201, json: async () => { localStorage.setItem("access_token", replayToken("b", "other")); return { url: "/files/new" }; } } as Response)); vi.stubGlobal("fetch", request);
    await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 0, pending: 2, blocked: 2 });
    expect(request).toHaveBeenCalledTimes(1); expect(listOfflineServerMutations()).toEqual([page]); expect(listOfflineServerAttachments()[0]).toEqual({ ...item, replayState: "unknown", attempts: 1, lastError: expect.stringContaining("context changed; retain pending work for review; server outcome unknown; explicit review required") });
    expect(readCachedServerResponse("/api/v1", page.path, page.origin!.cacheScope)).toEqual(originalCache);
  });
  it("rewrites identical attachment bytes only for their exact original page and origin", async () => {
    const input = { serverBase: "/api/v1", path: "/spaces/s/documents/a/attachments", spaceId: "s", documentId: "a", name: "same.png", mimeType: "image/png", dataUrl: "data:image/png;base64,QQ==" };
    queueOfflineServerAttachment(input);
    queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: input.dataUrl });
    const other = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/b/content", "PUT", { content: input.dataUrl });
    const foreign = queueOfflineServerMutation("https://other.invalid/api/v1", "/spaces/s/documents/a/content", "PUT", { content: input.dataUrl });
    const request = vi.fn(async (_url, init) => { if (init.method === "POST") return new Response(JSON.stringify({ url: "/files/owned-by-a" }), { status: 201 }); if (String(_url).includes("/a/")) expect(JSON.parse(init.body)).toEqual({ content: "/files/owned-by-a" }); else expect(JSON.parse(init.body)).toEqual(other.body); return new Response(null, { status: 204 }); }); vi.stubGlobal("fetch", request);
    await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 3, pending: 0, blocked: 0 }); expect(listOfflineServerMutations()).toEqual([foreign]);
  });
  it("retains create409 and stops a dependent content write without synthetic acknowledgement", async () => {
    const create = queueOfflineServerMutation("/api/v1", "/spaces/s/documents", "POST", { source_id: "a", content: "Exact create" });
    const update = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: "Dependent" });
    const request = vi.fn(async () => new Response(null, { status: 409 })); vi.stubGlobal("fetch", request);
    await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 0, pending: 2, blocked: 2 }); expect(request).toHaveBeenCalledTimes(1);
    expect(listOfflineServerMutations()).toEqual([{ ...create, attempts: 1, lastError: "Server returned 409; original intent requires review" }, update]);
  });
  it("allows fresh same-session token rotation but does not rebind a new server session", async () => {
    const item = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: "Retained" });
    api.setToken(replayToken("actor-a", "different-session")); const request = vi.fn(async () => new Response(null, { status: 204 })); vi.stubGlobal("fetch", request);
    await replayOfflineServerMutations(); expect(request).not.toHaveBeenCalled(); expect(listOfflineServerMutations()).toEqual([item]);
    api.setToken(replayToken("actor-a", "session-a", 2)); await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 1, pending: 0, blocked: 0 }); expect(request).toHaveBeenCalledTimes(1);
  });

  it("enqueues nonsecret immutable account/session origin through the actual offline producer", async () => {
    api.setToken(replayToken()); vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Offline")));
    await api.fetch("/spaces/s/documents/a/content", { method: "PUT", body: { content: "Original" } });
    const [item] = listOfflineServerMutations(); expect(item.origin).toMatchObject({ version: 1, accountId: "actor-a", sessionId: "session-a", serverBase: "/api/v1" });
    expect(item.replayState).toBe("unknown");
    expect(JSON.stringify(item)).not.toContain(api.getToken()); expect(JSON.stringify(item)).not.toContain("Bearer");
  });
  it("retains malformed present provenance without dispatch or normalization", async () => {
    const item = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: "Exact" });
    const key = `starlane_server_cache_v1:mutation_queue:item:${encodeURIComponent(item.id)}`;
    const malformed = JSON.stringify({ ...item, origin: { ...item.origin, accountId: null } }); localStorage.setItem(key, malformed);
    const request = vi.fn(); vi.stubGlobal("fetch", request); await replayOfflineServerMutations(); expect(request).not.toHaveBeenCalled(); expect(localStorage.getItem(key)).toBe(malformed);
  });

  it("retains both original dispatched and concurrently edited bytes without overwriting the newer cache", async () => {
    const original = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/content", "PUT", { content: "Original dispatched Ω\n" });
    const scope = original.origin!.cacheScope;
    const edited = { ...original, body: { content: "Newer edited bytes\n" } };
    const request = vi.fn(async () => {
      updateOfflineServerMutation(edited, original);
      cacheServerResponse("/api/v1", original.path, { content: "Newest workspace cache" }, scope);
      api.setToken(replayToken("other", "other-session"));
      return new Response(null, { status: 204 });
    });
    vi.stubGlobal("fetch", request);
    await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 0, pending: 2, blocked: 2 });
    const items = listOfflineServerMutations();
    expect(items.find(item => item.id === original.id)).toEqual(edited);
    const evidence = items.find(item => item.id !== original.id)!;
    expect(evidence).toMatchObject({ body: original.body, origin: original.origin, path: original.path, method: original.method, replayState: "unknown" });
    expect(readCachedServerResponse("/api/v1", original.path, scope)).toEqual({ content: "Newest workspace cache" });
    api.setToken(replayToken()); await replayOfflineServerMutations(); expect(request).toHaveBeenCalledTimes(1);
  });
  it("preserves dispatched producer JSON when its caller edits the object during rejection", async () => {
    api.setToken(replayToken());
    const body = { content: "Exact sent bytes\nΩ" };
    let reject!: (error: Error) => void;
    const request = vi.fn(() => new Promise<Response>((_resolve, fail) => { reject = fail; }));
    vi.stubGlobal("fetch", request);
    const pending = api.fetch("/spaces/s/documents/a/content", { method: "PUT", body });
    body.content = "Caller changed after dispatch"; reject(new TypeError("Connection lost"));
    await expect(pending).resolves.toBeUndefined();
    expect(listOfflineServerMutations()[0]).toMatchObject({ body: { content: "Exact sent bytes\nΩ" }, replayState: "unknown" });
    await replayOfflineServerMutations(); expect(request).toHaveBeenCalledTimes(1);
  });
  it("queues a known-unsent producer without invoking transport and later replays its exact bytes", async () => {
    api.setToken(replayToken()); Object.assign(navigator, { onLine: false });
    const request = vi.fn(async () => new Response(null, { status: 204 })); vi.stubGlobal("fetch", request);
    await api.fetch("/spaces/s/documents/a/content", { method: "PUT", body: { content: "Known unsent" } });
    expect(request).not.toHaveBeenCalled(); expect(listOfflineServerMutations()[0].replayState).toBeUndefined();
    Object.assign(navigator, { onLine: true }); await replayOfflineServerMutations(); expect(request).toHaveBeenCalledTimes(1);
  });

  it("does not upload an attachment before its unresolved source-ID page creation", async () => {
    const create = queueOfflineServerMutation("/api/v1", "/spaces/s/documents", "POST", { source_id: "a", title: "Pending page", content: "Exact source bytes" });
    const attachment = queueOfflineServerAttachment({ serverBase: "/api/v1", path: "/spaces/s/documents/a/attachments", spaceId: "s", documentId: "a", name: "a.png", mimeType: "image/png", dataUrl: "data:image/png;base64,QQ==" });
    const request = vi.fn(); vi.stubGlobal("fetch", request);
    await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 0, pending: 2, blocked: 2 });
    expect(request).not.toHaveBeenCalled(); expect(listOfflineServerMutations()).toEqual([create]); expect(listOfflineServerAttachments()).toEqual([{ ...attachment, lastError: "Attachment depends on an unresolved page creation; review the retained page and attachment together before replay" }]);
  });

  it("retains dispatched A bytes after logout instead of gating retention on current authentication", async () => {
    api.setToken(replayToken()); const scope = captureOfflineServerContext().origin.cacheScope;
    let reject!: (error: Error) => void;
    const request = vi.fn(() => new Promise<Response>((_resolve, fail) => { reject = fail; })); vi.stubGlobal("fetch", request);
    const pending = api.fetch("/spaces/s/documents/a/content", { method: "PUT", body: { content: "Original logged-out bytes" } });
    api.logout(); reject(new TypeError("Lost acknowledgement"));
    await expect(pending).rejects.toThrow("context changed");
    expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ body: { content: "Original logged-out bytes" }, replayState: "unknown", origin: expect.objectContaining({ accountId: "actor-a", sessionId: "session-a" }) })]);
    expect(readCachedServerResponse("/api/v1", "/spaces/s/documents/a/content", scope)).toBeNull();
    expect(api.getToken()).toBeNull(); expect(request).toHaveBeenCalledTimes(1);
  });
  it("retries only the captured JSON after a legitimate same-session held refresh", async () => {
    api.setToken(replayToken()); api.setRefreshToken("refresh-a");
    const body = { content: "Originally dispatched body" };
    let release!: (response: Response) => void; let reached!: () => void;
    const waiting = new Promise<void>(resolve => { reached = resolve; });
    const request = vi.fn(async (_url: string, init?: RequestInit): Promise<Response> => {
      if (String(_url).endsWith("/auth/refresh")) { reached(); return new Promise<Response>(resolve => { release = resolve; }); }
      if (request.mock.calls.length === 1) return new Response(JSON.stringify({ error: "unauthorized", message: "Invalid or expired token" }), { status: 401 });
      expect(init?.body).toBe(JSON.stringify({ content: "Originally dispatched body" }));
      expect(init?.headers).toMatchObject({ Authorization: `Bearer ${replayToken("actor-a", "session-a", 2)}` });
      return new Response(null, { status: 204 });
    }); vi.stubGlobal("fetch", request);
    const pending = api.fetch("/spaces/s/documents/a/content", { method: "PUT", body });
    await waiting; body.content = "Edited while refresh was held";
    release(new Response(JSON.stringify({ access_token: replayToken("actor-a", "session-a", 2), refresh_token: "refresh-a-2" }), { status: 200 }));
    await expect(pending).resolves.toBeUndefined(); expect(request).toHaveBeenCalledTimes(3); expect(listOfflineServerMutations()).toEqual([]);
  });
  it("does not overwrite a changed account or redispatch after a held refresh", async () => {
    api.setToken(replayToken()); api.setRefreshToken("refresh-a");
    let release!: (response: Response) => void; let reached!: () => void;
    const waiting = new Promise<void>(resolve => { reached = resolve; });
    const request = vi.fn(async (_url: string) => {
      if (String(_url).endsWith("/auth/refresh")) { reached(); return new Promise<Response>(resolve => { release = resolve; }); }
      return new Response(JSON.stringify({ error: "unauthorized", message: "Invalid or expired token" }), { status: 401 });
    }); vi.stubGlobal("fetch", request);
    const pending = api.fetch("/spaces/s/documents/a/content", { method: "PUT", body: { content: "A request" } });
    await waiting; api.setToken(replayToken("b", "session-b")); api.setRefreshToken("refresh-b");
    release(new Response(JSON.stringify({ access_token: replayToken("actor-a", "session-a", 2), refresh_token: "refresh-a-2" }), { status: 200 }));
    await expect(pending).rejects.toMatchObject({ status: 503 }); expect(request).toHaveBeenCalledTimes(2);
    expect(api.getToken()).toBe(replayToken("b", "session-b")); expect(api.getRefreshToken()).toBe("refresh-b"); expect(listOfflineServerMutations()).toEqual([]);
  });
  it("rejects a refresh response with another subject/session before installing or retrying it", async () => {
    api.setToken(replayToken()); api.setRefreshToken("refresh-a");
    const request = vi.fn(async (url: string) => String(url).endsWith("/auth/refresh")
      ? new Response(JSON.stringify({ access_token: replayToken("b", "session-b"), refresh_token: "foreign" }), { status: 200 })
      : new Response(JSON.stringify({ error: "unauthorized", message: "Invalid or expired token" }), { status: 401 })); vi.stubGlobal("fetch", request);
    await expect(api.fetch("/spaces/s/documents/a/content", { method: "PUT", body: { content: "Original" } })).rejects.toMatchObject({ status: 503 });
    expect(request).toHaveBeenCalledTimes(2); expect(api.getToken()).toBe(replayToken()); expect(api.getRefreshToken()).toBe("refresh-a");
  });

  it("retains a successful dispatched204 under its original origin instead of returning it to a changed account", async () => {
    api.setToken(replayToken()); const scope = captureOfflineServerContext().origin.cacheScope;
    let release!: (response: Response) => void;
    const request = vi.fn(() => new Promise<Response>(resolve => { release = resolve; })); vi.stubGlobal("fetch", request);
    const pending = api.fetch("/spaces/s/documents/a/content", { method: "PUT", body: { content: "Original204 bytes" } });
    api.setToken(replayToken("b", "session-b")); release(new Response(null, { status: 204 }));
    await expect(pending).rejects.toThrow("context changed");
    expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ body: { content: "Original204 bytes" }, replayState: "unknown", origin: expect.objectContaining({ accountId: "actor-a" }) })]);
    expect(readCachedServerResponse("/api/v1", "/spaces/s/documents/a/content", scope)).toBeNull(); expect(request).toHaveBeenCalledTimes(1);
  });
  for (const change of ["account", "provider", "mode"] as const) {
    it(`retains original mutation bytes without foreign result/cache after ${change} drift during real response text wait`, async () => {
      api.setToken(replayToken()); const scope = captureOfflineServerContext().origin.cacheScope;
      let reached!: () => void; let release!: (text: string) => void;
      const waiting = new Promise<void>(resolve => { reached = resolve; });
      vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, status: 200, text: () => { reached(); return new Promise<string>(resolve => { release = resolve; }); } } as Response)));
      const pending = api.fetch("/spaces/s/documents/a/drafts/draft-a", { method: "PUT", body: { content: "Original text-wait bytes" } });
      await waiting;
      if (change === "account") api.setToken(replayToken("b", "session-b"));
      if (change === "provider") localStorage.setItem("starlane_sync_provider", "google-drive");
      if (change === "mode") setCurrentLaunchMode("local");
      release(JSON.stringify({ id: "draft-a", content: "A server result" }));
      await expect(pending).rejects.toThrow("context changed");
      expect(readCachedServerResponse("/api/v1", "/spaces/s/documents/a/drafts/draft-a", scope)).toBeNull();
      expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ body: { content: "Original text-wait bytes" }, replayState: "unknown", origin: expect.objectContaining({ accountId: "actor-a" }) })]);
    });
  }

  it("preserves ordinary same-context204 completion without dispatching a native mutation mirror", async () => {
    api.setToken(replayToken());
    const invoke = vi.fn(); (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = { invoke };
    localStorage.setItem("server_url", "https://fictional.invalid");
    const request = vi.fn(async () => new Response(null, { status: 204 })); vi.stubGlobal("fetch", request);
    await expect(api.fetch("/spaces/s/documents/a/content", { method: "PUT", body: { content: "Same actor" } })).resolves.toBeUndefined();
    expect(listOfflineServerMutations()).toEqual([]); expect(request).toHaveBeenCalledTimes(1); expect(invoke).not.toHaveBeenCalled();
  });
  it("rejects foreign successful mutation results with ordinary opaque-token compatibility", async () => {
    api.setToken("opaque-original"); let release!: (response: Response) => void;
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(resolve => { release = resolve; })));
    const pending = api.fetch("/spaces/s/documents/a/content", { method: "PUT", body: { content: "Opaque original" } });
    api.setToken("opaque-other"); release(new Response(null, { status: 204 }));
    await expect(pending).rejects.toThrow("Original mutation context changed"); expect(listOfflineServerMutations()).toEqual([]);
  });

  for (const outcome of ["201", "202", "204", "wrong-draft", "missing-source", "invalid-source", "wrong-page", "wrong-space"]) {
    it(`holds source-bound draft replay without ACK for ${outcome}`, async () => {
      const original = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/drafts/d", "PUT", { content: "Exact source-bound intent", source_digest: "a".repeat(64) });
      const status = /^\d+$/.test(outcome) ? Number(outcome) : 200;
      const row: Record<string, unknown> = { ...sourceBoundAckFields, id: "d", document_id: "a", space_id: "s", content: "Exact source-bound intent", source_digest: "b".repeat(64) };
      if (outcome === "wrong-draft") row.id = "other";
      if (outcome === "wrong-page") row.document_id = "other";
      if (outcome === "wrong-space") row.space_id = "other";
      if (outcome === "missing-source") delete row.source_digest;
      if (outcome === "invalid-source") row.source_digest = "NOT-A-DIGEST";
      const request = vi.fn(async () => new Response(status === 204 ? null : JSON.stringify(row), { status })); vi.stubGlobal("fetch", request);
      await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 0, pending: 1, blocked: 1 });
      expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ ...original, attempts: 1, replayState: "unknown" })]);
      await replayOfflineServerMutations(); expect(request).toHaveBeenCalledTimes(1);
    });
  }
  it("ACKs exact source-bound draft200 while preserving original-context guards through the JSON await", async () => {
    queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/drafts/d", "PUT", { content: "Exact intent", source_digest: "a".repeat(64) });
    const response = () => new Response(JSON.stringify({ ...sourceBoundAckFields, id: "d", document_id: "a", space_id: "s", content: "Exact intent", source_digest: "b".repeat(64) }), { status: 200 });
    vi.stubGlobal("fetch", vi.fn(async () => response()));
    await expect(replayOfflineServerMutations()).resolves.toEqual({ applied: 1, pending: 0, blocked: 0 });
    const original = queueOfflineServerMutation("/api/v1", "/spaces/s/documents/a/drafts/d", "PUT", { content: "Next exact intent", source_digest: "b".repeat(64) });
    let release!: (row: unknown) => void; let reached!: () => void;
    const waiting = new Promise<void>(resolve => { reached = resolve; });
    const held = response();
    vi.spyOn(held, "json").mockImplementation(() => { reached(); return new Promise(resolve => { release = resolve; }); });
    const request = vi.fn(async () => held); vi.stubGlobal("fetch", request);
    const pending = replayOfflineServerMutations(); await waiting;
    api.setToken(replayToken("other", "other-session"));
    release({ ...sourceBoundAckFields, id: "d", document_id: "a", space_id: "s", content: "Next exact intent", source_digest: "c".repeat(64) });
    await expect(pending).resolves.toEqual({ applied: 0, pending: 1, blocked: 1 });
    expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ ...original, attempts: 1, replayState: "unknown" })]);
    expect(request).toHaveBeenCalledTimes(1);
  });

  for (const variant of ["missing-source", "foreign-draft", "wrong-content", "wrong-title", "wrong-icon", "wrong-cover", "missing-owner", "viewer", "missing-stamp", "malformed-presentation", "foreign-owner"]) {
    it(`retains malformed200 ${variant} before publishing the dispatched patch into workspace cache`, async () => {
      api.setToken(replayToken());
      const origin = captureOfflineServerContext().origin;
      const original = { id: "a", title: "OLD CACHED TITLE", icon: "old", cover_image: "old-cover" };
      cacheServerResponse("/api/v1", "/spaces/s/documents/a", original, origin.cacheScope);
      const body = { title: " New title ", icon: " ", cover_image: "nonempty raw ", content: "EXACT DRAFT BYTES", source_digest: "a".repeat(64) };
      const row: Record<string, unknown> = { ...sourceBoundAckFields, id: "d", document_id: "a", space_id: "s", title: "New title", icon: null, cover_image: "nonempty raw ", content: "EXACT DRAFT BYTES", source_digest: "b".repeat(64) };
      if (variant === "missing-source") delete row.source_digest;
      if (variant === "foreign-draft") row.id = "other";
      if (variant === "missing-owner") delete row.owner_id;
      if (variant === "viewer") row.access_role = "viewer";
      if (variant === "missing-stamp") delete row.updated_at;
      if (variant === "malformed-presentation") row.icon = {};
      if (variant === "foreign-owner") row.owner_id = "foreign";
      if (variant === "wrong-content") row.content = "OTHER BYTES";
      if (variant === "wrong-title") row.title = "Other title";
      if (variant === "wrong-icon") row.icon = "other-icon";
      if (variant === "wrong-cover") row.cover_image = "nonempty raw";
      const request = vi.fn(async () => new Response(JSON.stringify(row), { status: 200 })); vi.stubGlobal("fetch", request);
      await expect(api.fetch("/spaces/s/documents/a/drafts/d", { method: "PUT", body, expectedStatuses: [200] })).rejects.toMatchObject({ code: "outcome_unproven" });
      expect(readCachedServerResponse("/api/v1", "/spaces/s/documents/a", origin.cacheScope)).toEqual(original);
      expect(listOfflineServerMutations()).toEqual([expect.objectContaining({ body, replayState: "unknown", origin })]);
      expect(request).toHaveBeenCalledTimes(1);
    });
  }
  it("confirms the exact normalized patch200 before permitting its ordinary cache effect", async () => {
    api.setToken(replayToken());
    const body = { title: " New title ", icon: " ", cover_image: "exact raw ", content: "EXACT BYTES", source_digest: "a".repeat(64) };
    const row = { ...sourceBoundAckFields, id: "d", document_id: "a", space_id: "s", title: "New title", icon: null, cover_image: "exact raw ", content: "EXACT BYTES", source_digest: "b".repeat(64) };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(row), { status: 200 })));
    await expect(api.fetch("/spaces/s/documents/a/drafts/d", { method: "PUT", body, expectedStatuses: [200] })).resolves.toEqual(row);
    expect(listOfflineServerMutations()).toEqual([]);
  });

});

describe("original draft attachment replay audience",()=>{
 beforeEach(()=>{vi.unstubAllGlobals();stubLocalStorage();api.logout();setCurrentLaunchMode("server");vi.stubGlobal("navigator",{locks:{request:vi.fn((_name:string,run:()=>Promise<unknown>)=>run())}});});
 const originalDraft="11111111-1111-4111-8111-111111111111",otherDraft="22222222-2222-4222-8222-222222222222";
 const input={serverBase:"/api/v1",path:"/spaces/s/documents/a/attachments",spaceId:"s",documentId:"a",name:"private.txt",mimeType:"text/plain",dataUrl:"data:text/plain;base64,QQ==",scopeVersion:2 as const,draftId:originalDraft};
 it("replays the persisted original draft header and rewrites only that draft's queued references",async()=>{
  queueOfflineServerAttachment(input);const first=queueOfflineServerMutation("/api/v1",`/spaces/s/documents/a/drafts/${originalDraft}`,"PUT",{content:input.dataUrl,source_digest:"a".repeat(64)});const second=queueOfflineServerMutation("/api/v1",`/spaces/s/documents/a/drafts/${otherDraft}`,"PUT",{content:input.dataUrl,source_digest:"b".repeat(64)});
  const fetchMock=vi.fn(async(_url:string,options?:RequestInit)=>options?.method==="POST"?new Response(JSON.stringify({url:"/api/v1/files/private-original",audience:{schema_version:2,draft_id:originalDraft,draft_created_at:"2026-10-16T00:00:00Z"}}),{status:201}):new Response(JSON.stringify({error:"current draft refused"}),{status:403}));vi.stubGlobal("fetch",fetchMock);
  await replayOfflineServerMutations();expect(fetchMock.mock.calls[0][1]?.headers).toEqual(expect.objectContaining({"X-Starlane-Upload-Draft":originalDraft}));expect(listOfflineServerAttachments()).toHaveLength(0);expect((listOfflineServerMutations().find(x=>x.id===first.id)?.body as {content:string}).content).toBe("/api/v1/files/private-original");expect((listOfflineServerMutations().find(x=>x.id===second.id)?.body as {content:string}).content).toBe(input.dataUrl);
 });
 it("holds a wrong-draft ACK with its original bytes instead of rewriting or acknowledging the upload",async()=>{
  const queued=queueOfflineServerAttachment(input);vi.stubGlobal("fetch",vi.fn(async()=>new Response(JSON.stringify({url:"/api/v1/files/wrong",audience:{schema_version:2,draft_id:otherDraft,draft_created_at:"2026-10-16T00:00:00Z"}}),{status:201})));
  const result=await replayOfflineServerMutations();expect(result.applied).toBe(0);expect(listOfflineServerAttachments()).toEqual([expect.objectContaining({id:queued.id,draftId:originalDraft,dataUrl:input.dataUrl,replayState:"unknown"})]);
 });
 it("retains malformed persisted scope visibly and dispatches nothing",async()=>{
  const queued=queueOfflineServerAttachment(input);const key=Array.from({length:localStorage.length},(_,i)=>localStorage.key(i)).find(k=>k?.includes(queued.id))!;const value=JSON.parse(localStorage.getItem(key)!);value.draftId="not-an-original-draft";localStorage.setItem(key,JSON.stringify(value));const fetchMock=vi.fn();vi.stubGlobal("fetch",fetchMock);const result=await replayOfflineServerMutations();expect(fetchMock).not.toHaveBeenCalled();expect(result.pending).toBe(1);expect(listOfflineServerAttachments()[0]).toEqual(expect.objectContaining({draftId:"not-an-original-draft",dataUrl:input.dataUrl,replayState:"unknown"}));
 });
});
