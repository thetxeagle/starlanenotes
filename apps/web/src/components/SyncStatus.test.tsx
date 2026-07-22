import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/hooks/useSyncState", () => ({
  useSyncState: () => "conflict",
}));

import { SyncStatus } from "./SyncStatus";

describe("SyncStatus", () => {
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
});
