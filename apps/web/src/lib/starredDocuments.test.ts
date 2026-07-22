import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  filterStarredDocuments,
  getStarredDocumentKey,
  isDocumentStarred,
  loadStarredDocuments,
  mergeStarredDocumentMetadata,
  readStarredDocuments,
  toggleStarredDocument,
  toggleStarredDocumentSynced,
} from "./starredDocuments";
import { api } from "./api";
import { setCurrentLaunchMode } from "./launchMode";

describe("starredDocuments", () => {
  const store = new Map<string, string>();

  beforeEach(() => {
    store.clear();
    vi.stubGlobal("localStorage", {
      getItem: vi.fn((key: string) => store.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => store.set(key, value)),
      removeItem: vi.fn((key: string) => store.delete(key)),
      clear: vi.fn(() => store.clear()),
    });
    vi.stubGlobal("window", {
      dispatchEvent: vi.fn(),
    });
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-06T12:00:00.000Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
    api.setToken(null);
    setCurrentLaunchMode("server");
    vi.unstubAllGlobals();
  });

  it("builds stable per-space document keys", () => {
    expect(getStarredDocumentKey({ spaceId: "space-1", id: "doc-1" })).toBe(
      "space-1:doc-1",
    );
  });

  it("toggles starred documents in local storage", () => {
    const input = { spaceId: "space-1", id: "doc-1", title: "Roadmap" };

    expect(toggleStarredDocument(input)).toBe(true);
    expect(isDocumentStarred(input)).toBe(true);
    expect(readStarredDocuments()[0]).toMatchObject({
      id: "doc-1",
      spaceId: "space-1",
      title: "Roadmap",
      starred_at: "2026-07-06T12:00:00.000Z",
    });

    expect(toggleStarredDocument(input)).toBe(false);
    expect(isDocumentStarred(input)).toBe(false);
  });

  it("refreshes starred metadata from loaded document lists", () => {
    toggleStarredDocument({ spaceId: "space-1", id: "doc-1", title: "Old" });

    const merged = mergeStarredDocumentMetadata(readStarredDocuments(), [
      {
        spaceId: "space-1",
        id: "doc-1",
        title: "New",
        spaceName: "Ops",
        updated_at: "2026-07-07T00:00:00.000Z",
      },
    ]);

    expect(merged[0]).toMatchObject({
      title: "New",
      spaceName: "Ops",
      updated_at: "2026-07-07T00:00:00.000Z",
    });
  });

  it("filters starred documents by title or space and sorts newest stars first", () => {
    const docs = [
      {
        id: "doc-1",
        spaceId: "space-1",
        title: "Roadmap",
        spaceName: "Ops",
        starred_at: "2026-07-06T12:00:00.000Z",
      },
      {
        id: "doc-2",
        spaceId: "space-2",
        title: "Budget",
        spaceName: "Finance",
        starred_at: "2026-07-07T12:00:00.000Z",
      },
    ];

    expect(filterStarredDocuments(docs, "").map((doc) => doc.id)).toEqual(["doc-2", "doc-1"]);
    expect(filterStarredDocuments(docs, "ops").map((doc) => doc.id)).toEqual(["doc-1"]);
    expect(filterStarredDocuments(docs, "budget").map((doc) => doc.id)).toEqual(["doc-2"]);
  });

  it("sorts starred documents by title or updated timestamp", () => {
    const docs = [
      {
        id: "doc-1",
        spaceId: "space-1",
        title: "Zulu",
        updated_at: "2026-07-07T00:00:00.000Z",
        starred_at: "2026-07-06T12:00:00.000Z",
      },
      {
        id: "doc-2",
        spaceId: "space-1",
        title: "Alpha",
        updated_at: "2026-07-06T00:00:00.000Z",
        starred_at: "2026-07-07T12:00:00.000Z",
      },
      {
        id: "doc-3",
        spaceId: "space-2",
        title: "Middle",
        updated_at: "2026-07-08T00:00:00.000Z",
        starred_at: "2026-07-05T12:00:00.000Z",
      },
    ];

    expect(filterStarredDocuments(docs, "", "title_asc").map((doc) => doc.id)).toEqual([
      "doc-2",
      "doc-3",
      "doc-1",
    ]);
    expect(filterStarredDocuments(docs, "", "updated_desc").map((doc) => doc.id)).toEqual([
      "doc-3",
      "doc-1",
      "doc-2",
    ]);
  });

  it("loads server-backed starred documents into the local cache", async () => {
    setCurrentLaunchMode("server");
    api.setToken("token");
    const fetchMock = vi.fn(async () => ({
      ok: true,
      status: 200,
      json: async () => [
        {
          id: "doc-1",
          space_id: "space-1",
          space_name: "Ops",
          title: "Synced",
          starred_at: "2026-07-06T12:00:00.000Z",
        },
      ],
    }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(loadStarredDocuments("space-1")).resolves.toMatchObject([
      { id: "doc-1", spaceId: "space-1", title: "Synced", spaceName: "Ops" },
    ]);
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/spaces/space-1/documents/starred",
      expect.objectContaining({ method: "GET" }),
    );
  });

  it("syncs star toggles through the server API when authenticated", async () => {
    setCurrentLaunchMode("server");
    api.setToken("token");
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) => ({
      ok: true,
      status: init?.method === "POST" ? 204 : 200,
      json: async () => [],
    }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      toggleStarredDocumentSynced({ spaceId: "space-1", id: "doc-1", title: "Roadmap" }),
    ).resolves.toBe(true);
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/spaces/space-1/documents/doc-1/star",
      expect.objectContaining({ method: "POST" }),
    );
  });
});
