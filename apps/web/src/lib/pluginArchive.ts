import { PLUGIN_ARCHIVE_STORAGE_KEYS } from "@/lib/pluginInstallRecords";

export interface PluginArchiveStorage {
  getItem(key: string): string | null;
}

export interface PluginArchiveEntry {
  key: string;
  state: "present" | "absent" | "unavailable";
  raw: string | null;
}

export interface PluginArchive {
  browserAvailable: boolean;
  entries: PluginArchiveEntry[];
}

function browserStorage(): PluginArchiveStorage | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

/** Read original storage strings without parsing, normalizing or writing them. */
export function readPluginArchive(
  storage: PluginArchiveStorage | null = browserStorage(),
): PluginArchive {
  return {
    browserAvailable: storage !== null,
    entries: PLUGIN_ARCHIVE_STORAGE_KEYS.map((key) => {
      if (!storage) return { key, state: "unavailable", raw: null };
      try {
        const raw = storage.getItem(key);
        return { key, state: raw === null ? "absent" : "present", raw };
      } catch {
        return { key, state: "unavailable", raw: null };
      }
    }),
  };
}
