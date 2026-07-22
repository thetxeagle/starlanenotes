import { describe, expect, it } from "vitest";
import { isLikelyMobileBrowser, isLikelyMobilePlatform } from "./useRuntimePlatform";

describe("useRuntimePlatform helpers", () => {
  it("detects native mobile platforms reported by Tauri", () => {
    expect(isLikelyMobilePlatform("ios")).toBe(true);
    expect(isLikelyMobilePlatform("android")).toBe(true);
    expect(isLikelyMobilePlatform("macos")).toBe(false);
    expect(isLikelyMobilePlatform("windows")).toBe(false);
  });

  it("detects iPhone and Android webviews from browser signals", () => {
    expect(
      isLikelyMobileBrowser({
        userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 26_5 like Mac OS X)",
        platform: "iPhone",
        maxTouchPoints: 5,
      }),
    ).toBe(true);

    expect(
      isLikelyMobileBrowser({
        userAgent: "Mozilla/5.0 (Linux; Android 16; Pixel)",
        platform: "Linux armv8l",
        maxTouchPoints: 5,
      }),
    ).toBe(true);
  });

  it("treats iPad desktop-class user agents as mobile when touch points are present", () => {
    expect(
      isLikelyMobileBrowser({
        userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        platform: "MacIntel",
        maxTouchPoints: 5,
      }),
    ).toBe(true);
  });

  it("keeps normal desktop browser signals on the desktop layout", () => {
    expect(
      isLikelyMobileBrowser({
        userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        platform: "MacIntel",
        maxTouchPoints: 0,
      }),
    ).toBe(false);
  });
});
