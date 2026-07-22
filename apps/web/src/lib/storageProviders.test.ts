import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  getStorageMode,
  getStorageSelection,
  getSyncProvider,
  hasStoredStorageSelection,
  listProvidersForTarget,
  setStorageMode,
  setSyncProvider,
  supportsStorageSelection,
  type StorageSelection,
} from "./storageProviders";

describe("storageProviders", () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    (globalThis as { window?: unknown }).window = {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => {
          storage.set(key, value);
        },
      },
    };
  });

  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
  });

  it("defaults to server mode with no sync provider", () => {
    expect(getStorageMode()).toBe("server");
    expect(getSyncProvider()).toBe("none");
    expect(getStorageSelection()).toEqual({ mode: "server", provider: "none" });
    expect(hasStoredStorageSelection()).toBe(false);
  });

  it("persists and restores a local + icloud selection", () => {
    setStorageMode("local");
    setSyncProvider("icloud-drive");
    expect(hasStoredStorageSelection()).toBe(true);
    expect(getStorageSelection()).toEqual({
      mode: "local",
      provider: "icloud-drive",
    });
  });

  it("filters providers by runtime target", () => {
    const webProviders = listProvidersForTarget("web").map((p) => p.kind);
    expect(webProviders).toContain("none");
    expect(webProviders).toContain("google-drive");
    expect(webProviders).not.toContain("icloud-drive");
  });

  it("validates whether local selection is supported by target", () => {
    const localICloud: StorageSelection = { mode: "local", provider: "icloud-drive" };
    const localGoogle: StorageSelection = { mode: "local", provider: "google-drive" };

    expect(supportsStorageSelection(localICloud, "tauri-mobile")).toBe(true);
    expect(supportsStorageSelection(localICloud, "web")).toBe(false);
    expect(supportsStorageSelection(localGoogle, "web")).toBe(true);
  });
});
