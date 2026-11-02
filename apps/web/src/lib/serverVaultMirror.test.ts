import * as localVault from "./localVault";
import { setCurrentLaunchMode } from "./launchMode";
import { describe, expect, it, vi } from "vitest";
import {
  deriveServerVaultFolderName,
  queueServerResponseForVaultMirror,
  flushServerVaultMirror,
  ensureServerVaultFolder,
  mergeServerAttachmentIntoVaultSnapshot,
  mergeServerResponseIntoVaultSnapshot,
} from "./serverVaultMirror";
import {
  LOCAL_VAULT_SCHEMA_VERSION,
  type LocalVaultSnapshot,
} from "./localVault";

function createMemoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => values.delete(key),
    setItem: (key, value) => values.set(key, value),
  };
}

function emptySnapshot(): LocalVaultSnapshot {
  return {
    schema_version: LOCAL_VAULT_SCHEMA_VERSION,
    spaces: [],
    folders: [],
    documents: [],
    assets: [],
    templates: [],
  };
}

describe("server vault mirror", () => {
  it("never opens or saves the native vault for inspection but still mirrors page responses", async () => {
    const localStorage = createMemoryStorage();
    localStorage.setItem("server_url", "https://notes.example.com");
    vi.stubGlobal("window", {
      __TAURI_INTERNALS__: {},
      localStorage,
      setTimeout: vi.fn(() => 1),
      clearTimeout: vi.fn(),
    });
    vi.stubGlobal("localStorage", localStorage);
    setCurrentLaunchMode("server");
    const stored = emptySnapshot();
    stored.documents = [
      {
        id: "p",
        space_id: "s",
        title: "Old page",
        content: "Retained body",
        maintenance_user_id: "person",
        maintenance_group_id: null,
        created_at: "2026-10-08T00:00:00Z",
        updated_at: "2026-10-08T00:00:00Z",
      },
    ];
    const init = vi.fn(async () => undefined),
      saveSnapshot = vi.fn(async () => undefined);
    const adapter = vi
      .spyOn(localVault, "createLocalVaultAdapter")
      .mockReturnValue({
        init,
        loadSnapshot: async () => stored,
        saveSnapshot,
      } as unknown as ReturnType<typeof localVault.createLocalVaultAdapter>);
    try {
      for (const path of [
        "/spaces/s/documents/p/access",
        "/spaces/s/documents/p/access/inspect",
        "/spaces/s/documents/p/restrictions/inspect",
        "/spaces/s/documents/p/restrictions/preview",
        "/spaces/s/documents/p/restrictions/apply",
        "/spaces/s/documents/p/restrictions/inspect?fresh=1",
        "/spaces/s/documents/p/restrictions/preview?fresh=1",
        "/spaces/s/documents/p/restrictions/apply?fresh=1",
        "/moves/documents/p/preserved/inspect",
        "/moves/documents/p/preserved/inspect?cursor=older-op",
        "/moves/commits/op/preserved/p/inspect",
      ])
        queueServerResponseForVaultMirror(
          "https://notes.example.com/api/v1",
          path,
          { schema_version: 1 },
        );
      await flushServerVaultMirror();
      expect(adapter).not.toHaveBeenCalled();
      expect(init).not.toHaveBeenCalled();
      expect(saveSnapshot).not.toHaveBeenCalled();
      expect(window.setTimeout).not.toHaveBeenCalled();
      queueServerResponseForVaultMirror(
        "https://notes.example.com/api/v1",
        "/spaces/s/documents",
        [{ id: "p", title: "Page", updated_at: "2026-10-08T00:00:00Z" }],
      );
      await flushServerVaultMirror();
      expect(init).toHaveBeenCalledTimes(1);
      expect(saveSnapshot).toHaveBeenCalledWith(
        expect.objectContaining({
          documents: [
            expect.objectContaining({
              id: "p",
              title: "Page",
              content: "Retained body",
              maintenance_user_id: "person",
            }),
          ],
        }),
      );
    } finally {
      adapter.mockRestore();
      vi.unstubAllGlobals();
    }
  });
  it("carries known maintenance pair through page summary and body without granting draft ownership", () => {
    const snapshot = mergeServerResponseIntoVaultSnapshot(emptySnapshot(), "/spaces/space-1/documents", [{id:"page",title:"Page",maintenance_user_id:null,maintenance_group_id:"team",updated_at:"2026-10-08T00:00:00Z"}]);
    const body = mergeServerResponseIntoVaultSnapshot(snapshot,"/spaces/space-1/documents/page/content",{id:"page",title:"Page",content:"Exact body"});
    expect(body.documents[0]).toMatchObject({maintenance_user_id:null,maintenance_group_id:"team",content:"Exact body"});
    const cleared = mergeServerResponseIntoVaultSnapshot(body,"/spaces/space-1/documents",[{id:"page",title:"Page",maintenance_user_id:null,maintenance_group_id:null}]);
    expect(cleared.documents[0]).toMatchObject({maintenance_user_id:null,maintenance_group_id:null});
  });
  it("retains identifier through canonical list/content/draft mirror projections and explicit clears", () => {
    let snapshot = mergeServerResponseIntoVaultSnapshot(emptySnapshot(), "/spaces/space-1/documents", [{ id: "page", title: "Published", human_identifier: "ENG-0042", updated_at: "2026-10-08T00:00:00Z" }]);
    snapshot = mergeServerResponseIntoVaultSnapshot(snapshot, "/spaces/space-1/documents/page/content", { id: "page", title: "Published", content: "Exact body" });
    snapshot = mergeServerResponseIntoVaultSnapshot(snapshot, "/spaces/space-1/documents/page/drafts/draft", { id: "draft", title: "Private", content: "Exact draft", owner_id: "account", updated_at: "2026-10-08T00:00:00Z" });
    expect(snapshot.documents[0]).toMatchObject({ human_identifier: "ENG-0042", content: "Exact draft" });
    const cleared = mergeServerResponseIntoVaultSnapshot(snapshot, "/spaces/space-1/documents", [{ id: "page", title: "Published", human_identifier: null }]);
    expect(cleared.documents[0].human_identifier).toBeNull();
  });

  it("derives a stable visible vault folder name from the connected server", () => {
    expect(deriveServerVaultFolderName("https://notes.example.com/")).toBe(
      "notes.example.com",
    );
    expect(deriveServerVaultFolderName("http://localhost:3000")).toBe("localhost-3000");
    expect(deriveServerVaultFolderName("not a url")).toBe("Starlane Notes Server");
  });

  it("creates and selects the server-named native vault folder during connection", async () => {
    const localStorage = createMemoryStorage();
    const invoke = vi.fn().mockResolvedValue({
      path: "/vaults/notes.example.com",
      using_default: false,
      configured: true,
    });
    vi.stubGlobal("window", {
      __TAURI_INTERNALS__: { invoke },
      localStorage,
      clearTimeout: vi.fn(),
    });
    vi.stubGlobal("localStorage", localStorage);

    await ensureServerVaultFolder("https://notes.example.com/");

    expect(invoke).toHaveBeenCalledWith("create_vault_folder", {
      name: "notes.example.com",
    });
    vi.unstubAllGlobals();
  });

  it("stores basic metadata for every accessible space", () => {
    const snapshot = mergeServerResponseIntoVaultSnapshot(
      emptySnapshot(),
      "/spaces",
      [
        {
          id: "personal",
          name: "Personal Space",
          slug: "personal",
          created_by: "user-1",
        },
        {
          id: "shared",
          name: "Shared Space",
          slug: "shared",
          created_by: "user-2",
        },
      ],
      "2026-07-26T21:00:00.000Z",
    );

    expect(snapshot.spaces).toEqual([
      expect.objectContaining({
        id: "personal",
        name: "Personal Space",
        created_by: "user-1",
      }),
      expect.objectContaining({
        id: "shared",
        name: "Shared Space",
        created_by: "user-2",
      }),
    ]);
    expect(snapshot.folders).toEqual([]);
    expect(snapshot.documents).toEqual([]);
  });

  it("lazily merges opened folders, page metadata, and page bodies without losing content", () => {
    let snapshot = mergeServerResponseIntoVaultSnapshot(
      emptySnapshot(),
      "/spaces/shared/folders",
      [{ id: "folder-1", name: "Runbooks", parent_id: null }],
    );
    snapshot = mergeServerResponseIntoVaultSnapshot(
      snapshot,
      "/spaces/shared/documents?folder_id=folder-1",
      [
        {
          id: "doc-1",
          title: "Deploy",
          folder_id: "folder-1",
          updated_at: "2026-07-26T21:00:00.000Z",
        },
      ],
    );
    snapshot = mergeServerResponseIntoVaultSnapshot(
      snapshot,
      "/spaces/shared/documents/doc-1/content",
      { id: "doc-1", title: "Deploy", content: "# Ship it" },
    );
    snapshot = mergeServerResponseIntoVaultSnapshot(
      snapshot,
      "/spaces/shared/documents?folder_id=folder-1",
      [{ id: "doc-1", title: "Deploy safely", folder_id: "folder-1" }],
    );

    expect(snapshot.folders).toEqual([
      expect.objectContaining({ id: "folder-1", space_id: "shared", name: "Runbooks" }),
    ]);
    expect(snapshot.documents).toEqual([
      expect.objectContaining({
        id: "doc-1",
        space_id: "shared",
        folder_id: "folder-1",
        title: "Deploy safely",
        content: "# Ship it",
      }),
    ]);
  });

  it("mirrors the current private draft body instead of leaving an empty page shell", () => {
    let snapshot = mergeServerResponseIntoVaultSnapshot(
      emptySnapshot(),
      "/spaces/personal/documents",
      [{ id: "doc-1", title: "Draft page" }],
    );
    snapshot = mergeServerResponseIntoVaultSnapshot(
      snapshot,
      "/spaces/personal/documents/doc-1/drafts/draft-1",
      {
        id: "draft-1",
        document_id: "doc-1",
        space_id: "personal",
        owner_id: "user-1",
        title: "Draft page",
        content: "# Actual draft content",
        icon: "📝",
        base_updated_at: "2026-07-27T17:00:00.000Z",
        created_at: "2026-07-27T17:00:00.000Z",
        updated_at: "2026-07-27T18:00:00.000Z",
      },
    );

    expect(snapshot.documents).toEqual([
      expect.objectContaining({
        id: "doc-1",
        title: "Draft page",
        content: "# Actual draft content",
        icon: "📝",
        updated_at: "2026-07-27T18:00:00.000Z",
      }),
    ]);
    expect(snapshot.drafts).toEqual([
      expect.objectContaining({
        id: "draft-1",
        document_id: "doc-1",
        space_id: "personal",
        owner_id: "user-1",
        content: "# Actual draft content",
      }),
    ]);
  });

  it("removes a deleted server draft from the native mirror snapshot", () => {
    const snapshot = mergeServerResponseIntoVaultSnapshot(
      {
        ...emptySnapshot(),
        drafts: [
          {
            id: "draft-1",
            document_id: "doc-1",
            space_id: "personal",
            owner_id: "user-1",
            title: "Draft page",
            content: "Body",
            base_updated_at: "2026-07-27T17:00:00.000Z",
            created_at: "2026-07-27T17:00:00.000Z",
            updated_at: "2026-07-27T18:00:00.000Z",
          },
        ],
      },
      "/spaces/personal/documents/doc-1/drafts/draft-1",
      null,
    );

    expect(snapshot.drafts).toEqual([]);
  });

  it("merges an offline draft autosave patch into the existing native draft", () => {
    const snapshot = mergeServerResponseIntoVaultSnapshot(
      {
        ...emptySnapshot(),
        drafts: [
          {
            id: "draft-1",
            document_id: "doc-1",
            space_id: "personal",
            owner_id: "user-1",
            title: "Draft page",
            content: "Old body",
            base_updated_at: "2026-07-27T17:00:00.000Z",
            created_at: "2026-07-27T17:00:00.000Z",
            updated_at: "2026-07-27T18:00:00.000Z",
          },
        ],
      },
      "/spaces/personal/documents/doc-1/drafts/draft-1",
      { content: "Offline autosave" },
      "2026-07-27T19:00:00.000Z",
    );

    expect(snapshot.drafts).toEqual([
      expect.objectContaining({
        id: "draft-1",
        owner_id: "user-1",
        title: "Draft page",
        content: "Offline autosave",
        updated_at: "2026-07-27T19:00:00.000Z",
      }),
    ]);
  });

  it("removes every mirrored draft after a successful space purge", () => {
    const draft = {
      id: "draft-1",
      document_id: "doc-1",
      space_id: "personal",
      owner_id: "user-1",
      title: "Draft page",
      content: "Body",
      base_updated_at: "2026-07-27T17:00:00.000Z",
      created_at: "2026-07-27T17:00:00.000Z",
      updated_at: "2026-07-27T18:00:00.000Z",
    };
    const snapshot = mergeServerResponseIntoVaultSnapshot(
      {
        ...emptySnapshot(),
        drafts: [draft, { ...draft, id: "draft-2", space_id: "shared" }],
      },
      "/spaces/personal/drafts",
      { deleted: 1 },
    );

    expect(snapshot.drafts).toEqual([expect.objectContaining({ id: "draft-2" })]);
  });

  it("removes a published server draft from the native mirror snapshot", () => {
    const snapshot = mergeServerResponseIntoVaultSnapshot(
      {
        ...emptySnapshot(),
        drafts: [
          {
            id: "draft-1",
            document_id: "doc-1",
            space_id: "personal",
            owner_id: "user-1",
            title: "Draft page",
            content: "Body",
            base_updated_at: "2026-07-27T17:00:00.000Z",
            created_at: "2026-07-27T17:00:00.000Z",
            updated_at: "2026-07-27T18:00:00.000Z",
          },
        ],
      },
      "/spaces/personal/documents/doc-1/drafts/draft-1/publish",
      { id: "doc-1", title: "Published page" },
    );

    expect(snapshot.drafts).toEqual([]);
  });

  it("stores an accessed attachment with its downloaded bytes and ownership", () => {
    const snapshot = mergeServerAttachmentIntoVaultSnapshot(
      emptySnapshot(),
      "space-1",
      "doc-1",
      {
        id: "asset-1",
        filename: "diagram.png",
        original_name: "Architecture.png",
        mime_type: "image/png",
        size_bytes: 42,
        url: "/api/v1/files/diagram.png",
        created_at: "2026-07-26T21:00:00.000Z",
      },
      "data:image/png;base64,cG5n",
    );

    expect(snapshot.assets).toEqual([
      {
        id: "asset-1",
        name: "Architecture.png",
        mime_type: "image/png",
        size: 42,
        url: "data:image/png;base64,cG5n",
        source: "browser",
        space_id: "space-1",
        document_id: "doc-1",
        created_at: "2026-07-26T21:00:00.000Z",
      },
    ]);
  });

  it("persists an offline-created document and its content", () => {
    let snapshot = mergeServerResponseIntoVaultSnapshot(
      emptySnapshot(),
      "/spaces/space-1/documents",
      { id: "doc-1", title: "Offline note" },
    );
    snapshot = mergeServerResponseIntoVaultSnapshot(
      snapshot,
      "/spaces/space-1/documents/doc-1/content",
      { id: "doc-1", title: "Offline note", content: "Saved before upload" },
    );

    expect(snapshot.documents).toEqual([
      expect.objectContaining({ id: "doc-1", title: "Offline note", content: "Saved before upload" }),
    ]);
  });

  it("removes offline-deleted documents from the durable snapshot", () => {
    const created = mergeServerResponseIntoVaultSnapshot(
      emptySnapshot(),
      "/spaces/space-1/documents",
      { id: "doc-1", title: "Offline note" },
    );
    const deleted = mergeServerResponseIntoVaultSnapshot(
      created,
      "/spaces/space-1/documents/doc-1",
      null,
    );

    expect(deleted.documents).toEqual([]);
  });
});
