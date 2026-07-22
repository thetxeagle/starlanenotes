import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  findSharedSpaceDownloadCheckpoint,
  formatSharedSpaceDownloadCheckpoint,
  readSharedSpaceDownloadCheckpoints,
  recordSharedSpaceDownloadCheckpoint,
  writeSharedSpaceDownloadCheckpoints,
} from "./sharedSpaceDownloads";

describe("shared space download checkpoints", () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal("window", {
      localStorage: {
        clear: () => store.clear(),
        getItem: (key: string) => store.get(key) ?? null,
        removeItem: (key: string) => store.delete(key),
        setItem: (key: string, value: string) => store.set(key, value),
      },
    });
  });

  afterEach(() => {
    window.localStorage.clear();
    vi.unstubAllGlobals();
  });

  it("records the latest checkpoint per space", () => {
    recordSharedSpaceDownloadCheckpoint({
      spaceId: "space-1",
      spaceName: "Ops",
      slug: "ops",
      downloadedAt: "2026-07-07T01:00:00.000Z",
      documentCount: 2,
      folderCount: 1,
      tagCount: 0,
      propertyCount: 0,
    });
    const checkpoints = recordSharedSpaceDownloadCheckpoint({
      spaceId: "space-1",
      spaceName: "Ops",
      slug: "ops",
      downloadedAt: "2026-07-07T02:00:00.000Z",
      documentCount: 4,
      folderCount: 2,
      tagCount: 1,
      propertyCount: 3,
    });

    expect(checkpoints).toHaveLength(1);
    expect(checkpoints[0]).toMatchObject({ spaceId: "space-1", documentCount: 4 });
    expect(readSharedSpaceDownloadCheckpoints()).toEqual(checkpoints);
  });

  it("normalizes stored checkpoint records", () => {
    window.localStorage.setItem(
      "starlane_shared_space_downloads",
      JSON.stringify([
        {
          spaceId: "space-1",
          spaceName: "Ops",
          sourceServerUrl: " https://notes.example.test/ ",
          downloadedByUserId: " user-1 ",
          downloadedByDisplayName: " Ops Admin ",
          downloadedAt: "2026-07-07T01:00:00.000Z",
          documentCount: 2,
        },
        { broken: true },
      ]),
    );

    expect(readSharedSpaceDownloadCheckpoints()).toEqual([
      {
        spaceId: "space-1",
        spaceName: "Ops",
        slug: undefined,
        sourceServerUrl: "https://notes.example.test/",
        downloadedByUserId: "user-1",
        downloadedByDisplayName: "Ops Admin",
        downloadedAt: "2026-07-07T01:00:00.000Z",
        documentCount: 2,
        folderCount: 0,
        tagCount: 0,
        propertyCount: 0,
      },
    ]);
  });

  it("finds and formats checkpoints", () => {
    const checkpoint = {
      spaceId: "space-1",
      spaceName: "Ops",
      downloadedAt: "2026-07-07T01:00:00.000Z",
      documentCount: 1,
      folderCount: 0,
      tagCount: 0,
      propertyCount: 0,
    };
    writeSharedSpaceDownloadCheckpoints([checkpoint]);
    const found = findSharedSpaceDownloadCheckpoint(readSharedSpaceDownloadCheckpoints(), "space-1");

    expect(found).toEqual(checkpoint);
    expect(formatSharedSpaceDownloadCheckpoint(checkpoint)).toContain("1 doc downloaded");
  });
});
