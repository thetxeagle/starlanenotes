import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const syncState = vi.hoisted((): { value: string } => ({ value: "conflict" }));

vi.mock("@/hooks/useSyncState", () => ({
  useSyncState: () => syncState.value,
}));

import { SyncStatus } from "./SyncStatus";

describe("SyncStatus", () => {
  beforeEach(() => {
    syncState.value = "conflict";
  });

  it("routes local sync conflicts to the safe recovery surface", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SyncStatus />
      </MemoryRouter>,
    );

    expect(html).toContain("Sync conflict");
    expect(html).toContain('aria-label="Review sync conflict in Storage settings"');
    expect(html).toContain('href="/admin/storage"');
  });

  it("can keep the healthy synced state quiet in persistent chrome", () => {
    syncState.value = "synced";

    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SyncStatus hideSynced />
      </MemoryRouter>,
    );

    expect(html).toBe("");
  });
});
