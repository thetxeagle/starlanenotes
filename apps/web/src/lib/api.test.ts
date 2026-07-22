import { beforeEach, describe, expect, it, vi } from "vitest";
import { api, ApiError, isApiError, replayOfflineServerMutations } from "./api";
import { listOfflineServerMutations } from "./offlineServerCache";

function stubLocalStorage() {
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: vi.fn((key: string) => storage.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      storage.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      storage.delete(key);
    }),
  });
}

describe("api error envelopes", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    stubLocalStorage();
    api.logout();
  });

  it("preserves structured server error codes while keeping message compatibility", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
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
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
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

  it("queues offline document updates and replays them when the server returns", async () => {
    api.setToken("active-token");
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

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status: 204 })));
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 1,
      pending: 0,
      blocked: 0,
    });
    expect(listOfflineServerMutations("/api/v1")).toEqual([]);
  });

  it("queues source-ID-backed document creation and treats replay conflict as idempotent", async () => {
    api.setToken("active-token");
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

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status: 409 })));
    await expect(replayOfflineServerMutations()).resolves.toEqual({
      applied: 1,
      pending: 0,
      blocked: 0,
    });
  });
});
