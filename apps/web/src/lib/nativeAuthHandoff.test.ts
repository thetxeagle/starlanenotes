import { beforeEach, describe, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  beginNativeAuthHandoff,
  completeNativeAuthHandoffInBrowser,
  exchangeNativeAuthHandoff,
  finishNativeAuthHandoff,
  getNativeAuthReturnUrl,
  parseNativeAuthHandoffCallback,
  rememberBrowserNativeAuthHandoff,
} from "./nativeAuthHandoff";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  getApiBase: () => "/api/v1",
  getSessionClientDetails: () => ({
    session_kind: "app",
    device_name: "Starlane Notes desktop app",
    client_platform: "desktop_app",
  }),
  isTauri: () => true,
}));

const storage = new Map<string, string>();

beforeEach(() => {
  storage.clear();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  });
  vi.stubGlobal("sessionStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  });
  vi.stubGlobal("fetch", vi.fn());
  vi.mocked(openUrl).mockReset();
});

describe("native authentication handoff", () => {
  it("accepts only the configured opaque Starlane handoff callback", () => {
    expect(parseNativeAuthHandoffCallback("starlane://auth/handoff?handoff_id=h")).toEqual({
      handoffId: "h",
    });
    expect(parseNativeAuthHandoffCallback("https://auth/handoff?handoff_id=h")).toBeNull();
    expect(parseNativeAuthHandoffCallback("starlane://auth/other?handoff_id=h")).toBeNull();
  });

  it("rejects a callback without the matching private verifier before exchange", async () => {
    await expect(
      finishNativeAuthHandoff("starlane://auth/handoff?handoff_id=h"),
    ).rejects.toThrow("could not be verified");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("opens the dedicated app sign-in page and persists the verifier across app restarts", async () => {
    vi.mocked(fetch).mockResolvedValue(
      new Response(
        JSON.stringify({
          handoff_id: "h",
          authorization_url: "https://starlane.example/app-signin/?handoff_id=h",
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );

    await beginNativeAuthHandoff();

    expect(openUrl).toHaveBeenCalledWith(
      new URL("https://starlane.example/app-signin/?handoff_id=h"),
    );
    expect(storage.has("starlane-native-auth-handoff")).toBe(true);
  });

  it("clears the verifier when the system browser cannot be opened", async () => {
    vi.mocked(fetch).mockResolvedValue(
      new Response(
        JSON.stringify({
          handoff_id: "h",
          authorization_url: "https://starlane.example/app-signin/?handoff_id=h",
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );
    vi.mocked(openUrl).mockRejectedValue(new Error("not available"));

    await expect(beginNativeAuthHandoff()).rejects.toThrow(
      "Unable to open the system browser",
    );
    expect(storage.has("starlane-native-auth-handoff")).toBe(false);
  });

  it("polls with the private verifier and retains it while browser approval is pending", async () => {
    storage.set(
      "starlane-native-auth-handoff",
      JSON.stringify({ handoffId: "h", verifier: "v" }),
    );
    vi.mocked(fetch).mockResolvedValue(
      new Response(JSON.stringify({ handoff_id: "h", status: "pending" }), {
        status: 202,
      }),
    );

    await expect(exchangeNativeAuthHandoff()).resolves.toBeNull();
    expect(fetch).toHaveBeenCalledWith(
      "/api/v1/auth/native-handoff/exchange",
      expect.objectContaining({
        body: JSON.stringify({
          handoff_id: "h",
          code_verifier: "v",
          session_kind: "app",
          device_name: "Starlane Notes desktop app",
          client_platform: "desktop_app",
        }),
      }),
    );
    expect(storage.has("starlane-native-auth-handoff")).toBe(true);
  });

  it("collects approved tokens directly and clears the private verifier", async () => {
    storage.set(
      "starlane-native-auth-handoff",
      JSON.stringify({ handoffId: "h", verifier: "v" }),
    );
    vi.mocked(fetch).mockResolvedValue(
      new Response(
        JSON.stringify({ access_token: "a", refresh_token: "r", user: {} }),
        { status: 200 },
      ),
    );

    await expect(exchangeNativeAuthHandoff()).resolves.toEqual({
      access_token: "a",
      refresh_token: "r",
      user: {},
    });
    expect(storage.has("starlane-native-auth-handoff")).toBe(false);
  });

  it("retains the app verifier through a transient server failure", async () => {
    storage.set(
      "starlane-native-auth-handoff",
      JSON.stringify({ handoffId: "h", verifier: "v" }),
    );
    vi.mocked(fetch).mockResolvedValue(new Response(null, { status: 503 }));

    await expect(exchangeNativeAuthHandoff()).resolves.toBeNull();
    expect(storage.has("starlane-native-auth-handoff")).toBe(true);
  });

  it("approves a retained browser request without returning tokens in the URL", async () => {
    const localStorage = { getItem: () => "browser-access-token" };
    vi.stubGlobal("localStorage", localStorage);
    vi.stubGlobal("window", { location: { search: "?handoff_id=h" }, localStorage });
    rememberBrowserNativeAuthHandoff();
    vi.stubGlobal("window", { location: { search: "" }, localStorage });
    vi.mocked(fetch).mockResolvedValue(
      new Response(JSON.stringify({ handoff_id: "h", status: "approved" }), {
        status: 200,
      }),
    );

    await expect(completeNativeAuthHandoffInBrowser()).resolves.toBe("h");
    expect(fetch).toHaveBeenCalledWith(
      "/api/v1/auth/native-handoff/complete",
      expect.objectContaining({ body: JSON.stringify({ handoff_id: "h" }) }),
    );
    expect(getNativeAuthReturnUrl("h")).toBe("starlane://auth/handoff?handoff_id=h");
    expect(storage.has("starlane-browser-native-auth-handoff")).toBe(false);
  });
});
