import { describe, expect, it, vi } from "vitest";
import { strToU8, zipSync } from "fflate";
import {
  buildNotionImportPlan,
  validateNotionImportPagePlan,
  csvToMarkdown,
  executeNotionImportPlan,
  finalizeNotionImportContent,
  preflightNotionExportArchive,
  type NotionImportPlan,
  type NotionImportTargetClient,
} from "./notionImport";

function makeArchive(files: Record<string, string | Uint8Array>): Uint8Array {
  return zipSync(
    Object.fromEntries(
      Object.entries(files).map(([path, value]) => [
        path,
        typeof value === "string" ? strToU8(value) : value,
      ]),
    ),
  );
}

describe("Notion export preflight", () => {
  it("summarizes Markdown pages, CSV databases, assets, and the common export root", async () => {
    const result = await preflightNotionExportArchive(
      "Workspace.zip",
      makeArchive({
        "Workspace/Project Alpha 0123456789abcdef0123456789abcdef.md": "# Project Alpha\n\nHello",
        "Workspace/Projects 11111111111111111111111111111111.csv": "Name,Status\nAlpha,Active",
        "Workspace/Project Alpha/image.png": new Uint8Array([1, 2, 3]),
        "Workspace/index.html": "<html></html>",
        "Workspace/.DS_Store": "noise",
      }),
    );

    expect(result).toMatchObject({
      status: "ready",
      rootFolder: "Workspace",
      pageCount: 1,
      databaseCount: 1,
      assetCount: 1,
      ignoredCount: 2,
      unsupportedCount: 0,
      emptyTextCount: 0,
      blockers: [],
    });
    expect(result.warnings).toContain("2 hidden or Notion index files will be ignored.");
  });

  it("warns about unsupported and empty content without treating it as importable", async () => {
    const result = await preflightNotionExportArchive(
      "Workspace.zip",
      makeArchive({
        "Page.md": "  \n",
        "Database.csv": "Name\nEntry",
        "Document.html": "<p>Unsupported page export</p>",
      }),
    );

    expect(result.status).toBe("ready");
    expect(result.emptyTextCount).toBe(1);
    expect(result.unsupportedCount).toBe(1);
    expect(result.warnings).toEqual([
      "1 unsupported content file will be skipped.",
      "1 empty page or database file will be skipped.",
    ]);
  });

  it("blocks traversal paths before extracting archive content", async () => {
    const archive = makeArchive({ "Page.md": "# Safe" });
    const path = strToU8("Page.md");
    const replacement = strToU8("../x.md");
    for (let index = 0; index <= archive.length - path.length; index += 1) {
      if (path.every((byte, offset) => archive[index + offset] === byte)) {
        archive.set(replacement, index);
      }
    }

    const result = await preflightNotionExportArchive("Workspace.zip", archive);

    expect(result.status).toBe("blocked");
    expect(result.blockers).toContain("../x.md: unsafe archive path.");
  });

  it("rejects non-ZIP selections and archives without portable content", async () => {
    await expect(
      preflightNotionExportArchive("Workspace.json", new Uint8Array([1])),
    ).rejects.toThrow("Choose the .zip file produced by Notion's Markdown & CSV export.");

    const result = await preflightNotionExportArchive(
      "Workspace.zip",
      makeArchive({ "index.html": "<html></html>", "file.pdf": new Uint8Array([1]) }),
    );
    expect(result.status).toBe("blocked");
    expect(result.blockers).toContain(
      "No Markdown pages or CSV databases were found in this export.",
    );
  });

  it("blocks duplicate normalized entry paths", async () => {
    const archive = makeArchive({ "Folder/Page.md": "# First", "Folder\\Page.md": "# Second" });

    const result = await preflightNotionExportArchive("Workspace.zip", archive);
    expect(result.status).toBe("blocked");
    expect(result.blockers).toContain("Folder/Page.md: duplicate archive path.");
  });
});

