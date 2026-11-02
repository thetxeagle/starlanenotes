import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import type { DocumentRecord } from "@/lib/dataClient";
import {
  filterRecentDocuments,
  getRecentDocumentPath,
  getRecentDocumentsListState,
  getRecentDocumentsSnapshotForSpace,
  isCurrentRecentDocumentsRequest,
  RecentDocumentsPage,
  sortRecentDocuments,
} from "./RecentDocumentsPage";

describe("RecentDocumentsPage helpers", () => {
  function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((resolvePromise, rejectPromise) => {
      resolve = resolvePromise;
      reject = rejectPromise;
    });
    return { promise, resolve, reject };
  }

  it("hides a previous space snapshot immediately after the route changes", () => {
    const snapshot = {
      spaceId: "space-a",
      spaceName: "Alpha",
      documents: [{ id: "doc-a", title: "Alpha document" }],
    };

    expect(getRecentDocumentsSnapshotForSpace(snapshot, "space-a")).toEqual(snapshot);
    expect(getRecentDocumentsSnapshotForSpace(snapshot, "space-b")).toBeNull();
  });

  it("ignores out-of-order rows and errors from a superseded space request", async () => {
    const olderRows = deferred<string[]>();
    const olderError = deferred<string[]>();
    let current = { spaceId: "space-a", requestId: 1 };
    let rows: string[] = [];
    let error = "";
    const commit = (requestSpaceId: string, requestId: number, request: Promise<string[]>) =>
      request.then(
        (result) => {
          if (
            isCurrentRecentDocumentsRequest(
              requestSpaceId,
              requestId,
              current.spaceId,
              current.requestId,
            )
          ) {
            rows = result;
          }
        },
        () => {
          if (
            isCurrentRecentDocumentsRequest(
              requestSpaceId,
              requestId,
              current.spaceId,
              current.requestId,
            )
          ) {
            error = `Unable to load ${requestSpaceId}`;
          }
        },
      );

    const rowRequest = commit("space-a", 1, olderRows.promise);
    const errorRequest = commit("space-a", 1, olderError.promise);
    current = { spaceId: "space-b", requestId: 2 };
    rows = ["space-b document"];

    olderRows.resolve(["stale space-a document"]);
    olderError.reject(new Error("stale failure"));
    await Promise.all([rowRequest, errorRequest]);

    expect(rows).toEqual(["space-b document"]);
    expect(error).toBe("");
  });

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

  it("filters recent pages by title", () => {
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

  it("shows loading, successful-empty, and failed-load states without conflating them", () => {
    expect(getRecentDocumentsListState(true, "", 0, 0)).toBe("loading");
    expect(getRecentDocumentsListState(false, "", 0, 0)).toBe("empty");
    expect(getRecentDocumentsListState(false, "Unable to load recent pages.", 0, 0)).toBe(
      "error",
    );
  });

  it("renders a status instead of an empty message before the initial request settles", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <RecentDocumentsPage />
      </MemoryRouter>,
    );

    expect(html).toContain('role="status"');
    expect(html).toContain("Loading recent pages…");
    expect(html).not.toContain(">No recent pages<");
  });
});
