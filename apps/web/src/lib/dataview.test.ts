import { describe, expect, it } from "vitest";
import { filterDataviewDocuments, parseDataviewQuery, type DataviewDocument } from "./dataview";

const docs: DataviewDocument[] = [
  {
    id: "a",
    title: "Project Alpha Plan",
    content: "Launch checklist and owner notes",
    space_id: "field-notes",
    space_name: "Field Notes",
    tags: ["project-alpha", "planning"],
    properties: {
      Status: "Draft",
      ownerName: "Eagle",
      reviewDate: "2026-7-6",
    },
    created_at: "2026-07-01T00:00:00.000Z",
    updated_at: "2026-07-06T00:00:00.000Z",
  },
  {
    id: "b",
    title: "Published Runbook",
    content: "Operations notes",
    space_id: "ops",
    space_name: "Operations",
    tags: ["runbook"],
    properties: {
      Status: "Published",
      ownerName: "Church",
      reviewDate: "2026-07-09",
    },
    created_at: "2026-07-02T00:00:00.000Z",
    updated_at: "2026-07-05T00:00:00.000Z",
  },
];

describe("dataview query engine", () => {
  it("parses field predicates and boolean operators", () => {
    expect(parseDataviewQuery('title:"Project Alpha" AND tag:#planning')).toMatchObject({
      expression: { type: "and" },
    });
  });

  it("matches quoted title text, tag slugs, and camelCase properties", () => {
    const result = filterDataviewDocuments(
      docs,
      'title:"Project Alpha" AND tag:#planning AND ownerName:Eagle',
    );
    expect(result.map((doc) => doc.id)).toEqual(["a"]);
  });

  it("supports wildcard and fuzzy searches", () => {
    expect(filterDataviewDocuments(docs, "title:Project*").map((doc) => doc.id)).toEqual(["a"]);
    expect(filterDataviewDocuments(docs, "title~PjAha").map((doc) => doc.id)).toEqual(["a"]);
  });

  it("supports equality, is not, and normalized dates", () => {
    const result = filterDataviewDocuments(
      docs,
      'Status = Draft AND reviewDate is 2026-07-06 AND ownerName is not Church',
    );
    expect(result.map((doc) => doc.id)).toEqual(["a"]);
  });

  it("supports OR and NOT groups", () => {
    const result = filterDataviewDocuments(docs, "(space:ops OR tag:planning) AND NOT Status:Draft");
    expect(result.map((doc) => doc.id)).toEqual(["b"]);
  });

  it("applies result limits to blank and filtered queries", () => {
    expect(filterDataviewDocuments(docs, "", 1).map((doc) => doc.id)).toEqual(["a"]);
    expect(filterDataviewDocuments(docs, "title:*", 1).map((doc) => doc.id)).toEqual(["a"]);
  });
});