describe("Notion import planning", () => {
  it("maps page-backed directories to child pages and plain directories to folders", async () => {
    const parentId = "11111111111111111111111111111111";
    const childId = "22222222222222222222222222222222";
    const grandchildId = "33333333333333333333333333333333";
    const greatGrandchildId = "44444444444444444444444444444444";
    const databaseId = "55555555555555555555555555555555";
    const archive = makeArchive({
      [`Workspace/Team/Parent ${parentId}.md`]: "# Parent body",
      [`Workspace/Team/Parent ${parentId}/Child ${childId}.md`]: "Child body",
      [`Workspace/Team/Parent ${parentId}/Child ${childId}/Grandchild ${grandchildId}.md`]:
        "Grandchild body",
      [`Workspace/Team/Parent ${parentId}/Child ${childId}/Grandchild ${grandchildId}/Great ${greatGrandchildId}.md`]:
        "Great body",
      [`Workspace/Team/Projects ${databaseId}.csv`]: 'Name,Notes\r\nAlpha,"Line one\nLine two"',
    });

    const plan = await buildNotionImportPlan("Workspace.zip", archive);

    expect(plan.folders).toEqual([
      expect.objectContaining({ key: "team", name: "Team", depth: 1 }),
    ]);
    expect(
      plan.documents.map(({ title, parentKey, folderKey, kind, depth }) => ({
        title,
        parentKey,
        folderKey,
        kind,
        depth,
      })),
    ).toEqual([
      { title: "Parent", parentKey: undefined, folderKey: "team", kind: "page", depth: 1 },
      {
        title: "Projects",
        parentKey: undefined,
        folderKey: "team",
        kind: "database",
        depth: 1,
      },
      {
        title: "Child",
        parentKey: `Team/Parent ${parentId}`,
        folderKey: "team",
        kind: "page",
        depth: 2,
      },
      {
        title: "Grandchild",
        parentKey: `Team/Parent ${parentId}/Child ${childId}`,
        folderKey: "team",
        kind: "page",
        depth: 3,
      },
      {
        title: "Great",
        parentKey: `Team/Parent ${parentId}/Child ${childId}/Grandchild ${grandchildId}`,
        folderKey: "team",
        kind: "page",
        depth: 4,
      },
    ]);
    expect(plan.documents.find((document) => document.title === "Projects")?.content).toContain(
      "| Alpha | Line one<br>Line two |",
    );
    expect(plan.warnings.some((warning) => warning.includes("flattened"))).toBe(false);
  });

  it("preserves folder nesting beyond three levels without collapsing names", async () => {
    const plan = await buildNotionImportPlan(
      "Workspace.zip",
      makeArchive({
        "Workspace/One/Two/Three/Four/Page 11111111111111111111111111111111.md": "Body",
      }),
    );

    expect(plan.folders.map((folder) => folder.name)).toEqual(["One", "Two", "Three", "Four"]);
    expect(plan.documents[0].folderKey).toBe("one/two/three/four");
    expect(plan.warnings.some((warning) => warning.includes("folder nesting"))).toBe(false);
  });

  it("converts quoted CSV fields to a portable Markdown table", () => {
    expect(csvToMarkdown('\uFEFFName,Details\n"A | B","Said ""hello"""')).toBe(
      '| Name | Details |\n| --- | --- |\n| A \\| B | Said "hello" |',
    );
  });
});

