import { describe, expect, it } from "vitest";
import {
  GRAPH_ERROR_CLASS,
  fitGraphViewport,
  getGraphDocumentPath,
  graphBounds,
  graphNeighborsByNode,
  graphSelectedNeighborIds,
  graphVisibleFocusNodeIds,
  graphVisibleNodeIds,
  graphVisibleStats,
} from "./GraphPage";
import type { GraphRecord } from "@/lib/dataClient";

describe("GraphPage helpers", () => {
  it("builds encoded document routes for graph nodes", () => {
    expect(getGraphDocumentPath("space-1", "doc-1")).toBe("/spaces/space-1/docs/doc-1");
    expect(getGraphDocumentPath("space/one", "doc/one")).toBe("/spaces/space%2Fone/docs/doc%2Fone");
  });

  it("calculates graph bounds with node radius included", () => {
    expect(
      graphBounds([
        { x: 10, y: 20, r: 5 },
        { x: 40, y: 50, r: 10 },
      ]),
    ).toEqual({
      minX: 5,
      minY: 15,
      maxX: 50,
      maxY: 60,
    });
    expect(graphBounds([])).toBeNull();
  });

  it("fits visible nodes into the graph viewport", () => {
    const fit = fitGraphViewport([
      { x: 100, y: 100, r: 10 },
      { x: 300, y: 300, r: 10 },
    ]);

    expect(fit.zoom).toBeGreaterThan(0);
    expect(fit.zoom).toBeLessThanOrEqual(4);
    expect(Number.isFinite(fit.pan.x)).toBe(true);
    expect(Number.isFinite(fit.pan.y)).toBe(true);
  });

  it("summarizes visible graph nodes and edges", () => {
    const graph: GraphRecord = {
      nodes: [
        { id: "a", title: "A", connection_count: 1, orphan: false },
        { id: "b", title: "B", connection_count: 1, orphan: false },
        { id: "c", title: "C", connection_count: 0, orphan: true },
      ],
      edges: [
        { source: "a", target: "b", relation: "link" },
        { source: "a", target: "c", relation: "link" },
      ],
    };

    expect(graphVisibleStats(graph, new Set(["a", "b"]))).toEqual({
      visibleNodes: 2,
      hiddenNodes: 1,
      visibleEdges: 1,
      orphanNodes: 1,
    });
  });

  it("builds bidirectional neighbor sets for focus and selected-node highlighting", () => {
    const neighbors = graphNeighborsByNode(
      [{ id: "a" }, { id: "b" }, { id: "c" }, { id: "orphan" }],
      [
        { source: "a", target: "b", relation: "link" },
        { source: "c", target: "a", relation: "backlink" },
        { source: "missing", target: "a", relation: "ignored" },
      ],
    );

    expect([...neighbors.a].sort()).toEqual(["b", "c"]);
    expect([...neighbors.b]).toEqual(["a"]);
    expect([...neighbors.orphan]).toEqual([]);
    expect([...(graphVisibleFocusNodeIds("a", neighbors) ?? [])].sort()).toEqual(["a", "b", "c"]);
    expect([...(graphSelectedNeighborIds("orphan", neighbors) ?? [])]).toEqual(["orphan"]);
  });

  it("filters visible graph nodes by orphan, folder, degree, and focus rules", () => {
    const visible = graphVisibleNodeIds(
      [
        { id: "a", folder_id: "folder-1", connection_count: 3, orphan: false },
        { id: "b", folder_id: "folder-1", connection_count: 1, orphan: false },
        { id: "c", folder_id: "folder-2", connection_count: 4, orphan: false },
        { id: "root", connection_count: 5, orphan: false },
        { id: "orphan", folder_id: "folder-1", connection_count: 8, orphan: true },
      ],
      {
        focusVisibleNodeIds: new Set(["a", "c", "orphan"]),
        folderFilter: "folder-1",
        minConnections: 2,
        showOrphans: false,
      },
    );

    expect([...visible]).toEqual(["a"]);
    expect([
      ...graphVisibleNodeIds(
        [
          { id: "a", folder_id: "folder-1", connection_count: 3, orphan: false },
          { id: "root", connection_count: 5, orphan: false },
        ],
        {
          folderFilter: "none",
          minConnections: 0,
          showOrphans: true,
        },
      ),
    ]).toEqual(["root"]);
  });

  it("uses semantic danger tokens for graph error surfaces", () => {
    expect(GRAPH_ERROR_CLASS).toContain("border-danger");
    expect(GRAPH_ERROR_CLASS).toContain("bg-danger");
    expect(GRAPH_ERROR_CLASS).toContain("text-danger");
    expect(GRAPH_ERROR_CLASS).not.toContain("red-");
  });
});
