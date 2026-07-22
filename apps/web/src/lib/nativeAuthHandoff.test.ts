import { beforeEach, describe, expect, it, vi } from "vitest";
import { completeNativeAuthHandoffInBrowser, finishNativeAuthHandoff, parseNativeAuthHandoffCallback, rememberBrowserNativeAuthHandoff } from "./nativeAuthHandoff";

vi.mock("@/lib/api", () => ({ getApiBase: () => "/api/v1", isTauri: () => true }));

const storage = new Map<string, string>();

beforeEach(() => {
  storage.clear();
  vi.stubGlobal("sessionStorage", { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key) });
  vi.stubGlobal("fetch", vi.fn());
});

describe("native authentication handoff", () => {
  it("accepts only the configured Starlane handoff callback shape", () => {
    expect(parseNativeAuthHandoffCallback("starlane://auth/handoff?handoff_id=h&code=c")).toEqual({ handoffId: "h", code: "c" });
    expect(parseNativeAuthHandoffCallback("https://auth/handoff?handoff_id=h&code=c")).toBeNull();
    expect(parseNativeAuthHandoffCallback("starlane://auth/other?handoff_id=h&code=c")).toBeNull();
  });

  it("rejects a callback without the matching private verifier before exchange", async () => {
    await expect(finishNativeAuthHandoff("starlane://auth/handoff?handoff_id=h&code=c")).rejects.toThrow("could not be verified");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("sends only the matching code, handoff id, and verifier to the exchange endpoint", async () => {
    storage.set("starlane-native-auth-handoff", JSON.stringify({ handoffId: "h", verifier: "v" }));
    vi.mocked(fetch).mockResolvedValue(new Response(JSON.stringify({ access_token: "a", refresh_token: "r", user: {} }), { status: 200 }));
    await finishNativeAuthHandoff("starlane://auth/handoff?handoff_id=h&code=c");
    expect(fetch).toHaveBeenCalledWith("/api/v1/auth/native-handoff/exchange", expect.objectContaining({ body: JSON.stringify({ handoff_id: "h", code: "c", code_verifier: "v" }) }));
    expect(storage.has("starlane-native-auth-handoff")).toBe(false);
  });

  it("retains an opaque handoff id across an SSO redirect and completes it without tokens in the URL", async () => {
    const localStorage = { getItem: () => null };
    vi.stubGlobal("localStorage", localStorage);
    vi.stubGlobal("window", { location: { search: "?native_handoff=h", assign: vi.fn() }, localStorage });
    rememberBrowserNativeAuthHandoff();
    vi.stubGlobal("window", { location: { search: "", assign: vi.fn() }, localStorage });
    vi.mocked(fetch).mockResolvedValue(new Response(JSON.stringify({ handoff_id: "h", code: "c" }), { status: 200 }));
    await completeNativeAuthHandoffInBrowser();
    expect(fetch).toHaveBeenCalledWith("/api/v1/auth/native-handoff/complete", expect.objectContaining({ body: JSON.stringify({ handoff_id: "h" }) }));
    expect(window.location.assign).toHaveBeenCalledWith("starlane://auth/handoff?handoff_id=h&code=c");
    expect(storage.has("starlane-browser-native-auth-handoff")).toBe(false);
  });
});