describe("Notion import execution", () => {
  it("reuses folders, renames document collisions, and reports independent failures", async () => {
    const plan: NotionImportPlan = {
      archiveName: "Workspace.zip",
      warnings: [],
      skippedPaths: [],
      folders: [
        { key: "team", name: "Team", depth: 1 },
        { key: "blocked", name: "Blocked", depth: 1 },
      ],
      documents: [
        {
          key: "parent",
          sourcePath: "Parent.md",
          title: "Parent",
          content: "Parent body",
          kind: "page",
          folderKey: "team",
          depth: 1,
        },
        {
          key: "independent",
          sourcePath: "Independent.md",
          title: "Independent",
          content: "Independent body",
          kind: "page",
          depth: 1,
        },
        {
          key: "blocked/page",
          sourcePath: "Blocked/Page.md",
          title: "Blocked page",
          content: "Blocked body",
          kind: "page",
          folderKey: "blocked",
          depth: 1,
        },
        {
          key: "child",
          sourcePath: "Parent/Child.md",
          title: "Child",
          content: "Child body",
          kind: "page",
          folderKey: "team",
          parentKey: "parent",
          depth: 2,
        },
      ],
    };
    const created: Array<{ title: string; folder_id?: string | null; parent_id?: string | null }> =
      [];
    const client: NotionImportTargetClient = {
      listFolders: async () => [{ id: "folder-team", name: "Team" }],
      listDocuments: async () => [
        { id: "existing-parent", title: "Parent", folder_id: "folder-team" },
      ],
      createFolder: async (_spaceId, name) => {
        if (name === "Blocked") throw new Error("Folder permission denied");
        return { id: `folder-${name.toLowerCase()}`, name };
      },
      createDocument: async (_spaceId, input) => {
        created.push(input);
        return {
          id: `doc-${created.length}`,
          draft_id: `draft-${created.length}`,
          draft_source_digest: "a".repeat(64),
        };
      },
      saveDocumentDataAsset: async () => ({ id: "asset-1", url: "data:image/png;base64," }),
      updateDocumentDraft: async () => undefined,
      updateDocumentContent: async () => undefined,
    };
    const progress: string[] = [];

    const result = await executeNotionImportPlan(client, "space-1", plan, ({ label }) => {
      progress.push(label);
    });

    expect(result).toMatchObject({
      status: "partial",
      createdFolderCount: 0,
      reusedFolderCount: 1,
      createdDocumentCount: 3,
      renamedDocumentCount: 1,
      documentDraftIdsBySourcePath: {
        "Parent.md": "draft-1",
        "Independent.md": "draft-2",
        "Parent/Child.md": "draft-3",
      },
    });
    expect(created).toEqual([
      {
        title: "Parent (Notion import)",
        content: "Parent body",
        folder_id: "folder-team",
        parent_id: null,
      },
      {
        title: "Independent",
        content: "Independent body",
        folder_id: null,
        parent_id: null,
      },
      {
        title: "Child",
        content: "Child body",
        folder_id: "folder-team",
        parent_id: "doc-1",
      },
    ]);
    expect(result.failures).toEqual([
      {
        kind: "folder",
        key: "blocked",
        label: "Blocked",
        reason: "Folder permission denied",
      },
      {
        kind: "document",
        key: "blocked/page",
        label: "Blocked page",
        reason: "Destination folder was not created.",
      },
    ]);
    expect(progress).toEqual(["Team", "Blocked", "Parent", "Independent", "Blocked page", "Child"]);
  });
});

