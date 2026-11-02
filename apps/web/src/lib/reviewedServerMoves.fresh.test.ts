import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import { api, getApiBase } from "./api";
import { setSyncProvider } from "./storageProviders";
import { setCurrentLaunchMode } from "./launchMode";
import {
  cacheServerResponse,
  serverCacheScopeForSession,
  listOfflineServerMutations,
} from "./offlineServerCache";
import { reviewedServerMoves, capturePreservedMetadataContext } from "./reviewedServerMoves";
const index = {
  schema_version: 1,
  space_id: "s",
  document_id: "p",
  checked_at: "2026-10-08T21:00:00Z",
  operations: [],
  next_cursor: null,
};
beforeEach(() => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (k: string) => values.get(k) ?? null,
    setItem: (k: string, v: string) => values.set(k, v),
    removeItem: (k: string) => values.delete(k),
  };
  vi.stubGlobal("localStorage", storage);
  vi.stubGlobal("window", { localStorage: storage, dispatchEvent: vi.fn() });
  setCurrentLaunchMode("server");
  setSyncProvider("none");
  api.setToken("inert-A");
  api.setRefreshToken(null);
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
describe("actual fresh inspection transport and context", () => {
  it("makes bodyless POST and cannot return seeded GET cache or queue inspection on network failure", async () => {
    const path = "/moves/documents/p/preserved/inspect";
    const scope = serverCacheScopeForSession(api.getToken(), api.getRefreshToken());
    cacheServerResponse(getApiBase(), path, index, scope);
    const fetch = vi.fn<typeof globalThis.fetch>(async () => {
      throw Error("Network down");
    });
    vi.stubGlobal("fetch", fetch);
    await expect(reviewedServerMoves.preservedIndex("s", "p")).rejects.toThrow();
    expect(fetch).toHaveBeenCalledWith(
      expect.stringContaining(path),
      expect.objectContaining({ method: "POST" }),
    );
    expect(fetch.mock.calls[0][1]?.body).toBeUndefined();
    expect(listOfflineServerMutations(getApiBase())).toEqual([]);
  });
  it("fences a response after account/page/space or original credential drift", async () => {
    let current = { spaceId: "s", documentId: "p", accountId: "A" };
    const original = capturePreservedMetadataContext(() => current);
    original.assertCurrent();
    current = { ...current, documentId: "other" };
    expect(() => original.assertCurrent()).toThrow("page changed");
    current = { spaceId: "foreign", documentId: "p", accountId: "A" };
    expect(() => original.assertCurrent()).toThrow("page changed");
    current = { spaceId: "s", documentId: "p", accountId: "B" };
    expect(() => original.assertCurrent()).toThrow("Account");
    current = { spaceId: "s", documentId: "p", accountId: "A" };
    api.setToken("inert-B");
    const display = vi.fn();
    await expect(
      Promise.resolve(index).then((v) => {
        original.assertCurrent();
        display(v);
      }),
    ).rejects.toThrow("Account");
    expect(display).not.toHaveBeenCalled();
  });
  it.each(["credential", "server", "provider"])(
    "supports a fresh same-user capture after explicit reopen following %s drift",
    async (kind) => {
      const current = { spaceId: "s", documentId: "p", accountId: "A" };
      const old = capturePreservedMetadataContext(() => current);
      let release!: () => void;
      const wait = new Promise<void>((resolve) => {
        release = resolve;
      });
      const staleDisplay = vi.fn();
      const oldResponse = wait.then(() => {
        old.assertCurrent();
        staleDisplay(index);
      });
      if (kind === "credential") api.setToken("inert-refreshed-A");
      if (kind === "server") {
        localStorage.setItem("server_url", "https://fictional-new-server.example");
        vi.stubGlobal("window", { localStorage, __TAURI_INTERNALS__: {}, dispatchEvent: vi.fn() });
      }
      if (kind === "provider") setSyncProvider("google-drive");
      const rejected = expect(oldResponse).rejects.toThrow("Account");
      release();
      await rejected;
      expect(staleDisplay).not.toHaveBeenCalled();
      const reopened = capturePreservedMetadataContext(() => current);
      vi.stubGlobal(
        "fetch",
        vi.fn<typeof globalThis.fetch>(
          async () =>
            new Response(JSON.stringify(index), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            }),
        ),
      );
      reopened.assertCurrent();
      const fresh = await reviewedServerMoves.preservedIndex("s", "p");
      reopened.assertCurrent();
      expect(fresh).toEqual(index);
    },
  );
});
