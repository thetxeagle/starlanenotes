import { afterEach, describe, expect, it, vi } from "vitest";
import { PLUGIN_ARCHIVE_STORAGE_KEYS } from "@/lib/pluginInstallRecords";
import { readPluginArchive } from "./pluginArchive";

afterEach(() => vi.unstubAllGlobals());

describe("read-only legacy plugin archive", () => {
  it("preserves exact original strings including malformed JSON and unknown fields", () => {
    const values = [
      '  [{"unknown":{"enabled":true,"future":null}}]\n',
      '{malformed\n audit text',
      '\u0000raw unicode 🦅\r\n  tail ',
    ];
    const getItem = vi.fn((key: string) => values[PLUGIN_ARCHIVE_STORAGE_KEYS.findIndex(storedKey => storedKey === key)] ?? null);
    const setItem = vi.fn();
    const removeItem = vi.fn();
    const clear = vi.fn();
    const storage = { getItem, setItem, removeItem, clear };
    const archive = readPluginArchive(storage);
    expect(archive.browserAvailable).toBe(true);
    expect(archive.entries).toEqual(PLUGIN_ARCHIVE_STORAGE_KEYS.map((key, index) => ({ key, state: "present", raw: values[index] })));
    expect(getItem.mock.calls).toEqual(PLUGIN_ARCHIVE_STORAGE_KEYS.map(key => [key]));
    expect(setItem).not.toHaveBeenCalled();
    expect(removeItem).not.toHaveBeenCalled();
    expect(clear).not.toHaveBeenCalled();
  });

  it("distinguishes stored empty strings from absent values", () => {
    const archive = readPluginArchive({ getItem: key => key === PLUGIN_ARCHIVE_STORAGE_KEYS[0] ? "" : null });
    expect(archive.entries[0]).toEqual({ key: PLUGIN_ARCHIVE_STORAGE_KEYS[0], state: "present", raw: "" });
    expect(archive.entries.slice(1)).toEqual(PLUGIN_ARCHIVE_STORAGE_KEYS.slice(1).map(key => ({ key, state: "absent", raw: null })));
  });

  it("isolates per-key access errors without losing readable neighboring values", () => {
    const archive = readPluginArchive({ getItem: key => {
      if (key === PLUGIN_ARCHIVE_STORAGE_KEYS[1]) throw new Error("Denied");
      return key === PLUGIN_ARCHIVE_STORAGE_KEYS[0] ? "original" : null;
    } });
    expect(archive.browserAvailable).toBe(true);
    expect(archive.entries.map(entry => [entry.state, entry.raw])).toEqual([
      ["present", "original"], ["unavailable", null], ["absent", null],
    ]);
  });

  it("reports inaccessible storage separately from missing records", () => {
    expect(readPluginArchive(null)).toEqual({browserAvailable:false,entries:PLUGIN_ARCHIVE_STORAGE_KEYS.map(key => ({key,state:"unavailable",raw:null}))});
    vi.stubGlobal("window", Object.defineProperty({}, "localStorage", {get() {throw new Error("Storage access denied");}}));
    expect(readPluginArchive()).toEqual(readPluginArchive(null));
  });

  it("does not require a browser during server rendering", () => {
    vi.stubGlobal("window", undefined);
    expect(readPluginArchive()).toEqual(readPluginArchive(null));
  });
});
