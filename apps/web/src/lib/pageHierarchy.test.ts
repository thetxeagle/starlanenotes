import { describe, expect, it } from "vitest";
import { buildPageCatalog, pageCatalogMoveTargets, buildPageIndex, createPageTraversalBudget, getPageAncestors, getPageSubtree, validatePageDestination, type PageHierarchyNode } from "./pageHierarchy";
function chain(count: number, space = "s"): PageHierarchyNode[] {
  return Array.from({ length: count }, (_, n) => ({ id: `f${n}`, space_id: space, parent_id: n ? `f${n - 1}` : undefined }));
}
describe("bounded iterative page topology", () => {
  it.each([4, 32, 256, 10_000])("preserves a %i-level chain without recursion or flattening", count => {
    const rows = chain(count);
    const before = JSON.stringify(rows);
    const index = buildPageIndex(rows);
    const budget = createPageTraversalBudget();
    expect(getPageAncestors(index, "s", `f${count - 1}`, budget)).toEqual([...rows].reverse());
    expect(getPageSubtree(index, "s", "f0", budget)).toEqual(rows);
    expect(budget.size).toBe(count);
    expect(JSON.stringify(rows)).toBe(before);
  });
  it("counts a broad closure and destination together, allowing exact 10000 but refusing the next new node", () => {
    const rows = [{ id: "root", space_id: "s" }, ...Array.from({ length: 9998 }, (_, n) => ({ id: `child${n}`, space_id: "s", parent_id: "root" })), { id: "target", space_id: "s" }];
    const budget = createPageTraversalBudget();
    validatePageDestination(buildPageIndex(rows), "s", undefined, "target", "root", budget);
    expect(budget.size).toBe(10000);
    expect(() => budget.visit("new-node")).toThrow("10000 participating nodes");
    rows.push({ id: "one-too-many", space_id: "s", parent_id: "root" });
    expect(() => validatePageDestination(buildPageIndex(rows), "s", undefined, "target", "root")).toThrow("10000 participating nodes");
  });
  it("does not charge unrelated workspace records or duplicate visits", () => {
    const rows = chain(20_000, "unrelated");
    const index = buildPageIndex([...rows, { id: "selected", space_id: "s" }]);
    const budget = createPageTraversalBudget();
    getPageSubtree(index, "s", "selected", budget);
    getPageAncestors(index, "s", "selected", budget);
    expect(budget.size).toBe(1);
  });
  it("refuses cycles, orphan parents, foreign edges, self/descendant placement and selected duplicate identity without mutation", () => {
    const rows = chain(32);
    const before = JSON.stringify(rows);
    const index = buildPageIndex(rows);
    expect(() => validatePageDestination(index, "s", undefined, "f0", "f0")).toThrow("inside itself");
    expect(() => validatePageDestination(index, "s", undefined, "f31", "f0")).toThrow("descendants");
    expect(() => getPageAncestors(buildPageIndex([{ ...rows[0], parent_id: "missing" }]), "s", "f0")).toThrow("Page not found");
    expect(() => getPageSubtree(buildPageIndex([...rows, { id: "foreign", space_id: "other", parent_id: "f0" }]), "s", "f0")).toThrow("Page not found");
    expect(() => getPageSubtree(buildPageIndex([...rows, { ...rows[0], parent_id: "f31" }]), "s", "f0")).toThrow("duplicate identity");
    expect(() => getPageAncestors(buildPageIndex(rows.map(row => row.id === "f0" ? { ...row, parent_id: "f31" } : row)), "s", "f31")).toThrow("cycle");
    expect(JSON.stringify(rows)).toBe(before);
  });
});

describe("page rendering catalog resource behavior", () => {
  it("resolves reversed deep input and every destination with linear record reads", () => {
    let reads = 0;
    const count = 10_000;
    const rows = Array.from({ length: count }, (_, n) => ({
      get id() { reads++; return `r${n}`; },
      get space_id() { reads++; return "s"; },
      get parent_id() { reads++; return n ? `r${n - 1}` : undefined; },
      get title() { reads++; return `Level ${n}`; },
    })).reverse();
    const catalog = buildPageCatalog(rows);
    expect(catalog.depths.get("r9999")).toBe(10000);
    expect(catalog.children.get("r0")?.[0].id).toBe("r1");
    expect(catalog.subtreeSizes.get("r0")).toBe(10000);
    const targets = pageCatalogMoveTargets(catalog, "s", "r9999", catalog, "s");
    expect(targets.has("r0")).toBe(true);
    expect(targets.has("r9998")).toBe(false); // Current parent is not a relocation.
    expect(targets.has("r9999")).toBe(false);
    expect(reads).toBeLessThan(count * 90);
    expect(catalog.labels.get("r9999")!.length).toBeLessThanOrEqual(2048);
  });
  it("keeps invalid/cyclic/foreign/duplicate branches out of render order and detects invalid subtree participation", () => {
    const catalog = buildPageCatalog([
      { id: "root", space_id: "s", title: "Root" },
      { id: "foreign", space_id: "other", parent_id: "root", title: "Foreign" },
      { id: "orphan", space_id: "s", parent_id: "missing", title: "Orphan" },
      { id: "cycle-a", space_id: "s", parent_id: "cycle-b", title: "A" },
      { id: "cycle-b", space_id: "s", parent_id: "cycle-a", title: "B" },
      { id: "duplicate", space_id: "s", title: "One" },
      { id: "duplicate", space_id: "s", title: "Two" },
    ]);
    expect(catalog.order.map(row => row.id)).toEqual(["root"]);
    expect(catalog.subtreeValid.get("root")).toBe(false);
    expect(() => pageCatalogMoveTargets(catalog, "s", "root", catalog, "s")).toThrow();
  });
  it("counts source closure and additional target ancestry together without charging unrelated roots", () => {
    const source = buildPageCatalog(chain(9999));
    const target = buildPageCatalog([{ id: "target-root", space_id: "t" }, { id: "target-child", space_id: "t", parent_id: "target-root" }, ...Array.from({ length: 10001 }, (_, n) => ({ id: `other-${n}`, space_id: "t" }))]);
    const targets = pageCatalogMoveTargets(source, "s", "f0", target, "t");
    expect(targets.has("target-root")).toBe(true);
    expect(targets.has("target-child")).toBe(false);
    expect(targets.has("other-10000")).toBe(true);
  });
});

it("validates the source closure in its original folder while binding destination parents to the target folder", () => {
  const rows = [{ id: "source", space_id: "s", folder_id: "old" }, { id: "child", space_id: "s", folder_id: "old", parent_id: "source" }, { id: "target", space_id: "s", folder_id: "new" }];
  const index = buildPageIndex(rows);
  expect(() => validatePageDestination(index, "s", "new", "target", "source")).not.toThrow();
  expect(() => validatePageDestination(index, "s", "old", "target", "source")).toThrow("same folder");
  expect(() => getPageSubtree(buildPageIndex(rows.map(row => row.id === "child" ? { ...row, folder_id: "different" } : row)), "s", "source")).toThrow("same folder");
});
