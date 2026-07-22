import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  deleteSavedSearchView,
  readSavedSearchViews,
  saveSearchView,
} from "./savedSearches";

describe("savedSearches", () => {
  const store = new Map<string, string>();

  beforeEach(() => {
    store.clear();
    vi.stubGlobal("localStorage", {
      getItem: vi.fn((key: string) => store.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => store.set(key, value)),
      removeItem: vi.fn((key: string) => store.delete(key)),
      clear: vi.fn(() => store.clear()),
    });
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-06T12:00:00.000Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("saves compact named search views", () => {
    const views = saveSearchView("Roadmap", {
      query: " roadmap ",
      filters: { tag: "Planning", property_name: "" },
      starredOnly: false,
    });

    expect(views[0]).toMatchObject({
      name: "Roadmap",
      query: "roadmap",
      filters: { tag: "Planning" },
      starredOnly: false,
      created_at: "2026-07-06T12:00:00.000Z",
    });
  });

  it("replaces views by name and deletes by id", () => {
    const first = saveSearchView("Pinned", {
      query: "",
      filters: {},
      starredOnly: true,
    })[0];
    saveSearchView("Pinned", {
      query: "updated",
      filters: {},
      starredOnly: false,
    });

    expect(readSavedSearchViews()).toHaveLength(1);
    expect(readSavedSearchViews()[0]).toMatchObject({ query: "updated", starredOnly: false });

    expect(deleteSavedSearchView(first.id)).toEqual([]);
  });
});
