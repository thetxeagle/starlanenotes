import { describe, expect, it } from "vitest";
import { getSpaceHomeDocumentPath, getSpaceHomeSectionPath } from "./SpaceHomePage";

describe("SpaceHomePage routes", () => {
  it("builds encoded document routes", () => {
    expect(getSpaceHomeDocumentPath("space-1", "doc-1")).toBe("/spaces/space-1/docs/doc-1");
    expect(getSpaceHomeDocumentPath("space/one", "doc/one")).toBe(
      "/spaces/space%2Fone/docs/doc%2Fone",
    );
  });

  it("builds encoded section routes", () => {
    expect(getSpaceHomeSectionPath("space-1", "recent")).toBe("/spaces/space-1/recent");
    expect(getSpaceHomeSectionPath("space/one", "journal")).toBe(
      "/spaces/space%2Fone/journal",
    );
    expect(getSpaceHomeSectionPath("space/one", "graph")).toBe("/spaces/space%2Fone/graph");
    expect(getSpaceHomeSectionPath("space/one", "workflows")).toBe(
      "/spaces/space%2Fone/workflows",
    );
  });
});
