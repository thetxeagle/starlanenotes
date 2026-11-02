import { describe, expect, it } from "vitest";
import {
  DEFAULT_SPACE_ROLE_DEFINITIONS,
  getSpaceRoleLabel,
  normalizeSpaceRole,
} from "./spacePermissions";

describe("space permissions", () => {
  it("keeps built-in roles ordered from administrative to read-only", () => {
    expect(DEFAULT_SPACE_ROLE_DEFINITIONS.map((role) => role.id)).toEqual([
      "space_admin",
      "editor",
      "commenter",
      "viewer",
    ]);
    expect(DEFAULT_SPACE_ROLE_DEFINITIONS[0]?.permissions).toContain("manage_members");
    expect(
      DEFAULT_SPACE_ROLE_DEFINITIONS[DEFAULT_SPACE_ROLE_DEFINITIONS.length - 1]?.permissions,
    ).toEqual(["view"]);
  });

  it("maps the legacy owner role into the space-admin presentation model", () => {
    expect(normalizeSpaceRole("owner")).toBe("space_admin");
    expect(getSpaceRoleLabel("owner")).toBe("Owner");
    expect(getSpaceRoleLabel("commenter")).toBe("Commenter");
  });
});
