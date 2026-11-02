import { describe, expect, it } from "vitest";
import {
  isLikelyMobileBrowser,
  isLikelyMobilePlatform,
  shouldUseMobileBrowserLayout,
} from "./useRuntimePlatform";

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
    const iPadDesktopClassSignals = {
      userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
      platform: "MacIntel",
      maxTouchPoints: 5,
    };

    expect(isLikelyMobileBrowser(iPadDesktopClassSignals)).toBe(true);
    expect(shouldUseMobileBrowserLayout(false, iPadDesktopClassSignals)).toBe(true);
  });

  it("keeps an iPad browser in the touch-first shell even above the phone breakpoint", () => {
    expect(
      shouldUseMobileBrowserLayout(false, {
        userAgent: "Mozilla/5.0 (iPad; CPU OS 26_0 like Mac OS X)",
        platform: "iPad",
        maxTouchPoints: 5,
      }),
    ).toBe(true);
  });

  it("keeps a narrow normal desktop browser in the responsive mobile shell", () => {
    expect(
      shouldUseMobileBrowserLayout(true, {
        userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        platform: "MacIntel",
        maxTouchPoints: 0,
      }),
    ).toBe(true);
  });

  it("keeps normal desktop browser signals on the desktop layout", () => {
    const desktopSignals = {
      userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
      platform: "MacIntel",
      maxTouchPoints: 0,
    };

    expect(isLikelyMobileBrowser(desktopSignals)).toBe(false);
    expect(shouldUseMobileBrowserLayout(false, desktopSignals)).toBe(false);
  });
});
