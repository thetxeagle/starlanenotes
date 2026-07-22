import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cacheServerResponse,
  getOfflineServerWorkspaceStatus,
  OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT,
  hasCachedServerWorkspace,
  queueOfflineServerMutation,
  serverCacheScopeForToken,
} from "./offlineServerCache";

function stubLocalStorage() {
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: vi.fn((key: string) => storage.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => storage.set(key, value)),
    removeItem: vi.fn((key: string) => storage.delete(key)),
  });
}

describe("offline server workspace cache", () => {
  beforeEach(() => {
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
    queueOfflineServerMutation(server, "/spaces/space-1/folders", "POST", { name: "Offline" }, scope);
    queueOfflineServerMutation("https://other.example.com/api/v1", "/spaces/space-2/folders", "POST", {
      name: "Other server",
    });

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
});
