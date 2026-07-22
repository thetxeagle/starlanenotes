import { describe, expect, it } from "vitest";
import { getRecentKey, getSwipeAction, sortRecentItems } from "./mobileRecentUtils";

describe("mobileRecentUtils", () => {
  it("builds stable recent keys", () => {
    expect(getRecentKey({ spaceId: "space-1", id: "doc-9" })).toBe("space-1:doc-9");
  });

  it("detects swipe actions using horizontal threshold and axis dominance", () => {
    expect(getSwipeAction(80, 10)).toBe("pin");
    expect(getSwipeAction(-90, 15)).toBe("delete");
    expect(getSwipeAction(40, 5)).toBeNull();
    expect(getSwipeAction(90, 120)).toBeNull();
  });

  it("sorts pinned docs first, then by updated_at descending", () => {
    const docs = [
      { spaceId: "a", id: "1", updated_at: "2026-04-13T12:00:00Z" },
      { spaceId: "a", id: "2", updated_at: "2026-04-14T12:00:00Z" },
      { spaceId: "a", id: "3", updated_at: "2026-04-12T12:00:00Z" },
    ];
    const pinned = { "a:3": true } as Record<string, true>;

    const result = sortRecentItems(docs, pinned);
    expect(result.map((d) => d.id)).toEqual(["3", "2", "1"]);
  });
});
