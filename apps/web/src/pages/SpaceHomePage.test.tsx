import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  canEditSpaceDescription,
  getSpaceHomeContinueDocument,
  getSpaceHomeRecentDocuments,
  getSpaceDescriptionActionLabel,
  getSpaceHomeContextLabel,
  getSpaceHomeDocumentPath,
  getSpaceHomeQuickAccessDocuments,
  getSpaceHomeSectionPath,
  normalizeSpaceDescription,
  isEmptySpaceHome,
  settleCurrentSpaceHomeRequest,
  SPACE_DESCRIPTION_MAX_LENGTH,
  SPACE_HOME_TOUCH_ACTION_CLASS,
  shouldShowSpaceHomeHeaderCreateAction,
  shouldShowContinueWorking,
  shouldShowQuickAccess,
  shouldShowRecentPages,
  shouldShowStartWriting,
} from "./SpaceHomePage";
import type { DocumentRecord } from "@/lib/dataClient";
import type { StarredDocumentRecord } from "@/lib/starredDocuments";

describe("SpaceHomePage routes", () => {
  it("builds encoded document routes", () => {
    expect(getSpaceHomeDocumentPath("space-1", "doc-1")).toBe("/spaces/space-1/docs/doc-1");
    expect(getSpaceHomeDocumentPath("space/one", "doc/one")).toBe(
      "/spaces/space%2Fone/docs/doc%2Fone",
    );
  });

  it("builds encoded section routes", () => {
    expect(getSpaceHomeSectionPath("space-1", "recent")).toBe("/spaces/space-1/recent");
    expect(getSpaceHomeSectionPath("space/one", "journal")).toBe("/spaces/space%2Fone/journal");
    expect(getSpaceHomeSectionPath("space/one", "graph")).toBe("/spaces/space%2Fone/graph");
    expect(getSpaceHomeSectionPath("space/one", "workflows")).toBe("/spaces/space%2Fone/workflows");
  });

  it("keeps Space Home actions touch-safe on mobile without enlarging desktop chrome", () => {
    expect(SPACE_HOME_TOUCH_ACTION_CLASS).toContain("min-h-11");
    expect(SPACE_HOME_TOUCH_ACTION_CLASS).toContain("sm:min-h-0");
  });

  it("keeps the header focused on page creation instead of duplicating Main navigation", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/SpaceHomePage.tsx"), "utf8");

    expect(source).toContain("New page");
    expect(source).not.toContain('getSpaceHomeSectionPath(spaceId, "journal")');
  });

  it("allows managers to edit the bounded space description", () => {
    expect(canEditSpaceDescription("admin")).toBe(true);
    expect(canEditSpaceDescription("owner")).toBe(true);
    expect(canEditSpaceDescription("space_admin")).toBe(true);
    expect(canEditSpaceDescription("editor")).toBe(false);
    expect(canEditSpaceDescription("viewer")).toBe(false);
    expect(normalizeSpaceDescription("  Shared project context.  ")).toBe(
      "Shared project context.",
    );
    expect(SPACE_DESCRIPTION_MAX_LENGTH).toBe(1000);
    expect(getSpaceDescriptionActionLabel()).toBe("Add description");
    expect(getSpaceDescriptionActionLabel("   ")).toBe("Add description");
    expect(getSpaceDescriptionActionLabel("Shared project context.")).toBe("Edit");
  });

  it("uses the current space's remembered page as the continue-working target", () => {
    const documents: DocumentRecord[] = [{ id: "doc-1", title: "Project Atlas" }];

    expect(
      getSpaceHomeContinueDocument("space-1", documents, {
        spaceId: "space-1",
        docId: "doc-1",
        path: "/spaces/space-1/docs/doc-1",
      }),
    ).toEqual(documents[0]);
    expect(
      getSpaceHomeContinueDocument("space-2", documents, {
        spaceId: "space-1",
        docId: "doc-1",
        path: "/spaces/space-1/docs/doc-1",
      }),
    ).toBeUndefined();
  });

  it("uses only current-space starred pages that still exist for quick access", () => {
    const documents: DocumentRecord[] = [
      { id: "doc-1", title: "Project Atlas" },
      { id: "doc-2", title: "Release plan" },
    ];
    const starred: StarredDocumentRecord[] = [
      { id: "doc-1", title: "Old name", spaceId: "space-1", starred_at: "2026-07-23T00:00:00Z" },
      { id: "missing", title: "Missing", spaceId: "space-1", starred_at: "2026-07-23T00:00:00Z" },
      {
        id: "doc-2",
        title: "Release plan",
        spaceId: "space-2",
        starred_at: "2026-07-23T00:00:00Z",
      },
    ];

    expect(getSpaceHomeQuickAccessDocuments("space-1", documents, starred)).toEqual([documents[0]]);
  });

  it("removes the Continue working page from Favorites", () => {
    const active: DocumentRecord = { id: "active", title: "Current page" };
    const other: DocumentRecord = { id: "other", title: "Other page" };
    const starred: StarredDocumentRecord[] = [
      { id: "active", title: "Current page", spaceId: "space-1", starred_at: "2026-07-23" },
      { id: "other", title: "Other page", spaceId: "space-1", starred_at: "2026-07-23" },
    ];

    expect(getSpaceHomeQuickAccessDocuments("space-1", [active, other], starred, active.id)).toEqual([
      other,
    ]);
  });

  it("removes every Continue working and Quick access overlap from Recent pages", () => {
    const active: DocumentRecord = { id: "active", title: "Current page" };
    const favorite: DocumentRecord = { id: "favorite", title: "Pinned page" };

    expect(getSpaceHomeRecentDocuments([active, favorite], active, [active, favorite])).toEqual([]);
  });

  it("filters overlaps before applying the six-page Recent cap", () => {
    const active: DocumentRecord = { id: "active", title: "Current page" };
    const favorites: DocumentRecord[] = [
      { id: "favorite-1", title: "Pinned one" },
      { id: "favorite-2", title: "Pinned two" },
    ];
    const recent = Array.from({ length: 7 }, (_, index) => ({
      id: `recent-${index + 1}`,
      title: `Recent ${index + 1}`,
    }));

    expect(getSpaceHomeRecentDocuments([active, ...favorites, ...recent], active, favorites)).toEqual(
      recent.slice(0, 6),
    );
  });

  it("keeps the first six pages when Recent has no Quick access overlap", () => {
    const documents: DocumentRecord[] = Array.from({ length: 7 }, (_, index) => ({
      id: `doc-${index + 1}`,
      title: `Page ${index + 1}`,
    }));

    expect(getSpaceHomeRecentDocuments(documents, undefined, [])).toEqual(documents.slice(0, 6));
  });

  it("shows contextual sections only when they have content", () => {
    const document: DocumentRecord = { id: "doc-1", title: "Project Atlas" };

    expect(shouldShowContinueWorking(document)).toBe(true);
    expect(shouldShowContinueWorking(undefined)).toBe(false);
    expect(shouldShowQuickAccess([document])).toBe(true);
    expect(shouldShowQuickAccess([])).toBe(false);
    expect(shouldShowRecentPages([document])).toBe(true);
    expect(shouldShowRecentPages([])).toBe(false);
  });

  it("shows returned server visibility and role, with truthful unknown and local states", () => {
    expect(
      getSpaceHomeContextLabel(
        { id: "space-1", name: "Atlas", slug: "atlas", visibility: "internal", member_role: "editor" },
        false,
      ),
    ).toBe("Internal · Editor");
    expect(
      getSpaceHomeContextLabel({ id: "space-1", name: "Atlas", slug: "atlas" }, false),
    ).toBe("Visibility unavailable · Access role unavailable");
    expect(
      getSpaceHomeContextLabel({ id: "space-1", name: "Atlas", slug: "atlas" }, true),
    ).toBe("Local space · Owner");
  });

  it("keeps folder inventory out of Space Home while retaining its useful page sections", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/SpaceHomePage.tsx"), "utf8");

    expect(source).not.toContain("dataClient.listFolders");
    expect(source).not.toContain('aria-labelledby="folders-title"');
    expect(source).not.toContain("rootSpaceHomeFolders");
    expect(source).toContain('aria-labelledby="continue-working-title"');
    expect(source).toContain('aria-labelledby="quick-access-title"');
    expect(source).toContain('aria-labelledby="recent-pages-title"');
    expect(source).toContain("shouldShowRecentPages(recentDocuments)");
    expect(source).toContain('aria-labelledby="start-writing-title"');
  });

  it("uses one focused Start writing state when a space has no pages", () => {
    expect(isEmptySpaceHome([])).toBe(true);
    expect(isEmptySpaceHome([{ id: "doc-1", title: "Project Atlas" }])).toBe(false);
    expect(shouldShowSpaceHomeHeaderCreateAction([])).toBe(false);
    expect(
      shouldShowSpaceHomeHeaderCreateAction([{ id: "doc-1", title: "Project Atlas" }]),
    ).toBe(true);
    expect(shouldShowStartWriting(false, [])).toBe(false);
    expect(shouldShowStartWriting(true, [])).toBe(true);
    expect(shouldShowStartWriting(true, [{ id: "doc-1", title: "Project Atlas" }])).toBe(false);
  });

  it("ignores an older Home request after navigation changes spaces", async () => {
    let resolveOld!: (value: string) => void;
    const oldRequest = new Promise<string>((resolve) => {
      resolveOld = resolve;
    });
    let requestId = 1;
    let activeSpaceId = "space-a";
    let displayedSpace = "space-b";
    let loading = true;
    const requestSpaceId = activeSpaceId;
    const thisRequestId = requestId;

    const settling = settleCurrentSpaceHomeRequest(
      oldRequest,
      () => thisRequestId === requestId && requestSpaceId === activeSpaceId,
      (space) => {
        displayedSpace = space;
      },
      () => {
        displayedSpace = "error";
      },
      () => {
        loading = false;
      },
    );
    activeSpaceId = "space-b";
    requestId += 1;
    resolveOld("space-a");
    await settling;

    expect(displayedSpace).toBe("space-b");
    expect(loading).toBe(true);
  });
});
