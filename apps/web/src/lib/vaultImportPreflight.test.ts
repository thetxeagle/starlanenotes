import { describe, expect, it } from "vitest";
import { formatVaultImportPreflight, summarizeVaultImportSnapshot } from "./vaultImportPreflight";

describe("vault import preflight", () => {
  it("summarizes snapshot counts and sync state", () => {
    const summary = summarizeVaultImportSnapshot({
      schema_version: 1,
      spaces: [{ id: "space-1" }],
      folders: [{ id: "folder-1" }],
      documents: [{ id: "doc-1" }, { id: "doc-2" }],
      assets: [{ id: "asset-1" }],
      templates: [],
      sync_state: {
        state: "conflict",
        queued_operations: [{ id: "op-1" }],
      },
    });

    expect(summary).toEqual({
      schemaVersion: 1,
      spaceCount: 1,
      folderCount: 1,
      documentCount: 2,
      assetCount: 1,
      templateCount: 0,
      syncState: "conflict",
      queuedOperationCount: 1,
    });
    expect(formatVaultImportPreflight(summary)).toContain(
      "1 space, 2 documents, 1 folder, 1 asset, 0 templates",
    );
    expect(formatVaultImportPreflight(summary)).toContain(
      "Sync state: conflict; 1 queued operation.",
    );
  });

  it("handles invalid snapshot values defensively", () => {
    expect(formatVaultImportPreflight(summarizeVaultImportSnapshot(null))).toContain(
      "0 spaces, 0 documents",
    );
  });
});
