import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  buildSpaceHomePath,
  clearLastViewedSpaceId,
  getLastViewedSpaceHomePath,
  getPreferredSpaceHomePath,
  readLastViewedSpaceId,
  writeLastViewedSpaceId,
} from "./lastViewedSpace";

const localStorageMock = {
  data: new Map<string, string>(),
  getItem: vi.fn((key: string) => localStorageMock.data.get(key) ?? null),
  setItem: vi.fn((key: string, value: string) => {
    localStorageMock.data.set(key, value);
  }),
  removeItem: vi.fn((key: string) => {
    localStorageMock.data.delete(key);
  }),
};

describe("lastViewedSpace", () => {
  beforeEach(() => {
    localStorageMock.data.clear();
    vi.stubGlobal("window", { localStorage: localStorageMock });
  });

  it("stores and resolves the last viewed space home path", () => {
    expect(readLastViewedSpaceId()).toBeNull();
    expect(getLastViewedSpaceHomePath()).toBe("/");

    writeLastViewedSpaceId("space-1");

    expect(readLastViewedSpaceId()).toBe("space-1");
    expect(getLastViewedSpaceHomePath()).toBe("/spaces/space-1/docs");
    expect(getLastViewedSpaceHomePath("space-2")).toBe("/spaces/space-2/docs");

    clearLastViewedSpaceId();
    expect(readLastViewedSpaceId()).toBeNull();
  });

  it("encodes space IDs when building startup home paths", () => {
    expect(buildSpaceHomePath("space one")).toBe("/spaces/space%20one/docs");
    expect(buildSpaceHomePath("space/one")).toBe("/spaces/space%2Fone/docs");
    expect(getLastViewedSpaceHomePath("space/one")).toBe("/spaces/space%2Fone/docs");
  });

  it("prefers the last viewed space when resolving the mobile root landing page", () => {
    writeLastViewedSpaceId("space-2");

    expect(getPreferredSpaceHomePath([{ id: "space-1" }, { id: "space-2" }])).toBe(
      "/spaces/space-2/docs",
    );
  });

  it("falls back to the first available space when the preferred space is unavailable", () => {
    writeLastViewedSpaceId("missing-space");

    expect(getPreferredSpaceHomePath([{ id: "space/one" }, { id: "space-2" }])).toBe(
      "/spaces/space%2Fone/docs",
    );
  });

  it("returns null when there is no space landing page to open", () => {
    expect(getPreferredSpaceHomePath([])).toBeNull();
  });
});
