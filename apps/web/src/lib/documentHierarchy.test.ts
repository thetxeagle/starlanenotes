import { describe, expect, it } from "vitest";
import { buildDocumentParentPath } from "./documentHierarchy";

const pages = [
  { id: "root", title: "Root", parent_id: undefined },
  { id: "parent", title: "Parent with a long title", parent_id: "root" },
  { id: "child", title: "Child", parent_id: "parent" },
];

describe("parent page breadcrumbs", () => {
  it("orders ancestors from outermost to immediate parent", () => {
    expect(buildDocumentParentPath(pages, "child")).toEqual([
      { id: "root", title: "Root" },
      { id: "parent", title: "Parent with a long title" },
    ]);
  });

  it("uses the confirmed move destination instead of a stale list", () => {
    expect(buildDocumentParentPath(pages, "child", "root")).toEqual([
      { id: "root", title: "Root" },
    ]);
    expect(buildDocumentParentPath(pages, "child", null)).toEqual([]);
  });

  it("does not invent unavailable ancestors or include the current page in a cycle", () => {
    expect(buildDocumentParentPath(pages, "child", "unavailable")).toEqual([]);
    expect(
      buildDocumentParentPath(
        [
          { id: "child", title: "Child", parent_id: "parent" },
          { id: "parent", title: "Parent", parent_id: "child" },
        ],
        "child",
      ),
    ).toEqual([{ id: "parent", title: "Parent" }]);
  });
});
