import { describe, expect, it } from "vitest";
import {
  getPersonalSpaceOwnerLabel,
  getSpaceOwnerIdentity,
  isPersonalSpaceLabelCandidate,
} from "./spaceLabels";

describe("space label helpers", () => {
  it("formats stable usernames for personal-space labels", () => {
    expect(
      getPersonalSpaceOwnerLabel({
        id: "space-1",
        name: "Eagle's Space",
        slug: "eagles-space",
        owner_username: "eagle_117",
        owner_display_name: "The Eagle",
      }),
    ).toBe("Personal space · @eagle_117");
  });

  it("falls back to display names when owner username is an email fallback", () => {
    expect(
      getPersonalSpaceOwnerLabel({
        id: "space-1",
        name: "Eagle's Space",
        slug: "personal-user-1",
        owner_username: "eagle@example.test",
        owner_display_name: "The Eagle",
      }),
    ).toBe("Personal space · The Eagle");
  });

  it("detects the local personal space and avoids labeling team spaces", () => {
    expect(
      isPersonalSpaceLabelCandidate({
        id: "local-space",
        name: "Field Notes",
        slug: "local-space",
      }),
    ).toBe(true);
    expect(
      getPersonalSpaceOwnerLabel({
        id: "space-ops",
        name: "Operations",
        slug: "operations",
        owner_username: "eagle",
      }),
    ).toBeNull();
  });

  it("keeps non-stable owner usernames readable without an at-prefix", () => {
    expect(
      getSpaceOwnerIdentity({
        id: "space-1",
        name: "Personal Space",
        slug: "personal-space",
        owner_username: "eagle@example.test",
      }),
    ).toBe("eagle@example.test");
  });
});
