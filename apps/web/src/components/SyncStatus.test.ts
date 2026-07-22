import { describe, expect, it } from "vitest";
import { SYNC_STATUS_CONFIG } from "./SyncStatus";

describe("SyncStatus", () => {
  it("uses semantic status tokens for sync indicators", () => {
    expect(SYNC_STATUS_CONFIG.synced.color).toBe("bg-success");
    expect(SYNC_STATUS_CONFIG.pending.color).toBe("bg-warning");
    expect(SYNC_STATUS_CONFIG.conflict.color).toBe("bg-accent");
    expect(SYNC_STATUS_CONFIG.error.color).toBe("bg-danger");

    for (const status of Object.values(SYNC_STATUS_CONFIG)) {
      expect(status.color).not.toMatch(/green-|yellow-|orange-|red-/);
    }
  });
});
