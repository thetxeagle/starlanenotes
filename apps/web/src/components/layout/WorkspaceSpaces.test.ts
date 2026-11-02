import { describe, expect, it } from "vitest";
import {
  canCreateWorkspaceSpace,
  getWorkspaceSpaceSections,
  isCurrentWorkspaceSpaceLoad,
  resolveActiveWorkspaceSpace,
  type WorkspaceSpace,
} from "./WorkspaceSpaces";

const spaces: WorkspaceSpace[] = [
  { id: "space-a", name: "Alpha", slug: "alpha", member_role: "owner" },
  { id: "space-b", name: "Bravo", slug: "bravo", member_role: "editor" },
  { id: "space-c", name: "Charlie", slug: "charlie", member_role: "owner" },
  { id: "space-d", name: "Delta", slug: "delta", member_role: "viewer" },
];

describe("workspace space ownership", () => {
  it("prefers the routed space, then the current space, then the first available space", () => {
    expect(resolveActiveWorkspaceSpace(spaces, "space-b", "space-c")?.id).toBe("space-b");
    expect(resolveActiveWorkspaceSpace(spaces, "missing", "space-c")?.id).toBe("space-c");
    expect(resolveActiveWorkspaceSpace(spaces, "missing", "missing")?.id).toBe("space-a");
    expect(resolveActiveWorkspaceSpace([], "space-a", "space-a")).toBeNull();
  });

  it("rejects a superseded space-list response", () => {
    expect(isCurrentWorkspaceSpaceLoad(4, 4)).toBe(true);
    expect(isCurrentWorkspaceSpaceLoad(3, 4)).toBe(false);
  });

  it("allows local workspaces and server administrators to create spaces", () => {
    expect(canCreateWorkspaceSpace(true, "member")).toBe(true);
    expect(canCreateWorkspaceSpace(false, "admin")).toBe(true);
    expect(canCreateWorkspaceSpace(false, "member")).toBe(false);
    expect(canCreateWorkspaceSpace(false, undefined)).toBe(false);
  });

  it("groups recent spaces in stored order, then owned spaces, then remaining spaces", () => {
    const sections = getWorkspaceSpaceSections(spaces, ["space-b", "space-a"]);

    expect(sections.map((section) => section.label)).toEqual(["Recent", "Owned", "All spaces"]);
    expect(sections[0].spaces.map((space) => space.id)).toEqual(["space-b", "space-a"]);
    expect(sections[1].spaces.map((space) => space.id)).toEqual(["space-c"]);
    expect(sections[2].spaces.map((space) => space.id)).toEqual(["space-d"]);
    expect(getWorkspaceSpaceSections(spaces, [], "brav")[0].spaces).toEqual([spaces[1]]);
  });
});