describe("Notion import content finalization", () => {
  it("rewrites imported page links, owns referenced assets, and preserves unresolved/code links", async () => {
    const parentHash = "11111111111111111111111111111111";
    const childHash = "22222222222222222222222222222222";
    const parentPath = `Workspace/Parent ${parentHash}.md`;
    const childPath = `Workspace/Parent ${parentHash}/Child ${childHash}.md`;
    const imagePath = `Workspace/Parent ${parentHash}/diagram.png`;
    const manualPath = `Workspace/Parent ${parentHash}/manual.pdf`;
    const parentContent = [
      `[Child](Parent%20${parentHash}/Child%20${childHash}.md)`,
      `![Diagram](Parent%20${parentHash}/diagram.png "Preview")`,
      `![Diagram again](Parent%20${parentHash}/diagram.png)`,
      `[Manual](Parent%20${parentHash}/manual.pdf)`,
      `[Missing](Missing%2099999999999999999999999999999999.md)`,
      "`[Code link](Parent%20ignored.md)`",
      "```md",
      `[Fenced link](Parent%20${parentHash}/Child%20${childHash}.md)`,
      "```",
      "[External](https://example.com)",
    ].join("\n\n");
    const archive = makeArchive({
      [parentPath]: parentContent,
      [childPath]: "Child body",
      [imagePath]: new Uint8Array([1, 2, 3]),
      [manualPath]: new Uint8Array([4, 5, 6]),
    });
    const plan = await buildNotionImportPlan("Workspace.zip", archive);
    const execution = {
      status: "completed" as const,
      createdFolderCount: 0,
      reusedFolderCount: 0,
      createdDocumentCount: 2,
      renamedDocumentCount: 0,
      documentIdsBySourcePath: {
        [parentPath]: "doc-parent",
        [childPath]: "doc-child",
      },
      documentDraftIdsBySourcePath: {
        [parentPath]: "draft-parent",
        [childPath]: "draft-child",
      },
      documentDraftSourcesBySourcePath: {
        [parentPath]: "a".repeat(64),
        [childPath]: "b".repeat(64),
      },
      failures: [],
    };
    const updates = new Map<string, string>();
    const savedAssets: string[] = [];
    const client: NotionImportTargetClient = {
      listFolders: async () => [],
      listDocuments: async () => [],
      createFolder: async (_spaceId, name) => ({ id: `folder-${name}`, name }),
      createDocument: async () => ({ id: "unused" }),
      saveDocumentDataAsset: async (input) => {
        savedAssets.push(input.name);
        return input.name.endsWith(".pdf")
          ? { id: "attachment-manual", url: "/files/manual.pdf", filename: "manual.pdf" }
          : { id: "asset-diagram", url: input.data_url };
      },
      updateDocumentDraft: async (_spaceId, documentId, _draftId, { content }, source) => {
        expect(source).toBe("a".repeat(64));
        updates.set(documentId, content);
      },
      updateDocumentContent: async () => {
        throw new Error("draft mutation should own imported content");
      },
    };

    const result = await finalizeNotionImportContent(client, "space-1", plan, archive, execution);

    expect(result).toEqual({
      updatedDocumentCount: 1,
      rewrittenPageLinkCount: 1,
      uploadedAssetCount: 2,
      unresolvedPageLinkCount: 1,
      skippedAssetCount: 0,
      failures: [],
    });
    expect(savedAssets).toEqual(["diagram.png", "manual.pdf"]);
    expect(updates.get("doc-parent")).toContain("[[Child|doc-child]]");
    expect(updates.get("doc-parent")).toContain(
      '![Diagram](starlane://asset/asset-diagram "Preview")',
    );
    expect(updates.get("doc-parent")).toContain("![Diagram again](starlane://asset/asset-diagram)");
    expect(updates.get("doc-parent")).toContain("[Manual](/files/manual.pdf)");
    expect(updates.get("doc-parent")).toContain(
      "[Missing](Missing%2099999999999999999999999999999999.md)",
    );
    expect(updates.get("doc-parent")).toContain("`[Code link](Parent%20ignored.md)`");
    expect(updates.get("doc-parent")).toContain(
      `[Fenced link](Parent%20${parentHash}/Child%20${childHash}.md)`,
    );
    expect(updates.get("doc-parent")).toContain("[External](https://example.com)");
  });

  it("reports failed asset ownership and content updates without corrupting source links", async () => {
    const pagePath = "Workspace/Page 11111111111111111111111111111111.md";
    const archive = makeArchive({
      [pagePath]: "![Diagram](diagram.png)",
      "Workspace/diagram.png": new Uint8Array([1, 2, 3]),
    });
    const plan = await buildNotionImportPlan("Workspace.zip", archive);
    const client: NotionImportTargetClient = {
      listFolders: async () => [],
      listDocuments: async () => [],
      createFolder: async (_spaceId, name) => ({ id: `folder-${name}`, name }),
      createDocument: async () => ({ id: "unused" }),
      saveDocumentDataAsset: async () => {
        throw new Error("Upload denied");
      },
      updateDocumentDraft: async () => {
        throw new Error("should not update unchanged draft");
      },
      updateDocumentContent: async () => {
        throw new Error("should not update unchanged source");
      },
    };

    const result = await finalizeNotionImportContent(client, "space-1", plan, archive, {
      status: "completed",
      createdFolderCount: 0,
      reusedFolderCount: 0,
      createdDocumentCount: 1,
      renamedDocumentCount: 0,
      documentIdsBySourcePath: { [pagePath]: "doc-page" },
      documentDraftIdsBySourcePath: { [pagePath]: "draft-page" },
      failures: [],
    });

    expect(result.updatedDocumentCount).toBe(0);
    expect(result.failures).toEqual([
      {
        kind: "document",
        key: "Page 11111111111111111111111111111111",
        label: "Page",
        reason: "Workspace/diagram.png: Upload denied",
      },
    ]);
  });
  it("retains original creation digest for asset transforms and holds missing legacy source without a save", async () => {
    const pagePath = "Workspace/Page 11111111111111111111111111111111.md";
    const archive = makeArchive({
      [pagePath]: "![Diagram](diagram.png)",
      "Workspace/diagram.png": new Uint8Array([1, 2, 3]),
    });
    const plan = await buildNotionImportPlan("Workspace.zip", archive);
    const sources: unknown[] = [];
    const client: NotionImportTargetClient = {
      requiresDraftSourceCas: true,
      listFolders: async () => [],
      listDocuments: async () => [],
      createFolder: async () => ({ id: "folder", name: "Workspace" }),
      createDocument: async () => ({
        id: "page",
        draft_id: "draft",
        draft_source_digest: "a".repeat(64),
      }),
      saveDocumentDataAsset: async () => ({ id: "asset", url: "/files/diagram.png" }),
      updateDocumentDraft: async (_s, _p, _d, _patch, source) => {
        sources.push(source);
      },
      updateDocumentContent: async () => {
        throw new Error("No canonical fallback");
      },
    };
    const execution = await executeNotionImportPlan(client, "s", plan);
    expect(execution.documentDraftSourcesBySourcePath?.[pagePath]).toBe("a".repeat(64));
    const positive = await finalizeNotionImportContent(client, "s", plan, archive, execution);
    expect(positive.updatedDocumentCount).toBe(1);
    expect(sources).toEqual(["a".repeat(64)]);
    const legacy = { ...execution, documentDraftSourcesBySourcePath: {} };
    const held = await finalizeNotionImportContent(client, "s", plan, archive, legacy);
    expect(held.updatedDocumentCount).toBe(0);
    expect(held.failures.some((item) => item.reason.includes("Imported draft needs review"))).toBe(
      true,
    );
    expect(sources).toEqual(["a".repeat(64)]);
  });

  it("binds import creation and later transforms to original actual client context before awaited responses", async () => {
    const storage = new Map<string, string>();
    const local = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
      removeItem: (key: string) => {
        storage.delete(key);
      },
    };
    vi.stubGlobal("localStorage", local);
    vi.stubGlobal("window", { addEventListener: vi.fn(), localStorage: local });
    try {
      const { captureDraftSaveSelection, verifyDraftSaveSelection } = await import("./dataClient");
      const { api } = await import("./api");
      const { setCurrentLaunchMode } = await import("./launchMode");
      const { setSyncProvider } = await import("./storageProviders");
      const token = (id: string) =>
        `e30.${btoa(JSON.stringify({ sub: id, sid: `session-${id}`, token_type: "access" }))}.fixture`;
      setCurrentLaunchMode("server");
      setSyncProvider("none");
      api.setToken(token("a"));
      const path = "Workspace/Page 11111111111111111111111111111111.md";
      const archive = makeArchive({
        [path]: "![Asset](diagram.png)",
        "Workspace/diagram.png": new Uint8Array([1]),
      });
      const plan = await buildNotionImportPlan("Workspace.zip", archive);
      const upload = vi.fn(async () => ({ id: "asset", url: "/files/asset" }));
      let changeDuringCreate = false;
      const client: NotionImportTargetClient = {
        requiresDraftSourceCas: true,
        captureDraftSaveSelection,
        verifyDraftSaveSelection,
        listFolders: async () => [],
        listDocuments: async () => [],
        createFolder: async () => ({ id: "folder", name: "Workspace" }),
        createDocument: async () => {
          if (changeDuringCreate) api.setToken(token("b"));
          return { id: "p", draft_id: "d", draft_source_digest: "a".repeat(64) };
        },
        saveDocumentDataAsset: upload,
        updateDocumentDraft: async () => {
          throw new Error("No foreign save");
        },
        updateDocumentContent: async () => {
          throw new Error("No fallback");
        },
      };
      const original = await executeNotionImportPlan(client, "s", plan);
      expect(original.createdDocumentCount).toBe(1);
      expect(original.draftSaveSelection?.accountId).toBe("a");
      api.setToken(token("b"));
      await expect(
        finalizeNotionImportContent(client, "s", plan, archive, original),
      ).rejects.toThrow("Draft session or storage changed");
      expect(upload).not.toHaveBeenCalled();
      api.setToken(token("a"));
      changeDuringCreate = true;
      const changed = await executeNotionImportPlan(client, "s", plan);
      expect(changed.createdDocumentCount).toBe(0);
      expect(changed.documentDraftSourcesBySourcePath).toEqual({});
      expect(
        changed.failures.some((failure) =>
          failure.reason.includes("Draft session or storage changed"),
        ),
      ).toBe(true);
      api.setToken(null);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

describe("deep Notion folder hierarchy", () => {
  it.each([4, 32, 256])(
    "plans and executes %i exact folder parents without flattening",
    async (depth) => {
      const segments = Array.from({ length: depth }, () => "x"),
        path = `Workspace/${segments.join("/")}/Page 11111111111111111111111111111111.md`;
      const plan = await buildNotionImportPlan(
        "Workspace.zip",
        makeArchive({ [path]: "Exact body" }),
      );
      expect(plan.folders).toHaveLength(depth);
      expect(plan.folders.map((row) => row.name)).toEqual(segments);
      for (let i = 1; i < depth; i++)
        expect(plan.folders[i].parentKey).toBe(plan.folders[i - 1].key);
      const created: Array<{ id: string; name: string; parent_id?: string }> = [];
      const documents: Array<{
        id: string;
        title: string;
        folder_id?: string;
        parent_id?: string;
      }> = [];
      const client = {
        listFolders: async () => created.map((row) => ({ ...row })),
        listDocuments: async () => documents.map((row) => ({ ...row })),
        createFolder: async (
          _space: string,
          name: string,
          options?: { parent_id?: string | null },
        ) => {
          const row = {
            id: `created-${created.length}`,
            name,
            parent_id: options?.parent_id ?? undefined,
          };
          created.push(row);
          return row;
        },
        createDocument: async (
          _space: string,
          input: { title: string; folder_id?: string | null; parent_id?: string | null },
        ) => {
          const row = {
            id: "created-page",
            title: input.title,
            folder_id: input.folder_id ?? undefined,
            parent_id: input.parent_id ?? undefined,
          };
          documents.push(row);
          return row;
        },
      } as unknown as NotionImportTargetClient;
      const result = await executeNotionImportPlan(client, "space", plan);
      expect(result.status).toBe("completed");
      expect(result.createdFolderCount).toBe(depth);
      for (let i = 1; i < depth; i++) expect(created[i].parent_id).toBe(created[i - 1].id);
      expect(documents[0].folder_id).toBe(created[depth - 1].id);
      expect(plan.warnings.some((warning) => warning.includes("folder nesting"))).toBe(false);
    },
  );
});

describe("deep Notion child pages", () => {
  it.each([4, 32, 256])(
    "plans and executes %i page parents and retains returned draft identities",
    async (depth) => {
      const files: Record<string, string> = {};
      for (let i = 1; i <= depth; i++)
        files[`Workspace/${Array(i).fill("x").join("/")}.md`] = `Exact body ${i}`;
      const plan = await buildNotionImportPlan("Workspace.zip", makeArchive(files));
      expect(plan.documents).toHaveLength(depth);
      expect(plan.documents[plan.documents.length - 1]?.depth).toBe(depth);
      const created: Array<{ id: string; parent_id?: string | null; content: string }> = [];
      const client = {
        listFolders: async () => [],
        listDocuments: async () => [],
        createFolder: async () => {
          throw Error("No folder expected");
        },
        createDocument: async (
          _space: string,
          input: { content: string; parent_id?: string | null },
        ) => {
          const id = `created-${created.length}`;
          created.push({ id, ...input });
          return { id, draft_id: `draft-${id}`, draft_source_digest: "a".repeat(64) };
        },
      } as unknown as NotionImportTargetClient;
      const result = await executeNotionImportPlan(client, "space", plan);
      expect(result.status).toBe("completed");
      expect(created).toHaveLength(depth);
      for (let i = 0; i < depth; i++) {
        expect(created[i]).toMatchObject({
          parent_id: i ? created[i - 1].id : null,
          content: `Exact body ${i + 1}`,
        });
        expect(result.documentIdsBySourcePath[plan.documents[i].sourcePath]).toBe(created[i].id);
        expect(result.documentDraftIdsBySourcePath[plan.documents[i].sourcePath]).toBe(
          `draft-${created[i].id}`,
        );
        expect(result.documentDraftSourcesBySourcePath?.[plan.documents[i].sourcePath]).toBe(
          "a".repeat(64),
        );
      }
    },
  );
  it("holds the pure page-operation boundary at10000 without lowering the archive bound", () => {
    const rows = Array.from({ length: 10000 }, (_, i) => ({
      key: `p${i}`,
      sourcePath: `p${i}.md`,
      title: `P${i}`,
      content: "body",
      kind: "page" as const,
      depth: 1,
    }));
    expect(() => validateNotionImportPagePlan(rows)).not.toThrow();
    expect(() => validateNotionImportPagePlan([...rows, { ...rows[0], key: "extra" }])).toThrow(
      "10000",
    );
    expect(() =>
      validateNotionImportPagePlan([{ ...rows[0], parentKey: "private-missing" }]),
    ).toThrow("parent");
    expect(() => validateNotionImportPagePlan([{ ...rows[0], parentKey: "p0" }])).toThrow("parent");
  });
});

describe("Notion deep parent compatibility", () => {
  it("retains empty-parent and intervening-folder warnings instead of forging page ancestry", async () => {
    const plan = await buildNotionImportPlan(
      "Workspace.zip",
      makeArchive({
        "Workspace/Empty.md": " ",
        "Workspace/Empty/Child.md": "child",
        "Workspace/Parent.md": "parent",
        "Workspace/Parent/Plain/Child.md": "child in folder",
      }),
    );
    expect(
      plan.documents.find((row) => row.sourcePath === "Workspace/Empty/Child.md")?.parentKey,
    ).toBeUndefined();
    expect(
      plan.documents.find((row) => row.sourcePath === "Workspace/Parent/Plain/Child.md")?.parentKey,
    ).toBeUndefined();
    expect(
      plan.warnings.some((warning) => warning.includes("parent page is empty or unavailable")),
    ).toBe(true);
    expect(plan.warnings.some((warning) => warning.includes("intervening plain folder"))).toBe(
      true,
    );
  });
  it("rejects an invalid reviewed page plan before reading or mutating the target", async () => {
    const listFolders = vi.fn(async () => []),
      createDocument = vi.fn();
    await expect(
      executeNotionImportPlan(
        { listFolders, createDocument } as unknown as NotionImportTargetClient,
        "space",
        {
          archiveName: "Bad.zip",
          folders: [],
          documents: [
            {
              key: "child",
              sourcePath: "child.md",
              title: "Child",
              content: "body",
              kind: "page",
              depth: 1,
              parentKey: "unlisted-private",
            },
          ],
          warnings: [],
          skippedPaths: [],
        },
      ),
    ).rejects.toThrow("parent");
    expect(listFolders).not.toHaveBeenCalled();
    expect(createDocument).not.toHaveBeenCalled();
  });
});
