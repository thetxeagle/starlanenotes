import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import {
  claimStarredBulkMutation,
  claimStarredRowMutation,
  claimStarredRefresh,
  getStarredDocumentPath,
  getStarredDocumentFeedbackClass,
  groupStarredDocumentsBySpace,
  releaseStarredBulkMutation,
  releaseStarredRowMutation,
  releaseStarredRefresh,
  getStarredDocumentsListState,
  isStarredOperationCurrent,
  STARRED_DOCUMENT_GROUP_HEADING_CLASS,
  StarredDocumentsPage,
} from "./StarredDocumentsPage";
import type { StarredDocumentRecord } from "@/lib/starredDocuments";

describe("StarredDocumentsPage helpers", () => {
  it("invalidates superseded refresh completion", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/pages/StarredDocumentsPage.tsx"),
      "utf8",
    );

    expect(source).toContain("const requestId = ++refreshRequestIdRef.current");
    expect(source).toContain("if (requestId !== refreshRequestIdRef.current) return");
  });

  it("reports refresh failures while retaining cached stars", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/pages/StarredDocumentsPage.tsx"),
      "utf8",
    );

    expect(source).toContain('if (result.syncStatus === "failed")');
    expect(source).toContain("Unable to refresh Favorites. Showing the saved copy.");
  });

  it("bounds each row-level unstar mutation", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/pages/StarredDocumentsPage.tsx"),
      "utf8",
    );

    expect(source).toContain(
      "if (!claimStarredRowMutation(mutationLockRef.current, operationScope, key)) return",
    );
    expect(source).toContain('aria-busy={pendingUnstarKeys.has(`${document.spaceId}:${document.id}`)}');
  });

  it("bounds the bulk unstar mutation", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/pages/StarredDocumentsPage.tsx"),
      "utf8",
    );

    expect(source).toContain("!claimStarredBulkMutation(mutationLockRef.current, operationScope)");
    expect(source).toContain("aria-busy={unstarVisiblePending}");
  });

  it("does not start a bulk unstar while a row mutation is pending", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/pages/StarredDocumentsPage.tsx"),
      "utf8",
    );

    expect(source).toContain("pendingUnstarKeys.size > 0");
  });

  it("claims row and bulk mutations synchronously in both directions", () => {
    const lock = { scope: "space-a", rowKeys: new Set<string>(), bulk: false, refresh: false };

    expect(claimStarredRowMutation(lock, "space-a", "space-a:doc-1")).toBe(true);
    expect(claimStarredRowMutation(lock, "space-a", "space-a:doc-1")).toBe(false);
    expect(claimStarredBulkMutation(lock, "space-a")).toBe(false);
    releaseStarredRowMutation(lock, "space-a", "space-a:doc-1");
    expect(claimStarredBulkMutation(lock, "space-a")).toBe(true);
    expect(claimStarredBulkMutation(lock, "space-a")).toBe(false);
    expect(claimStarredRowMutation(lock, "space-a", "space-a:doc-2")).toBe(false);
    releaseStarredBulkMutation(lock, "space-a");
    expect(claimStarredRowMutation(lock, "space-a", "space-a:doc-2")).toBe(true);
  });

  it("keeps refresh and mutations mutually exclusive synchronously", () => {
    const lock = { scope: "space-a", rowKeys: new Set<string>(), bulk: false, refresh: false };

    expect(claimStarredRefresh(lock, "space-a")).toBe(true);
    expect(claimStarredRefresh(lock, "space-a")).toBe(false);
    expect(claimStarredRowMutation(lock, "space-a", "space-a:doc-1")).toBe(false);
    expect(claimStarredBulkMutation(lock, "space-a")).toBe(false);
    releaseStarredRefresh(lock, "space-a");
    expect(claimStarredRowMutation(lock, "space-a", "space-a:doc-1")).toBe(true);
    expect(claimStarredRefresh(lock, "space-a")).toBe(false);
  });

  it("gives a new Starred page scope an independent lock", () => {
    const lock = { scope: "space-a", rowKeys: new Set<string>(), bulk: false, refresh: true };

    expect(claimStarredRefresh(lock, "space-b")).toBe(true);
    releaseStarredRefresh(lock, "space-a");
    expect(lock.refresh).toBe(true);
  });

  it("publishes Starred mutation completion only to its originating page scope", () => {
    expect(isStarredOperationCurrent("space-a", "space-a")).toBe(true);
    expect(isStarredOperationCurrent("space-a", "space-b")).toBe(false);

    const source = readFileSync(
      resolve(process.cwd(), "src/pages/StarredDocumentsPage.tsx"),
      "utf8",
    );
    expect(source).toContain("if (!isCurrent()) break");
    expect(source).toContain("pendingUnstarState.scope === pageScope");
    expect(source).toContain("unstarVisibleScope === pageScope");
  });

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

  it("renders the anchored starred sort control", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <StarredDocumentsPage />
      </MemoryRouter>,
    );

    expect(html).toContain("Recently starred");
    expect(html).toContain('role="status"');
    expect(html).toContain('aria-busy="true"');
    expect(html).toContain("disabled");
    expect(html).toContain("Loading Favorites…");
    expect(html).not.toContain(">No Favorites<");
    expect(html).toContain('aria-label="Sort Favorites"');
    expect(html).toContain('aria-haspopup="listbox"');
    expect(html).not.toContain("<select");
  });

  it("uses readable space group headings instead of decorative eyebrow text", () => {
    expect(STARRED_DOCUMENT_GROUP_HEADING_CLASS).toContain("text-sm");
    expect(STARRED_DOCUMENT_GROUP_HEADING_CLASS).not.toContain("uppercase");
    expect(STARRED_DOCUMENT_GROUP_HEADING_CLASS).not.toContain("tracking-wide");
  });

  it("keeps success, partial warning, and sync failure feedback semantically distinct", () => {
    expect(getStarredDocumentFeedbackClass("success")).toContain("text-success");
    expect(getStarredDocumentFeedbackClass("warning")).toContain("border-accent/40");
    expect(getStarredDocumentFeedbackClass("error")).toContain("text-danger");
  });

  it("shows a loading status before reporting a successfully empty starred list", () => {
    expect(getStarredDocumentsListState(true, 0, 0)).toBe("loading");
    expect(getStarredDocumentsListState(false, 0, 0)).toBe("empty");
    expect(getStarredDocumentsListState(true, 1, 0)).toBe("empty");
    expect(getStarredDocumentsListState(true, 1, 1)).toBe("documents");
  });
});
