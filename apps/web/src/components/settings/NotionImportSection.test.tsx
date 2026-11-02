import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  NotionImportSection,
  formatNotionImportResult,
  formatNotionImportReview,
} from "./NotionImportSection";
import type {
  NotionImportExecutionResult,
  NotionImportFinalizeResult,
  NotionImportPreflight,
  NotionImportTargetClient,
} from "@/lib/notionImport";

const preflight: NotionImportPreflight = {
  status: "ready",
  archiveName: "Workspace.zip",
  rootFolder: "Workspace",
  pageCount: 3,
  databaseCount: 1,
  assetCount: 2,
  ignoredCount: 1,
  unsupportedCount: 0,
  emptyTextCount: 0,
  totalExpandedBytes: 4096,
  blockers: [],
  warnings: [],
  entries: [],
};

const client: NotionImportTargetClient = {
  listFolders: async () => [],
  listDocuments: async () => [],
  createFolder: async (_spaceId, name) => ({ id: `folder-${name}`, name }),
  createDocument: async () => ({ id: "doc-1", draft_id: "draft-1" }),
  saveDocumentDataAsset: async () => ({ id: "asset-1", url: "starlane://asset/asset-1" }),
  updateDocumentDraft: async () => undefined,
  updateDocumentContent: async () => undefined,
};

describe("NotionImportSection", () => {
  it("renders the review-before-write workflow and draft boundary", () => {
    const html = renderToStaticMarkup(
      <NotionImportSection spaceId="space-1" spaceName="Research" client={client} />,
    );

    expect(html).toContain("Import from Notion");
    expect(html).toContain("Markdown &amp; CSV");
    expect(html).toContain("review the ZIP before Starlane creates anything");
    expect(html).toContain("Imported pages remain drafts until you publish them");
    expect(html).toContain('accept=".zip,application/zip"');
    expect(html).toContain("Choose ZIP");
  });

  it("formats review counts without presenting CSV databases as proprietary objects", () => {
    expect(formatNotionImportReview(preflight)).toBe(
      "3 pages, 1 CSV database, 0 folders, and 2 asset candidates.",
    );
    expect(
      formatNotionImportReview(preflight, {
        archiveName: "Workspace.zip",
        folders: [{ key: "team", name: "Team", depth: 1 }],
        documents: [
          {
            key: "page",
            sourcePath: "Workspace/Page.md",
            title: "Page",
            content: "Body",
            kind: "page",
            depth: 1,
          },
        ],
        skippedPaths: [],
        warnings: [],
      }),
    ).toBe("1 page, 0 CSV databases, 1 folder, and 2 asset candidates.");
  });

  it("formats completed and partial results from both import phases", () => {
    const execution: NotionImportExecutionResult = {
      status: "partial",
      createdFolderCount: 1,
      reusedFolderCount: 0,
      createdDocumentCount: 3,
      renamedDocumentCount: 1,
      documentIdsBySourcePath: {},
      documentDraftIdsBySourcePath: {},
      failures: [{ kind: "document", key: "missing", label: "Missing", reason: "Create failed" }],
    };
    const finalized: NotionImportFinalizeResult = {
      updatedDocumentCount: 2,
      rewrittenPageLinkCount: 4,
      uploadedAssetCount: 2,
      unresolvedPageLinkCount: 1,
      skippedAssetCount: 0,
      failures: [],
    };

    expect(formatNotionImportResult(execution, finalized)).toBe(
      "3 drafts created, 4 page links repaired, 2 assets imported, 1 item needs review.",
    );
  });
});
