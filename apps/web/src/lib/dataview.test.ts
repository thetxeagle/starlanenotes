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
  it("filters canonical responsibility separately from custom owner/team properties and workflow", () => {
    const pages: DataviewDocument[] = [
      {
        id: "engineering",
        title: "Policy",
        maintenance_group_id: "engineering-ID",
        properties: { Owner: "Someone", Status: "Awaiting review" },
      },
      {
        id: "person",
        title: "Runbook",
        maintenance_user_id: "person-id",
        properties: { Owner: "Engineering", Team: "engineering-id" },
      },
      {
        id: "unassigned",
        title: "Checklist",
        properties: { "page.maintenance_group": "engineering-id" },
      },
    ];
    expect(
      filterDataviewDocuments(pages, "page.maintenance_group = engineering-ID").map((p) => p.id),
    ).toEqual(["engineering"]);
    expect(
      filterDataviewDocuments(pages, "page.maintenance_user = person-id").map((p) => p.id),
    ).toEqual(["person"]);
    expect(filterDataviewDocuments(pages, 'page.maintenance_user is ""').map((p) => p.id)).toEqual([
      "engineering",
      "unassigned",
    ]);
    expect(filterDataviewDocuments(pages, "page.maintenance_group = engineering-id")).toEqual([]);
    expect(filterDataviewDocuments(pages, "Owner = Engineering").map((p) => p.id)).toEqual([
      "person",
    ]);
    expect(
      filterDataviewDocuments(pages, "property.page.maintenance_group = engineering-id").map(
        (p) => p.id,
      ),
    ).toEqual(["unassigned"]);
    expect(
      filterDataviewDocuments(
        pages,
        'page.maintenance_group = engineering-ID AND Status = "Awaiting review"',
      ).map((p) => p.id),
    ).toEqual(["engineering"]);
  });

  it("separates the page identifier from existing custom identifier properties", () => {
    const pages: DataviewDocument[] = [
      {
        id: "canonical",
        title: "One",
        human_identifier: "Ops-17",
        properties: {
          Identifier: "legacy",
          PageIdentifier: "custom",
        },
      },
      {
        id: "legacy",
        title: "Two",
        properties: { Identifier: "Ops-17", PageIdentifier: "Ops-17" },
      },
      {
        id: "dotted",
        title: "Three",
        human_identifier: "Diff-99",
        properties: { "page.identifier": "legacy-dotted" },
      },
    ];
    expect(
      filterDataviewDocuments(pages, "page.identifier = ops-17").map((page) => page.id),
    ).toEqual(["canonical"]);
    expect(filterDataviewDocuments(pages, "Identifier = ops-17").map((page) => page.id)).toEqual([
      "legacy",
    ]);
    expect(
      filterDataviewDocuments(pages, "property.Identifier = ops-17").map((page) => page.id),
    ).toEqual(["legacy"]);
    expect(
      filterDataviewDocuments(pages, "property.PageIdentifier = ops-17").map((page) => page.id),
    ).toEqual(["legacy"]);
    expect(
      filterDataviewDocuments(pages, "property.page.identifier = legacy-dotted").map(
        (page) => page.id,
      ),
    ).toEqual(["dotted"]);
    expect(filterDataviewDocuments(pages, 'page.identifier is ""').map((page) => page.id)).toEqual([
      "legacy",
    ]);
  });
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
      "Status = Draft AND reviewDate is 2026-07-06 AND ownerName is not Church",
    );
    expect(result.map((doc) => doc.id)).toEqual(["a"]);
  });

  it("supports OR and NOT groups", () => {
    const result = filterDataviewDocuments(
      docs,
      "(space:ops OR tag:planning) AND NOT Status:Draft",
    );
    expect(result.map((doc) => doc.id)).toEqual(["b"]);
  });

  it("applies result limits to blank and filtered queries", () => {
    expect(filterDataviewDocuments(docs, "", 1).map((doc) => doc.id)).toEqual(["a"]);
    expect(filterDataviewDocuments(docs, "title:*", 1).map((doc) => doc.id)).toEqual(["a"]);
  });
});
