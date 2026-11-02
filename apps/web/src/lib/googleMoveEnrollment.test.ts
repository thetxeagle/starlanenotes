import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderSessionRecord } from "./providerSession";
const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const root = "root / original";
const folder = () => ({ id: root, mimeType: "application/vnd.google-apps.folder", trashed: false, capabilities: { canAddChildren: true, canListChildren: true } });
const user = () => ({ user: { me: true, permissionId: "permission-A", displayName: "Same label", emailAddress: "display@example.test" } });

describe("actual Google reviewed move enrollment default HTTP consumer", () => {
  const disk = new Map<string, string>();
  let client: typeof import("./dataClient").dataClient;
  let provider: typeof import("./providerSession");
  let selection: typeof import("./storageProviders");
  let launch: typeof import("./launchMode");
  let api: typeof import("./api").api;
  let original: ProviderSessionRecord;
  let targetCurrent: boolean;
  let fetchMock: ReturnType<typeof vi.fn<typeof globalThis.fetch>>;
  beforeEach(async () => {
    vi.resetModules(); disk.clear(); targetCurrent = true;
    const storage = { getItem: (key: string) => disk.get(key) ?? null, setItem: (key: string, value: string) => { disk.set(key, value); }, removeItem: (key: string) => { disk.delete(key); }, key: (index: number) => [...disk.keys()][index] ?? null, get length() { return disk.size; } };
    vi.stubGlobal("localStorage", storage); vi.stubGlobal("window", { addEventListener: vi.fn(), localStorage: storage, navigator: {} });
    provider = await import("./providerSession"); selection = await import("./storageProviders"); launch = await import("./launchMode"); api = (await import("./api")).api;
    launch.setCurrentLaunchMode("local"); selection.setStorageMode("local"); selection.setSyncProvider("google-drive"); api.setToken(null); api.setRefreshToken(null);
    original = provider.createGoogleDriveManualProviderSession({ access_token: "original-provider-bearer", refresh_token: "original-provider-refresh", expires_in: 3600, scope: "drive.file drive.metadata.readonly offline_access", provider_root_id: root, account_label: "Same label" });
    provider.writeProviderSession("google-drive", original);
    fetchMock = vi.fn<typeof globalThis.fetch>().mockImplementation(async url => new Response(JSON.stringify(String(url).includes("/about?") ? user() : folder()), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock); client = (await import("./dataClient")).dataClient;
  });
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
  const capture = () => client.captureGoogleDriveMoveEnrollment(() => targetCurrent);
  it("consumes actual default fetch, exact current Drive user/root and returns only immutable nonsecret identity", async () => {
    const before = [...disk.entries()]; const enrolled = await capture();
    expect(enrolled.binding).toEqual({ schema_version: 1, provider: "google-drive", account_permission_id: "permission-A", root_id: root });
    expect(Object.isFrozen(enrolled.binding)).toBe(true); enrolled.verifyCurrent(); expect([...disk.entries()]).toEqual(before);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    for (const [, init] of fetchMock.mock.calls) expect(init).toEqual({ method: "GET", headers: { Authorization: "Bearer original-provider-bearer" } });
    expect(String(fetchMock.mock.calls[1][0])).toContain(encodeURIComponent(root));
    expect(JSON.stringify(enrolled.binding)).not.toMatch(/bearer|refresh|email|Same label/);
  });
  it.each(["drive", "https://www.googleapis.com/auth/drive", "https://www.googleapis.com/auth/drive.file"])("accepts actual writable scope %s without inventing new scopes", async scope => {
    provider.writeProviderSession("google-drive", { ...original, auth: { ...original.auth!, scopes: [scope] } });
    expect((await capture()).binding.account_permission_id).toBe("permission-A");
  });
  it("rejects readonly-only bearer before any enrollment request", async () => {
    provider.writeProviderSession("google-drive", { ...original, auth: { ...original.auth!, scopes: ["drive.readonly", "drive.metadata.readonly", "offline_access"] } });
    await expect(capture()).rejects.toThrow("unavailable"); expect(fetchMock).not.toHaveBeenCalled();
  });
  it("rechecks original expiry after the final actual root JSON await", async () => {
    fetchMock.mockImplementation(async url => ({ ok: true, status: 200, json: async () => {
      await Promise.resolve();
      if (String(url).includes("/about?")) return user();
      vi.spyOn(Date, "now").mockReturnValue(Date.parse(original.auth!.expires_at) + 1);
      return folder();
    } } as Response));
    await expect(capture()).rejects.toThrow("unavailable"); expect(fetchMock).toHaveBeenCalledTimes(2);
  });
  it("same display label cannot substitute the actual Drive account identity", async () => {
    const first = await capture(); fetchMock.mockImplementation(async url => new Response(JSON.stringify(String(url).includes("/about?") ? { user: { me: true, permissionId: "permission-B", displayName: "Same label" } } : folder()), { status: 200 }));
    expect((await capture()).binding.account_permission_id).toBe("permission-B"); expect(first.binding.account_permission_id).toBe("permission-A");
  });
  it.each(["account missing", "account foreign", "account blank", "root different", "root trashed", "root not folder", "root cannot write", "root cannot list", "account 201", "root 202", "account 401", "root 403", "malformed", "missing capabilities"])("denies %s without persisting enrollment or changing ordinary session", async bad => {
    const before = [...disk.entries()];
    fetchMock.mockImplementation(async url => {
      const account = String(url).includes("/about?"); let body: unknown = account ? user() : folder(); let status = 200;
      if (account && bad === "account missing") body = {};
      if (account && bad === "account foreign") body = { user: { me: false, permissionId: "permission-A" } };
      if (account && bad === "account blank") body = { user: { me: true, permissionId: " " } };
      if (!account && bad === "root different") body = { ...folder(), id: "foreign-root" };
      if (!account && bad === "root trashed") body = { ...folder(), trashed: true };
      if (!account && bad === "root not folder") body = { ...folder(), mimeType: "text/plain" };
      if (!account && bad === "root cannot write") body = { ...folder(), capabilities: { canAddChildren: false, canListChildren: true } };
      if (!account && bad === "root cannot list") body = { ...folder(), capabilities: { canAddChildren: true, canListChildren: false } };
      if (!account && bad === "missing capabilities") body = { id: root, mimeType: "application/vnd.google-apps.folder", trashed: false };
      if (account && bad === "account 201") status = 201;
      if (!account && bad === "root 202") status = 202;
      if (account && bad === "account 401") status = 401;
      if (!account && bad === "root 403") status = 403;
      return new Response(bad === "malformed" ? "{" : JSON.stringify(body), { status });
    });
    await expect(capture()).rejects.toThrow("unavailable"); expect([...disk.entries()]).toEqual(before);
  });
  const change = (kind: string) => {
    if (kind === "root") provider.writeProviderSession("google-drive", { ...original, auth: { ...original.auth!, provider_root_id: "foreign-root" } });
    if (kind === "provider token") provider.writeProviderSession("google-drive", { ...original, auth: { ...original.auth!, access_token: "foreign-provider-token" } });
    if (kind === "session") provider.clearProviderSession("google-drive");
    if (kind === "provider") selection.setSyncProvider("none");
    if (kind === "mode") launch.setCurrentLaunchMode("server");
    if (kind === "storage mode") selection.setStorageMode("server");
    if (kind === "Notes token") api.setToken("foreign-notes-token");
    if (kind === "selection") targetCurrent = false;
  };
  it.each(["account fetch", "account decode", "root fetch", "root decode"].flatMap(boundary => ["root", "provider token", "session", "provider", "mode", "storage mode", "Notes token", "selection"].map(kind => [boundary, kind] as const)))("holds actual %s then denies %s drift before accepting evidence", async (boundary, kind) => {
    let arrived!: () => void, release!: () => void;
    const reached = new Promise<void>(resolve => { arrived = resolve; }); const resume = new Promise<void>(resolve => { release = resolve; });
    fetchMock.mockImplementation(async url => {
      const account = String(url).includes("/about?"); const prefix = account ? "account" : "root";
      if (boundary === `${prefix} fetch`) { arrived(); await resume; }
      return { ok: true, status: 200, json: async () => { if (boundary === `${prefix} decode`) { arrived(); await resume; } return account ? user() : folder(); } } as Response;
    });
    const pending = capture(); const denied = expect(pending).rejects.toThrow(/changed|unavailable/);
    await reached; change(kind); const retained = [...disk.entries()]; release(); await denied;
    expect([...disk.entries()]).toEqual(retained);
  });
  it("returned current verifier rejects a later changed session rather than rebinding immutable evidence", async () => {
    const enrolled = await capture(); change("root"); expect(() => enrolled.verifyCurrent()).toThrow("unavailable"); expect(enrolled.binding.root_id).toBe(root);
  });
  it.each(["disconnected", "simulated", "expired", "scope", "missing root"])("denies %s original session before HTTP", async bad => {
    const next = copy(original);
    if (bad === "disconnected") next.connected = false;
    if (bad === "simulated") next.sync_mode = "simulated";
    if (bad === "expired") next.auth!.expires_at = "2000-01-01T00:00:00Z";
    if (bad === "scope") next.auth!.scopes = [];
    if (bad === "missing root") delete next.auth!.provider_root_id;
    provider.writeProviderSession("google-drive", next); await expect(capture()).rejects.toThrow("unavailable"); expect(fetchMock).not.toHaveBeenCalled();
  });
});
