import { describe, expect, it } from "vitest";
import { resolveLaunchMode } from "./launchMode";

describe("resolveLaunchMode", () => {
  it("keeps selected mode when provider is supported by runtime", () => {
    const result = resolveLaunchMode(
      { mode: "local", provider: "google-drive" },
      "web",
    );
    expect(result.mode).toBe("local");
    expect(result.fallbackApplied).toBe(false);
  });

  it("falls back to server when selected provider is unsupported by runtime", () => {
    const result = resolveLaunchMode(
      { mode: "local", provider: "icloud-drive" },
      "web",
    );
    expect(result.mode).toBe("server");
    expect(result.fallbackApplied).toBe(true);
  });
});

