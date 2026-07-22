import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import {
  getStarredDocumentPath,
  groupStarredDocumentsBySpace,
  StarredDocumentsPage,
} from "./StarredDocumentsPage";
import type { StarredDocumentRecord } from "@/lib/starredDocuments";

describe("StarredDocumentsPage helpers", () => {
  it("builds encoded document routes", () => {
    expect(getStarredDocumentPath("space-1", "doc-1")).toBe("/spaces/space-1/docs/doc-1");
    expect(getStarredDocumentPath("space/one", "doc/one")).toBe(
      "/spaces/space%2Fone/docs/doc%2Fone",
    );
  });

  it("groups starred documents by space label", () => {
    const docs: StarredDocumentRecord[] = [
      {
        id: "doc-1",
        spaceId: "space-b",
        spaceName: "Ops",
        title: "Runbook",
        starred_at: "2026-07-06T12:00:00.000Z",
      },
      {
        id: "doc-2",
        spaceId: "space-a",
        spaceName: "Daily",
        title: "Today",
        starred_at: "2026-07-07T12:00:00.000Z",
      },
      {
        id: "doc-3",
        spaceId: "space-b",
        spaceName: "Ops",
        title: "Checklist",
        starred_at: "2026-07-08T12:00:00.000Z",
      },
    ];

    const groups = groupStarredDocumentsBySpace(docs);

    expect(groups.map((group) => group.label)).toEqual(["Daily", "Ops"]);
    expect(groups.find((group) => group.key === "space-b")?.documents.map((doc) => doc.id)).toEqual([
      "doc-1",
      "doc-3",
    ]);
  });

  it("renders starred sort options", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <StarredDocumentsPage />
      </MemoryRouter>,
    );

    expect(html).toContain("Recently starred");
    expect(html).toContain("Recently updated");
    expect(html).toContain("Title A-Z");
  });
});
