import { describe, expect, it } from "vitest";
import {
  getLocalSpaceEditState,
  getSpaceSettingsRoute,
  sortSpacesForSettings,
} from "./SpaceSettingsPage";
import type { SpaceRecord } from "@/lib/dataClient";

describe("SpaceSettingsPage helpers", () => {
  it("sorts local settings spaces alphabetically without mutating the source list", () => {
    const spaces: SpaceRecord[] = [
      { id: "space-b", name: "Zeta", slug: "zeta" },
      { id: "space-a", name: "Alpha", slug: "alpha" },
      { id: "space-c", name: "Archive", slug: "archive" },
    ];

    const sorted = sortSpacesForSettings(spaces);

    expect(sorted.map((space) => space.name)).toEqual(["Alpha", "Archive", "Zeta"]);
    expect(spaces.map((space) => space.name)).toEqual(["Zeta", "Alpha", "Archive"]);
  });

  it("builds the active-space settings route used by local switch/create actions", () => {
    expect(getSpaceSettingsRoute("space-1")).toBe("/spaces/space-1/settings");
    expect(getSpaceSettingsRoute("space/one")).toBe("/spaces/space%2Fone/settings");
  });

  it("marks the loaded local space as editing and other spaces as editable", () => {
    expect(getLocalSpaceEditState({ id: "space-1" }, "space-1")).toBe("editing");
    expect(getLocalSpaceEditState({ id: "space-2" }, "space-1")).toBe("edit");
    expect(getLocalSpaceEditState({ id: "space-2" }, null)).toBe("edit");
  });
});
