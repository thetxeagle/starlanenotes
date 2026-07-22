import { describe, expect, it } from "vitest";
import type { DocumentRecord } from "@/lib/dataClient";
import { filterRecentDocuments, getRecentDocumentPath, sortRecentDocuments } from "./RecentDocumentsPage";

describe("RecentDocumentsPage helpers", () => {
  it("builds encoded document routes", () => {
    expect(getRecentDocumentPath("space-1", "doc-1")).toBe("/spaces/space-1/docs/doc-1");
    expect(getRecentDocumentPath("space/one", "doc/one")).toBe(
      "/spaces/space%2Fone/docs/doc%2Fone",
    );
  });

  it("sorts documents by most recently updated with title fallback", () => {
    const documents: DocumentRecord[] = [
      { id: "old", title: "Old", updated_at: "2026-07-07T12:00:00.000Z" },
      { id: "beta", title: "Beta", updated_at: "2026-07-09T12:00:00.000Z" },
      { id: "alpha", title: "Alpha", updated_at: "2026-07-09T12:00:00.000Z" },
    ];

    expect(sortRecentDocuments(documents).map((document) => document.id)).toEqual([
      "alpha",
      "beta",
      "old",
    ]);
    expect(documents.map((document) => document.id)).toEqual(["old", "beta", "alpha"]);
  });

  it("filters recent documents by title", () => {
    const documents: DocumentRecord[] = [
      { id: "launch", title: "Launch Plan" },
      { id: "runbook", title: "Ops Runbook" },
    ];

    expect(filterRecentDocuments(documents, "launch").map((document) => document.id)).toEqual([
      "launch",
    ]);
    expect(filterRecentDocuments(documents, " ").map((document) => document.id)).toEqual([
      "launch",
      "runbook",
    ]);
  });
});
