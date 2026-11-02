import { beforeEach, describe, expect, it, vi } from "vitest";
import { api, getApiBase } from "./api";
import { setCurrentLaunchMode } from "./launchMode";
import {
  inspectDocumentAccess,
  parsePageAccessInspection,
  buildPageShareLink,
  capturePageShareTargetGuard,
} from "./pageAccessInspection";
import { dataClient } from "./dataClient";
import {
  cacheServerResponse,
  serverCacheScopeForSession,
  listOfflineServerMutations,
} from "./offlineServerCache";

export function reportFixture() {
  return {
    schema_version: 1,
    space_id: "space",
    page_id: "page",
    checked_at: "2026-10-08T12:00:00Z",
    capabilities: {
      view: true,
      comment: false,
      edit: false,
      move: false,
      manage_lock: false,
      enable_public_link: false,
      disable_public_link: false,
    },
    space_permission_sources: [
      { kind: "global_group_role", permissions: ["view", "edit_content"] },
    ],
    restrictions: { local: "inherited", ancestor_restricted: false },
    public_link: { configured: false, policy_allows_anonymous: false, reason: "not_enabled" },
    selected_private_draft: { scope: "shared_viewer", view: true, edit: false },
    sharing: { people_grants_available: false, guests_available: false },
  };
}
beforeEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  setCurrentLaunchMode("server");
  const storage = new Map<string, string>([["starlane_launch_mode", "server"]]);
  const localStorage = {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  };
  vi.stubGlobal("localStorage", localStorage);
  vi.stubGlobal("window", { localStorage, dispatchEvent: vi.fn() });
  api.setToken("fixture-token-a");
  api.setRefreshToken(null);
});
describe("page access inspection", () => {
  it("keeps a reused dialog bound to its original page and space across response/copy waits", async () => {
    let current = { spaceId: "space", pageId: "page" };
    const assertTarget = capturePageShareTargetGuard(() => current);
    assertTarget();
    const copy = vi.fn();
    current = { spaceId: "space", pageId: "other" };
    expect(() => {
      assertTarget();
      copy();
    }).toThrow("Page changed");
    expect(copy).not.toHaveBeenCalled();
    current = { spaceId: "foreign", pageId: "page" };
    await expect(
      Promise.resolve(reportFixture()).then((report) => {
        assertTarget();
        return report;
      }),
    ).rejects.toThrow("Page changed");
  });
  it("copies the configured authenticated server route in native mode, never a tauri or public URL", () => {
    expect(
      buildPageShareLink("space /raw", "page", {
        native: true,
        apiBase: "https://notes.example.com/prefix/api/v1",
        webOrigin: "tauri://localhost",
      }),
    ).toBe("https://notes.example.com/prefix/spaces/space%20%2Fraw/docs/page");
    expect(
      buildPageShareLink("space", "page", {
        native: false,
        apiBase: "/api/v1",
        webOrigin: "https://app.example.com",
      }),
    ).toBe("https://app.example.com/spaces/space/docs/page");
    for (const apiBase of [
      "/api/v1",
      "tauri://localhost/api/v1",
      "https://notes.example.com/invalid",
      "https://user:password@notes.example.com/api/v1",
    ])
      expect(() =>
        buildPageShareLink("space", "page", {
          native: true,
          apiBase,
          webOrigin: "tauri://localhost",
        }),
      ).toThrow("page link is unavailable");
  });
  it("checks every versioned field and exact target before exposing a report", () => {
    expect(parsePageAccessInspection(reportFixture(), "space", "page").capabilities.edit).toBe(
      false,
    );
    const invalid = [
      { ...reportFixture(), page_id: "foreign" },
      { ...reportFixture(), schema_version: 2 },
      { ...reportFixture(), checked_at: 1 },
      { ...reportFixture(), checked_at: "yesterday" },
      { ...reportFixture(), hidden_page_ids: [] },
      { ...reportFixture(), capabilities: { ...reportFixture().capabilities, edit: "true" } },
      { ...reportFixture(), restrictions: { local: "inherited", ancestor_restricted: 1 } },
      { ...reportFixture(), space_permission_sources: [{ kind: "owner", permissions: ["view"] }] },
      {
        ...reportFixture(),
        space_permission_sources: [{ kind: "direct_space_role", permissions: ["admin"] }],
      },
      {
        ...reportFixture(),
        selected_private_draft: { scope: "shared_owner", view: true, edit: true },
      },
      {
        ...reportFixture(),
        public_link: { configured: true, policy_allows_anonymous: true, reason: "accessible" },
      },
      { ...reportFixture(), sharing: { people_grants_available: "true", guests_available: false } },
    ];
    for (const row of invalid)
      expect(() => parsePageAccessInspection(row, "space", "page")).toThrow("Unable to verify");
  });
  it("rejects contradictory authority and duplicated aggregated sources from actual transport", async () => {
    const original = reportFixture();
    const cases = [
      {
        ...original,
        restrictions: { local: "restricted", ancestor_restricted: false },
        public_link: { configured: true, policy_allows_anonymous: true, reason: null },
      },
      {
        ...original,
        restrictions: { local: "inherited", ancestor_restricted: true },
        public_link: { configured: true, policy_allows_anonymous: true, reason: null },
      },
      {
        ...original,
        public_link: {
          configured: false,
          policy_allows_anonymous: false,
          reason: "deployment_disabled",
        },
      },
      { ...original, capabilities: { ...original.capabilities, view: false } },
      {
        ...original,
        public_link: { configured: false, policy_allows_anonymous: true, reason: null },
      },
      {
        ...original,
        public_link: { configured: true, policy_allows_anonymous: true, reason: "restricted" },
      },
      {
        ...original,
        public_link: { configured: true, policy_allows_anonymous: false, reason: null },
      },
      {
        ...original,
        public_link: { configured: true, policy_allows_anonymous: false, reason: "not_enabled" },
      },
      { ...original, selected_private_draft: { scope: "none", view: true, edit: false } },
      { ...original, selected_private_draft: { scope: "owned", view: false, edit: false } },
      { ...original, selected_private_draft: { scope: "shared_viewer", view: true, edit: true } },
      { ...original, space_permission_sources: [{ kind: "direct_space_role", permissions: [] }] },
      {
        ...original,
        space_permission_sources: [{ kind: "direct_space_role", permissions: ["view", "view"] }],
      },
      {
        ...original,
        space_permission_sources: [
          original.space_permission_sources[0],
          original.space_permission_sources[0],
        ],
      },
    ];
    for (const response of cases) {
      vi.stubGlobal(
        "fetch",
        vi.fn(async () => new Response(JSON.stringify(response), { status: 200 })),
      );
      await expect(dataClient.inspectDocumentAccess("space", "page")).rejects.toThrow(
        "Unable to verify",
      );
    }
  });
  it("uses existing transport with POST and no body, never a GET permission cache", async () => {
    const fetch = vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify(reportFixture()), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
    void fetch;
    expect(await dataClient.inspectDocumentAccess("space", "page")).toEqual(reportFixture());
    const request = vi.mocked(globalThis.fetch).mock.calls[0];
    expect(request[0]).toMatch(/\/spaces\/space\/documents\/page\/access\/inspect$/);
    expect(request[1]?.method).toBe("POST");
    expect(request[1]?.body).toBeUndefined();
  });
  it("does not turn a failed inspection into stale cached permission success", async () => {
    const path = "/spaces/space/documents/page/access/inspect";
    cacheServerResponse(
      getApiBase(),
      path,
      reportFixture(),
      serverCacheScopeForSession(api.getToken(), api.getRefreshToken()),
    );
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("Network failure");
      }),
    );
    expect(await api.fetch(path)).toEqual(reportFixture());
    const queuedBefore = listOfflineServerMutations(getApiBase());
    await expect(inspectDocumentAccess("space", "page")).rejects.toThrow();
    expect(listOfflineServerMutations(getApiBase())).toEqual(queuedBefore);
    expect(
      vi.mocked(globalThis.fetch).mock.calls[vi.mocked(globalThis.fetch).mock.calls.length - 1]?.[1]
        ?.method,
    ).toBe("POST");
  });
  it("rejects account drift across the actual response wait", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        api.setToken("fixture-token-b");
        return new Response(JSON.stringify(reportFixture()), { status: 200 });
      }),
    );
    await expect(inspectDocumentAccess("space", "page")).rejects.toThrow(
      "Account or workspace changed",
    );
  });
  it("does not request or manufacture server permissions for local mode", async () => {
    setCurrentLaunchMode("local");
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    await expect(inspectDocumentAccess("space", "page")).rejects.toThrow("connected to a server");
    expect(fetch).not.toHaveBeenCalled();
  });
});

describe("actual guest viewer availability projection", () => {
  it("accepts both strict boolean states independently of actor capabilities", () => {
    for(const available of [false,true]) {
      const value={...reportFixture(),sharing:{people_grants_available:true,guests_available:available}};
      const parsed=parsePageAccessInspection(value,"space","page");
      expect(parsed.sharing.guests_available).toBe(available);
      expect(parsed.capabilities).toEqual(value.capabilities);
    }
  });
  it.each(["true",1,null,undefined])("rejects truthy or absent guest availability %j", value => {
    expect(()=>parsePageAccessInspection({...reportFixture(),sharing:{people_grants_available:true,guests_available:value}},"space","page")).toThrow();
  });
});
