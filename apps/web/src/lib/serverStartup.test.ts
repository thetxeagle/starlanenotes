import { afterEach, describe, expect, it, vi } from "vitest";
import { checkServerStartup, normalizeServerUrl } from "./serverStartup";

describe("serverStartup", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("normalizes server URLs before startup checks", () => {
    expect(normalizeServerUrl(" https://notes.example.com/// ")).toBe(
      "https://notes.example.com",
    );
    expect(normalizeServerUrl("http://notes.local/")).toBe("http://notes.local");
  });

  it("rejects missing or unsupported server URLs", () => {
    expect(() => normalizeServerUrl("")).toThrow("Enter a server URL.");
    expect(() => normalizeServerUrl("notes.example.com")).toThrow(
      "Enter a valid server URL",
    );
    expect(() => normalizeServerUrl("ftp://notes.example.com")).toThrow(
      "must start with http:// or https://",
    );
  });

  it("checks the auth config endpoint without caching", async () => {
    const fetchMock = vi.fn(async () => ({
      ok: true,
      json: async () => ({ oidc_enabled: true, oidc_display_name: "SSO" }),
    }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(checkServerStartup("https://notes.example.com/")).resolves.toEqual({
      serverUrl: "https://notes.example.com",
      config: { oidc_enabled: true, oidc_display_name: "SSO" },
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "https://notes.example.com/api/v1/auth/config",
      expect.objectContaining({ cache: "no-store" }),
    );
  });

  it("turns unhealthy responses into user-facing startup errors", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: false,
        status: 503,
        json: async () => ({}),
      })),
    );

    await expect(checkServerStartup("https://notes.example.com")).rejects.toThrow(
      "Server returned 503.",
    );
  });

  it("explains WebKit load failures as connection, TLS, or CORS problems", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("Load failed");
      }),
    );

    await expect(checkServerStartup("https://notes.example.com")).rejects.toThrow(
      "production CORS allows the native app origin",
    );
  });
});
