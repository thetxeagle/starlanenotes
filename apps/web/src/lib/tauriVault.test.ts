import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearCorruptedVault,
  getCorruptedVaultRecord,
  markCorruptedVault,
  readCorruptedVaults,
} from "./tauriVault";

describe("tauriVault corrupted vault markers", () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      clear: () => store.clear(),
      getItem: (key: string) => store.get(key) ?? null,
      removeItem: (key: string) => store.delete(key),
      setItem: (key: string, value: string) => store.set(key, value),
    });
  });

  it("records and clears corrupted vaults by normalized path", () => {
    markCorruptedVault("/vaults/The Eagle/", "Snapshot is missing its personal space.");

    expect(readCorruptedVaults()).toHaveLength(1);
    expect(getCorruptedVaultRecord("/vaults/The Eagle")?.name).toBe("The Eagle");
    expect(getCorruptedVaultRecord("/vaults/The Eagle")?.message).toBe(
      "Snapshot is missing its personal space.",
    );

    clearCorruptedVault("/vaults/The Eagle");

    expect(readCorruptedVaults()).toEqual([]);
  });
});
