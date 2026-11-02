import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import {
  createSidebarFolderCatalog,
  createSidebarPageCatalog,
  getSidebarCreateLocationOptions,
  settleSidebarPageCreation,
  completeLocalSidebarMove,
  canCreateSidebarChildFolder,
  canMoveSidebarFolderAcrossSpaces,
  canDropSidebarFolderOnParent,
  canDropSidebarFolderOnRoot,
  canMoveSidebarDocumentToParent,
  canMoveSidebarFolderToRoot,
  canMoveSidebarFolderToParent,
  getSidebarDocumentDepth,
  getSidebarDocumentPathLabel,
  getSidebarDocumentPath,
  isCurrentSidebarSpaceRequest,
  getSidebarAccountMenuItemClass,
  getSidebarDeleteConfirmationMessage,
  getSidebarDeleteConfirmationTitle,
  getSidebarTemplatePickerTarget,
  getSidebarRenameEmojiLabel,
  getSidebarRenameEmojiPresets,
  getSidebarFolderMoveTargets,
  getSidebarFolderPathLabel,
  getSidebarFolderDepth,
  getSidebarActiveFolderIds,
  getSidebarFilesCreateActionClass,
  getSidebarNavigationTargetClass,
  getSidebarNavigationAriaCurrent,
  getSidebarNavigationItemStateClass,
  getSidebarUtilityActionClass,
  getSidebarUtilityGroupClass,
  getSidebarTreeActionClass,
  getSidebarCurrentTreeItemId,
  getSidebarTreeExpansionStorageKey,
  getSidebarTreeItemTabIndex,
  getSidebarTreeNavigationIndex,
  getSidebarTreeRowClass,
  getSidebarTreeToggleClass,
  getSidebarGraphPath,
  getSidebarHomePath,
  getSidebarJournalPath,
  getSidebarMenuNavigationIndex,
  getSidebarRecentPath,
  getSidebarSettingsNavigationOptions,
  getSidebarSpaceSelectorChevronClass,
  getSidebarSpaceSettingsPath,
  shouldRefreshSidebarContents,
  getSidebarStarredPath,
  getSidebarWorkflowPath,
  isSidebarRouteActive,
  isSidebarDocumentDescendant,
  OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT,
  openSidebarDocumentMoveDialog,
  SIDEBAR_DANGER_MENU_ITEM_CLASS,
  SIDEBAR_COMPACT_FILES_REVEAL_CLASS,
  SIDEBAR_SECURITY_SETTINGS_PATH,
  SIDEBAR_PROFILE_SETTINGS_PATH,
  SIDEBAR_SITE_ADMINISTRATION_PATH,
  SIDEBAR_TREE_ICON_SIZE,
  SIDEBAR_TREE_NESTED_GROUP_CLASS,
  SIDEBAR_TREE_TOGGLE_LANE_CLASS,
  Sidebar,
  RowActionMenu,
  shouldShowSidebarSiteAdministration,
  shouldShowSidebarAppSettings,
  shouldQueueSidebarDailyNote,
  sortSidebarDocuments,
  sortSidebarFolders,
} from "./Sidebar";

const navigationFixture = vi.hoisted(() => ({ pathname: "/", mode: "local", selected: false }));
const runtimeFixture = vi.hoisted(() => ({ runtime: "web" as "web" | "tauri-desktop" }));

vi.mock("./WorkspaceSpaces", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./WorkspaceSpaces")>();
  return {
    ...actual,
    useWorkspaceSpaces: () => {
      const context = actual.useWorkspaceSpaces();
      if (!navigationFixture.selected) return context;
      const space = {
        id: "space-1",
        name: "Selected space",
        slug: "selected",
        member_role: "owner" as const,
      };
      return { ...context, spaces: [space], activeSpace: space };
    },
  };
});

vi.mock("react-router-dom", () => ({
  useNavigate: () => vi.fn(),
  useLocation: () => ({ pathname: navigationFixture.pathname }),
  useParams: () => ({}),
}));

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    user: {
      id: "local-user",
      email: "local@example.test",
      display_name: "Local User",
      role: "owner",
      totp_enabled: false,
    },
  }),
}));

vi.mock("@/lib/launchMode", () => ({
  getCurrentLaunchMode: () => navigationFixture.mode,
}));

vi.mock("@/hooks/useRuntimePlatform", () => ({
  useRuntimePlatform: () => ({
    runtime: runtimeFixture.runtime,
    loading: false,
    isMobileRuntime: false,
    isMobileBrowser: false,
    isMobileViewport: false,
    isDesktopRuntime: runtimeFixture.runtime === "tauri-desktop",
  }),
}));

vi.mock("@/components/BrandLogo", () => ({
  BrandLogo: () => <span>Starlane Notes</span>,
}));

describe("Sidebar ordering", () => {
  it("refreshes the Files tree on structural graph changes but not content autosaves", () => {
    expect(
      shouldRefreshSidebarContents("space-1", { spaceId: "space-1", reason: "document-created" }),
    ).toBe(true);
    expect(
      shouldRefreshSidebarContents("space-1", { spaceId: "space-1", reason: "document-deleted" }),
    ).toBe(true);
    expect(
      shouldRefreshSidebarContents("space-1", {
        spaceId: "space-1",
        reason: "document-metadata-updated",
      }),
    ).toBe(true);
    expect(
      shouldRefreshSidebarContents("space-1", {
        spaceId: "space-1",
        reason: "document-content-updated",
      }),
    ).toBe(false);
    expect(
      shouldRefreshSidebarContents("space-2", { spaceId: "space-1", reason: "document-created" }),
    ).toBe(false);

    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );
    expect(source).toContain("addGraphDirtyListener(");
    expect(source).toContain(
      "shouldRefreshSidebarContents(activeSpaceIdRef.current, { spaceId, reason })",
    );
    expect(source).toContain("refreshSpaceContents(spaceId)");
    expect(source).toContain("spaceContentsRefreshRequestIdRef.current");
  });

  it("clears space-owned tree data and guards the actual load effect", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain("const requestSpaceId = activeSpaceId;");
    expect(source).toContain("const isCurrentSpaceRequest = () =>");
    expect(source).toContain("const isCurrentTreeRequest = () =>");
    expect(source).toContain("setFolders([]);");
    expect(source).toContain("setDocuments([]);");
    expect(source).toContain("setTemplates([]);");
    expect(source).toContain('spaceId={activeSpace?.id || ""}');
    expect(source).toContain("navigate(getSidebarDocumentPath(spaceId, doc.id))");
    expect(source).toContain("dataClient.deleteDocument(activeSpace.id, docId)");
  });

  it("keeps Files contents and document routes on the newest active space", async () => {
    type Contents = { folders: string[]; documents: string[]; templates: string[] };
    const deferred = () => {
      let resolve!: (value: Contents) => void;
      let reject!: (reason?: unknown) => void;
      const promise = new Promise<Contents>((resolvePromise, rejectPromise) => {
        resolve = resolvePromise;
        reject = rejectPromise;
      });
      return { promise, resolve, reject };
    };

    let current = { spaceId: "space-a", requestId: 1 };
    let contents: Contents = { folders: [], documents: [], templates: [] };
    let error = "";
    const applyRequest = async (spaceId: string, requestId: number, request: Promise<Contents>) => {
      try {
        const result = await request;
        if (isCurrentSidebarSpaceRequest(spaceId, requestId, current.spaceId, current.requestId)) {
          contents = result;
          error = "";
        }
      } catch {
        if (isCurrentSidebarSpaceRequest(spaceId, requestId, current.spaceId, current.requestId)) {
          contents = { folders: [], documents: [], templates: [] };
          error = `Failed to load ${spaceId}`;
        }
      }
    };

    const lateRows = deferred();
    const lateFailure = deferred();
    const newestRows = deferred();
    const firstRequest = applyRequest("space-a", 1, lateRows.promise);
    const failedRequest = applyRequest("space-a", 1, lateFailure.promise);
    current = { spaceId: "space-b", requestId: 2 };
    const newestRequest = applyRequest("space-b", 2, newestRows.promise);
    newestRows.resolve({
      folders: ["b-folder"],
      documents: ["b-document"],
      templates: ["b-template"],
    });
    await newestRequest;
    lateRows.resolve({
      folders: ["a-folder"],
      documents: ["a-document"],
      templates: ["a-template"],
    });
    lateFailure.reject(new Error("stale request failure"));
    await Promise.all([firstRequest, failedRequest]);

    expect(contents).toEqual({
      folders: ["b-folder"],
      documents: ["b-document"],
      templates: ["b-template"],
    });
    expect(error).toBe("");
    expect(getSidebarDocumentPath("space-b", contents.documents[0])).toBe(
      "/spaces/space-b/docs/b-document",
    );
  });

  it("opens desktop settings over the current workspace and keeps mobile destinations standalone", () => {
    const location = {
      pathname: "/spaces/space-1/docs/doc-1",
      search: "",
      hash: "",
      state: null,
      key: "workspace",
    };

    expect(getSidebarSettingsNavigationOptions(false, location)).toEqual({
      state: { backgroundLocation: location },
    });
    expect(getSidebarSettingsNavigationOptions(true, location)).toBeUndefined();
  });

  it("shows the space selector's down chevron when closed and up chevron when open", () => {
    expect(getSidebarSpaceSelectorChevronClass(false)).not.toContain("rotate-180");
    expect(getSidebarSpaceSelectorChevronClass(true)).toContain("rotate-180");
  });

  it("cycles enabled account-menu actions with the shared menu keys", () => {
    expect(getSidebarMenuNavigationIndex(0, 3, "ArrowDown")).toBe(1);
    expect(getSidebarMenuNavigationIndex(2, 3, "ArrowDown")).toBe(0);
    expect(getSidebarMenuNavigationIndex(0, 3, "ArrowUp")).toBe(2);
    expect(getSidebarMenuNavigationIndex(1, 3, "Home")).toBe(0);
    expect(getSidebarMenuNavigationIndex(1, 3, "End")).toBe(2);
    expect(getSidebarMenuNavigationIndex(0, 0, "ArrowDown")).toBe(-1);
  });

  it("focuses and directionally navigates the searchable space selector", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain("spaceSearchInputRef.current?.focus()");
    expect(source).toContain("const handleSpaceMenuKeyDown");
    expect(source).toContain("ref={spaceMenuContentRef}");
    expect(source).toContain("onKeyDown={handleSpaceMenuKeyDown}");
    expect(source).toContain("button[role='menuitemradio']:not(:disabled)");
  });

  it("does not refocus the create title when the title or destination changes", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain("const createDialogKind = createDialog?.kind");
    expect(source).toContain("if (createDialogKind) createTitleInputRef.current?.focus()");
    expect(source).toContain("}, [createDialogKind]);");
    expect(source).not.toContain("}, [createDialog]);");
  });

  it("opens new page and folder dialogs with empty values", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain('kind: "folder",\n        title: "",');
    expect(source).toContain('kind: "document",\n        title: "",');
  });

  it("uses the shared anchored popover for account menu dismissal", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain("open={accountMenuOpen}");
    expect(source).toContain('align="start"');
    expect(source).toContain('side="top"');
    expect(source).toContain('aria-label="Account"');
    expect(source).not.toContain("closeAccountMenuAndRestoreFocus");
  });

  it("loads nested documents for the Files tree and post-create refreshes", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain(".listDocuments(activeSpace.id, { include_nested: true })");
    expect(source).toContain(
      "dataClient.listDocuments(moveTargetSpaceId, { include_nested: true })",
    );
    expect(source).toContain("dataClient.listDocuments(spaceId, { include_nested: true })");
  });

  it("exposes duplicate and archive actions for page rows", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain('label: "Duplicate"');
    expect(source).toContain('label: "Archive"');
    expect(source).toContain("dataClient.duplicateDocument(activeSpace.id, docId)");
    expect(source).toContain("dataClient.archiveDocument(activeSpace.id, docId)");
  });

  it("loads a move source missing from the cached list and reports lookup failures", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );
    const moveSource = source
      .split("const openMoveDocument = useCallback(")[1]
      ?.split("const renderSpaceOption =")[0];

    expect(moveSource).toContain(".getDocument(sourceSpaceId, docId)");
    expect(moveSource).toContain("setDocuments((current) =>");
    expect(moveSource).toContain("Could not load page for moving");
    expect(source).toContain("emitDocumentLocationChanged({");
  });

  it("moves tree focus through visible nodes without wrapping", () => {
    expect(getSidebarTreeNavigationIndex(0, 3, "ArrowDown")).toBe(1);
    expect(getSidebarTreeNavigationIndex(2, 3, "ArrowDown")).toBe(2);
    expect(getSidebarTreeNavigationIndex(0, 3, "ArrowUp")).toBe(0);
    expect(getSidebarTreeNavigationIndex(2, 3, "Home")).toBe(0);
    expect(getSidebarTreeNavigationIndex(0, 3, "End")).toBe(2);
    expect(getSidebarTreeNavigationIndex(0, 0, "ArrowDown")).toBe(-1);
  });

  it("keeps Files tree keyboard navigation on one meaningful tab stop", () => {
    expect(getSidebarTreeItemTabIndex(null, "folder:projects", null, "folder:projects")).toBe(0);
    expect(
      getSidebarTreeItemTabIndex(null, "document:brief", "document:brief", "folder:projects"),
    ).toBe(0);
    expect(
      getSidebarTreeItemTabIndex(null, "folder:projects", "document:brief", "folder:projects"),
    ).toBe(-1);
    expect(
      getSidebarTreeItemTabIndex(
        "folder:notes",
        "folder:notes",
        "document:brief",
        "folder:projects",
      ),
    ).toBe(0);
    expect(
      getSidebarTreeItemTabIndex(
        "folder:notes",
        "document:brief",
        "document:brief",
        "folder:projects",
      ),
    ).toBe(-1);
  });

  it("marks only the routed page as the current Files-tree item", () => {
    const documents = [
      { id: "brief", title: "Brief" },
      { id: "brief-archive", title: "Brief archive" },
    ];

    expect(getSidebarCurrentTreeItemId(documents, "space-1", "/spaces/space-1/docs/brief")).toBe(
      "document:brief",
    );
    expect(
      getSidebarCurrentTreeItemId(documents, "space-1", "/spaces/space-1/docs/brief-archive"),
    ).toBe("document:brief-archive");
    expect(getSidebarCurrentTreeItemId(documents, "space-1", "/spaces/space-1/docs")).toBeNull();
  });

  it("exposes local Storage Administration and role-gates server Site Administration", () => {
    expect(SIDEBAR_SITE_ADMINISTRATION_PATH).toBe("/admin");
    expect(SIDEBAR_SECURITY_SETTINGS_PATH).toBe("/profile/security");
    expect(shouldShowSidebarSiteAdministration(false, "admin")).toBe(true);
    expect(shouldShowSidebarSiteAdministration(false, "member")).toBe(false);
    expect(shouldShowSidebarSiteAdministration(true, "admin")).toBe(true);
    expect(shouldShowSidebarSiteAdministration(true, undefined)).toBe(true);
    expect(shouldShowSidebarSiteAdministration(false, undefined)).toBe(false);
  });

  it("keeps App Settings reachable in the mobile drawer for local and server modes", () => {
    expect(shouldShowSidebarAppSettings(true, true)).toBe(true);
    expect(shouldShowSidebarAppSettings(false, true)).toBe(true);
    expect(shouldShowSidebarAppSettings(true, false)).toBe(true);
    expect(shouldShowSidebarAppSettings(false, false)).toBe(false);
  });

  it("queues a daily-note request until the mobile drawer knows its active space", () => {
    expect(shouldQueueSidebarDailyNote(null)).toBe(true);
    expect(shouldQueueSidebarDailyNote({ id: "space-1" })).toBe(false);
  });

  it("sorts folders alphabetically without mutating the source list", () => {
    const folders = [
      { id: "z", name: "Zeta" },
      { id: "a", name: "alpha" },
      { id: "m", name: "Middle" },
    ];

    expect(sortSidebarFolders(folders).map((folder) => folder.id)).toEqual(["a", "m", "z"]);
    expect(folders.map((folder) => folder.id)).toEqual(["z", "a", "m"]);
  });

  it("sorts pages alphabetically without mutating the source list", () => {
    const documents = [
      { id: "z", title: "Zebra" },
      { id: "a", title: "alpha" },
      { id: "m", title: "Meeting" },
    ];

    expect(sortSidebarDocuments(documents).map((doc) => doc.id)).toEqual(["a", "m", "z"]);
    expect(documents.map((doc) => doc.id)).toEqual(["z", "a", "m"]);
  });

  it("persists tree expansion per space and opens the active page's folder branch", () => {
    const folders = [
      { id: "projects", name: "Projects" },
      { id: "launch", name: "Launch", parent_id: "projects" },
    ];
    const documents = [
      { id: "overview", title: "Overview", folder_id: "launch" },
      { id: "child", title: "Child", folder_id: "launch", parent_id: "overview" },
    ];

    expect(getSidebarTreeExpansionStorageKey("space-a")).toBe(
      "starlane_sidebar_expanded_folders:space-a",
    );
    expect(
      getSidebarActiveFolderIds(folders, documents, "/spaces/space-a/documents/overview"),
    ).toEqual(["launch", "projects"]);
    expect(
      getSidebarActiveFolderIds(folders, documents, "/spaces/space-a/documents/child"),
    ).toEqual(["launch", "projects"]);
    expect(getSidebarActiveFolderIds(folders, documents, "/spaces/space-a/home")).toEqual([]);
  });

  it("limits child folder creation affordances to level 3", () => {
    const folders = [
      { id: "root", name: "Root" },
      { id: "child", name: "Child", parent_id: "root" },
      { id: "grandchild", name: "Grandchild", parent_id: "child" },
    ];

    expect(getSidebarFolderDepth(folders, "root")).toBe(1);
    expect(getSidebarFolderDepth(folders, "child")).toBe(2);
    expect(getSidebarFolderDepth(folders, "grandchild")).toBe(3);
    expect(canCreateSidebarChildFolder(folders, "root")).toBe(true);
    expect(canCreateSidebarChildFolder(folders, "child")).toBe(true);
    expect(canCreateSidebarChildFolder(folders, "grandchild")).toBe(true);
  });

  it("shows move-to-root only for nested local folders", () => {
    expect(canMoveSidebarFolderToRoot({ parent_id: "root" }, true)).toBe(true);
    expect(canMoveSidebarFolderToRoot({ parent_id: "root" }, false)).toBe(false);
    expect(canMoveSidebarFolderToRoot({}, true)).toBe(false);
  });

  it("allows root drops only for nested local folders", () => {
    const folders = [
      { id: "root", name: "Root" },
      { id: "child", name: "Child", parent_id: "root" },
    ];

    expect(canDropSidebarFolderOnRoot(folders, "child", true)).toBe(true);
    expect(canDropSidebarFolderOnRoot(folders, "child", false)).toBe(false);
    expect(canDropSidebarFolderOnRoot(folders, "root", true)).toBe(false);
    expect(canDropSidebarFolderOnRoot(folders, null, true)).toBe(false);
    expect(canDropSidebarFolderOnRoot(folders, "missing", true)).toBe(false);
  });

  it("filters folder move targets by hierarchy validity", () => {
    const folders = [
      { id: "root-a", name: "Root A" },
      { id: "root-b", name: "Root B" },
      { id: "child", name: "Child", parent_id: "root-a" },
      { id: "grandchild", name: "Grandchild", parent_id: "child" },
    ];

    expect(canMoveSidebarFolderToParent(folders, "root-a", "root-b")).toBe(true);
    expect(canMoveSidebarFolderToParent(folders, "child", "root-b")).toBe(true);
    expect(canMoveSidebarFolderToParent(folders, "root-b", "child")).toBe(true);
    expect(canMoveSidebarFolderToParent(folders, "child", "grandchild")).toBe(false);
    expect(getSidebarFolderMoveTargets(folders, "child").map((folder) => folder.id)).toEqual([
      "root-b",
    ]);
  });

  it("gates drag/drop folder moves through the same local hierarchy rules", () => {
    const folders = [
      { id: "root-a", name: "Root A" },
      { id: "root-b", name: "Root B" },
      { id: "child", name: "Child", parent_id: "root-a" },
      { id: "grandchild", name: "Grandchild", parent_id: "child" },
    ];

    expect(canDropSidebarFolderOnParent(folders, "child", "root-b", true)).toBe(true);
    expect(canDropSidebarFolderOnParent(folders, "child", "root-b", false)).toBe(false);
    expect(canDropSidebarFolderOnParent(folders, null, "root-b", true)).toBe(false);
    expect(canDropSidebarFolderOnParent(folders, "child", "grandchild", true)).toBe(false);
    expect(canDropSidebarFolderOnParent(folders, "root-a", "root-b", true)).toBe(true);
  });

  it("builds readable nested folder path labels for move targets", () => {
    const folders = [
      { id: "root", name: "Projects" },
      { id: "child", name: "Q3", parent_id: "root" },
      { id: "grandchild", name: "Launch", parent_id: "child" },
      { id: "cycle-a", name: "Cycle A", parent_id: "cycle-b" },
      { id: "cycle-b", name: "Cycle B", parent_id: "cycle-a" },
    ];

    expect(getSidebarFolderPathLabel(folders, "grandchild")).toBe("Projects / Q3 / Launch");
    expect(getSidebarFolderPathLabel(folders, "missing")).toBe("");
    expect(getSidebarFolderPathLabel(folders, "cycle-a")).toBe("");
  });

  it("validates document move targets without cycles or incompatible placement", () => {
    const documents = [
      { id: "root", title: "Root", folder_id: "folder" },
      { id: "child", title: "Child", folder_id: "folder", parent_id: "root" },
      { id: "grandchild", title: "Grandchild", folder_id: "folder", parent_id: "child" },
      { id: "other", title: "Other", folder_id: "folder" },
      { id: "outside", title: "Outside", folder_id: "other-folder" },
    ];

    expect(getSidebarDocumentDepth(documents, "root")).toBe(1);
    expect(getSidebarDocumentDepth(documents, "grandchild")).toBe(3);
    expect(isSidebarDocumentDescendant(documents, "root", "grandchild")).toBe(true);
    expect(canMoveSidebarDocumentToParent(documents, "child", "other", "folder")).toBe(true);
    expect(canMoveSidebarDocumentToParent(documents, "root", "grandchild", "folder")).toBe(false);
    expect(canMoveSidebarDocumentToParent(documents, "child", "outside", "folder")).toBe(false);
    expect(canMoveSidebarDocumentToParent(documents, "other", "grandchild", "folder")).toBe(true);
  });

  it("builds readable nested document path labels for move targets", () => {
    const folders = [{ id: "folder", name: "Projects" }];
    const documents = [
      { id: "root", title: "Root", folder_id: "folder" },
      { id: "child", title: "Child", folder_id: "folder", parent_id: "root" },
    ];

    expect(getSidebarDocumentPathLabel(folders, documents, "child")).toBe(
      "Projects / Root / Child",
    );
    expect(getSidebarDocumentPathLabel(folders, documents, "missing")).toBe("");
  });

  it("builds stable home and recent routes for the active space", () => {
    expect(getSidebarHomePath("space-1")).toBe("/spaces/space-1/docs");
    expect(getSidebarRecentPath("space-1")).toBe("/spaces/space-1/recent");
    expect(getSidebarJournalPath("space-1")).toBe("/spaces/space-1/journal");
    expect(getSidebarGraphPath("space-1")).toBe("/spaces/space-1/graph");
    expect(getSidebarWorkflowPath("space-1")).toBe("/spaces/space-1/workflows");
    expect(getSidebarStarredPath("space-1")).toBe("/spaces/space-1/starred");
    expect(getSidebarHomePath("space/one")).toBe("/spaces/space%2Fone/docs");
    expect(getSidebarRecentPath("space/one")).toBe("/spaces/space%2Fone/recent");
    expect(getSidebarJournalPath("space/one")).toBe("/spaces/space%2Fone/journal");
    expect(getSidebarGraphPath("space/one")).toBe("/spaces/space%2Fone/graph");
    expect(getSidebarWorkflowPath("space/one")).toBe("/spaces/space%2Fone/workflows");
    expect(getSidebarStarredPath("space/one")).toBe("/spaces/space%2Fone/starred");
    expect(getSidebarHomePath()).toBe("/");
    expect(getSidebarRecentPath()).toBe("/");
    expect(getSidebarJournalPath()).toBe("/");
    expect(getSidebarGraphPath()).toBe("/");
    expect(getSidebarWorkflowPath()).toBe("/");
    expect(getSidebarStarredPath()).toBe("/");
  });

  it("builds encoded document routes for sidebar document navigation", () => {
    expect(getSidebarDocumentPath("space-1", "doc-1")).toBe("/spaces/space-1/docs/doc-1");
    expect(getSidebarDocumentPath("space/one", "doc/one")).toBe(
      "/spaces/space%2Fone/docs/doc%2Fone",
    );
  });

  it("builds a stable space settings route", () => {
    expect(getSidebarSpaceSettingsPath("space-1")).toBe("/spaces/space-1/settings");
    expect(getSidebarSpaceSettingsPath("space/one")).toBe("/spaces/space%2Fone/settings");
    expect(getSidebarSpaceSettingsPath()).toBe("/");
  });

  it("uses semantic danger tokens for destructive row actions", () => {
    expect(SIDEBAR_DANGER_MENU_ITEM_CLASS).toContain("text-danger");
    expect(SIDEBAR_DANGER_MENU_ITEM_CLASS).toContain("hover:bg-danger");
    expect(SIDEBAR_DANGER_MENU_ITEM_CLASS).not.toContain("red-");
  });

  it("labels emoji editing by the Files-tree item type", () => {
    expect(getSidebarRenameEmojiLabel("document")).toBe("Page emoji");
    expect(getSidebarRenameEmojiLabel("folder")).toBe("Folder emoji");
    expect(getSidebarRenameEmojiPresets("document")).toContain("📝");
    expect(getSidebarRenameEmojiPresets("folder")).toContain("📁");
    expect(getSidebarRenameEmojiPresets("folder")).not.toContain("📝");
  });

  it("carries the entered title and destination into template selection", () => {
    expect(
      getSidebarTemplatePickerTarget({
        kind: "document",
        title: "  Project brief  ",
        query: "Personal / Projects",
        folderId: "projects",
        parentId: "parent-page",
      }),
    ).toEqual({
      title: "Project brief",
      folderId: "projects",
      parentId: "parent-page",
    });
    expect(isCurrentSidebarSpaceRequest("space-a", 1, "space-a", 2)).toBe(false);
  });

  it("keeps the desktop sidebar tree chevron lane and icon size uniform", () => {
    expect(SIDEBAR_TREE_TOGGLE_LANE_CLASS).toContain("w-4");
    expect(SIDEBAR_TREE_TOGGLE_LANE_CLASS).toContain("shrink-0");
    expect(SIDEBAR_TREE_NESTED_GROUP_CLASS).toBe("ml-4");
    expect(SIDEBAR_TREE_ICON_SIZE).toBe(14);
  });

  it("uses one icon-button group for mobile drawer search, notifications, and commands", () => {
    expect(getSidebarUtilityGroupClass(false)).toContain("starlane-control-chrome-surface");
    expect(getSidebarUtilityGroupClass(true)).toContain("starlane-control-chrome-surface");
    expect(getSidebarUtilityGroupClass(false)).toContain("gap-1");
    expect(getSidebarUtilityActionClass(false)).toContain("flex-1");
    expect(getSidebarUtilityActionClass(false, true)).toContain("h-11");
    expect(getSidebarUtilityActionClass(true)).toContain("h-9 w-full");

    const html = renderToStaticMarkup(
      <Sidebar showHeader={false} mobileSettingsMenu onOpenCommandMenu={() => undefined} />,
    );
    expect(html).toContain('role="group"');
    expect(html).toContain('aria-label="Workspace utilities"');
    expect(html).toContain('aria-label="Search documents"');
    expect(html).toContain('aria-label="Open command menu"');
    expect(html).not.toContain("<kbd");
  });

  it("keeps the desktop product lockup static without owning the collapse control", () => {
    const html = renderToStaticMarkup(<Sidebar />);

    expect(html).toContain('role="img" aria-label="Starlane Notes"');
    expect(html).not.toContain('aria-label="Open Starlane Notes home"');
    expect(html).toContain('aria-label="Home"');
    expect(html).not.toContain('aria-label="Collapse sidebar"');
    expect(html).not.toContain('aria-label="Expand sidebar"');
  });

  it("uses the Notes primary accent for active Main navigation", () => {
    expect(getSidebarNavigationItemStateClass(true)).toContain("border-primary");
    expect(getSidebarNavigationItemStateClass(true)).toContain("bg-surface-raised");
    expect(getSidebarNavigationItemStateClass(true)).toContain("text-primary");
    expect(getSidebarNavigationItemStateClass(true)).not.toContain("bg-primary/10");
    expect(getSidebarNavigationItemStateClass(false)).toContain("border-transparent");
  });

  it("does not announce disabled space destinations as current before a space is selected", () => {
    expect(getSidebarNavigationAriaCurrent(true)).toBe("page");
    expect(getSidebarNavigationAriaCurrent(false)).toBeUndefined();

    const html = renderToStaticMarkup(<Sidebar />);
    expect(html).not.toContain('aria-current="page"');
    expect(html).toMatch(/disabled=""[^>]*aria-label="Home"/);
    expect(html).toMatch(/disabled=""[^>]*aria-label="Recent"/);
  });

  it("announces Home alone for an actual selected-space render", () => {
    navigationFixture.selected = true;
    navigationFixture.pathname = "/spaces/space-1/docs";
    try {
      const html = renderToStaticMarkup(<Sidebar />);
      expect(html.match(/aria-current="page"/g)).toHaveLength(1);
      expect(html).toMatch(/aria-label="Home"[^>]*aria-current="page"/);
    } finally {
      navigationFixture.selected = false;
      navigationFixture.pathname = "/";
    }
  });

  it("announces Shared with me independently without inventing a selected space", () => {
    navigationFixture.mode = "server";
    navigationFixture.pathname = "/shared-with-me";
    try {
      const html = renderToStaticMarkup(<Sidebar />);
      expect(html.match(/aria-current="page"/g)).toHaveLength(1);
      expect(html).toMatch(/aria-label="Shared with me"[^>]*aria-current="page"/);
      expect(html).toMatch(/disabled=""[^>]*aria-label="Home"/);
    } finally {
      navigationFixture.mode = "local";
      navigationFixture.pathname = "/";
    }
  });

  it("uses the Notes blue ramp for Main and Files icons", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source.replace(/\s+/g, " ")).toContain(
      '<Search size={collapsed ? 15 : 14} className="sidebar-icon-blue" />',
    );
    expect(source.replace(/\s+/g, " ")).toContain('iconClassName="sidebar-icon-blue-mid"');
    expect(source.replace(/\s+/g, " ")).toContain(
      '<Home size={14} className="sidebar-icon-blue shrink-0" />',
    );
    expect(source.replace(/\s+/g, " ")).toContain(
      '<Clock3 size={14} className="sidebar-icon-blue-mid shrink-0" />',
    );
    expect(source.replace(/\s+/g, " ")).toContain(
      '<Star size={14} className="sidebar-icon-blue-deep shrink-0" />',
    );
    expect(source.replace(/\s+/g, " ")).toContain(
      '<CalendarDays size={14} className="sidebar-icon-blue-soft" />',
    );
    expect(source.replace(/\s+/g, " ")).toContain(
      '<Folder size={14} className="sidebar-icon-blue" />',
    );
    expect(source.replace(/\s+/g, " ")).toContain(
      '<Plus size={14} className="sidebar-icon-blue-deep" />',
    );
    expect(source.replace(/\s+/g, " ")).toContain(
      '<FolderPlus size={14} className="sidebar-icon-blue-mid" />',
    );
  });

  it("keeps account destinations in the mobile drawer and removes sidebar About", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain('aria-label="Open account menu"');
    expect(source).not.toContain("About Starlane Notes");
    expect(source).toContain('aria-label="Sidebar footer"');
  });

  it("uses in-app sidebar delete confirmation copy for pages and folders", () => {
    expect(getSidebarDeleteConfirmationTitle("document")).toBe("Delete page");
    expect(getSidebarDeleteConfirmationTitle("folder")).toBe("Delete folder");
    expect(getSidebarDeleteConfirmationMessage("document", "Launch Plan")).toBe(
      'Move page "Launch Plan" to Trash? You can restore it from Space settings.',
    );
    expect(getSidebarDeleteConfirmationMessage("document", "   ")).toBe(
      "Move this page to Trash? You can restore it from Space settings.",
    );
    expect(getSidebarDeleteConfirmationMessage("folder", "Projects")).toBe(
      'Delete folder "Projects" and its pages? This cannot be undone.',
    );
    expect(getSidebarDeleteConfirmationMessage("folder", "   ")).toBe(
      "Delete this folder and its pages? This cannot be undone.",
    );
  });

  it("matches sidebar routes without being confused by trailing slashes", () => {
    expect(isSidebarRouteActive("/spaces/space-1/recent/", "/spaces/space-1/recent")).toBe(true);
    expect(
      isSidebarRouteActive("/spaces/space%2Fone/graph", getSidebarGraphPath("space/one")),
    ).toBe(true);
    expect(isSidebarRouteActive("/spaces/space-1/docs/doc-1", "/spaces/space-1/docs")).toBe(false);
  });

  it("keeps desktop navigation and footer controls in order without duplicate top-bar owners", () => {
    const html = renderToStaticMarkup(<Sidebar />);

    expect(html).not.toContain("Open account menu");
    expect(html).not.toContain(">Account and workspace<");
    expect(html).toContain("Starlane Notes");
    expect(html).toContain('role="img" aria-label="Starlane Notes"');
    expect(html).not.toContain("Open Starlane Notes home");
    expect(html).toContain("Search documents");
    expect(html).not.toContain('aria-label="Workspace utilities"');
    expect(html).toContain('aria-label="Select space"');
    expect(html).toContain("No spaces");
    expect(html).not.toContain("Search spaces");
    expect(html).toContain("flex h-full min-h-0 flex-col");
    expect(html).toContain("flex shrink-0 border-b p-2");
    expect(html).toContain("min-h-0 flex-1 overflow-y-auto");
    expect(html).toContain("env(safe-area-inset-bottom)");
    expect(html.indexOf("Starlane Notes")).toBeLessThan(html.indexOf("Home"));
    expect(html.indexOf("Home")).toBeLessThan(html.indexOf("Recent"));
    expect(html.indexOf("Recent")).toBeLessThan(html.indexOf("Favorites"));
    expect(html.indexOf("Favorites")).toBeLessThan(html.indexOf("Journal"));
    expect(html).not.toContain('aria-label="Graph"');
    expect(html.indexOf("Journal")).toBeLessThan(html.indexOf("Workflows"));
    expect(html.indexOf("Workflows")).toBeLessThan(html.indexOf("Pages"));
    expect(html).toContain('role="tree"');
    expect(html).toContain('aria-label="Pages and folders"');
    expect(html).toContain('id="sidebar-files-heading"');
    expect(html).toContain('tabindex="-1"');
    expect(html).toContain('aria-label="New page"');
    expect(html).toContain('aria-label="New folder"');
    expect(html).not.toContain('aria-label="Daily note"');
  });

  it("keeps labelled navigation controls while compact and hides the file tree", () => {
    const html = renderToStaticMarkup(<Sidebar collapsed onShowFiles={() => undefined} />);

    expect(html).toContain('aria-label="Home"');
    expect(html).toContain('aria-label="Select space"');
    expect(html).toContain('title="Select space: No spaces"');
    expect(html).toContain('title="Starlane Notes"');
    expect(html).not.toContain('title="Starlane Notes home"');
    expect(html).toContain('title="Home"');
    expect(html).toContain('title="Recent"');
    expect(html).toContain('title="Favorites"');
    expect(html).toContain('title="Journal"');
    expect(html).not.toContain('title="Graph"');
    expect(html).not.toContain('aria-label="Graph"');
    expect(html).toContain('title="Workflows"');
    expect(html).toContain('aria-label="Show pages and folders"');
    expect(html).toContain('title="Show pages and folders"');
    expect(html).toContain(SIDEBAR_COMPACT_FILES_REVEAL_CLASS);
    expect(html).not.toContain('title="Account"');
    expect(html).not.toContain('aria-label="Expand sidebar"');
    expect(html).not.toContain('aria-label="Daily note"');
    expect(html).not.toContain('title="Daily note"');
    expect(html).not.toContain(">Pages<");
    expect(html).not.toContain('aria-label="New page"');
    expect(html).not.toContain('aria-label="New folder"');
  });

  it("keeps Files creation compact on desktop and touch-safe in the mobile drawer", () => {
    expect(getSidebarFilesCreateActionClass(false)).toContain("h-7 w-7");
    expect(getSidebarFilesCreateActionClass(true)).toContain("h-11 w-11");
  });

  it("keeps Main navigation and Files-tree controls touch-safe in the mobile drawer", () => {
    expect(getSidebarNavigationTargetClass(false)).toBe("py-1.5");
    expect(getSidebarNavigationTargetClass(true)).toContain("min-h-11");
    expect(getSidebarTreeRowClass(false)).toContain("pr-8");
    expect(getSidebarTreeRowClass(true)).toContain("min-h-11");
    expect(getSidebarTreeToggleClass(false)).toContain("h-7 w-4");
    expect(getSidebarTreeToggleClass(true)).toContain("h-11 w-9");
    expect(getSidebarTreeActionClass(false)).toContain("h-7 w-7");
    expect(getSidebarTreeActionClass(true)).toContain("h-11 w-11");
  });

  it("keeps account menu rows touch-safe in the mobile drawer without enlarging desktop menus", () => {
    expect(getSidebarAccountMenuItemClass(false)).toContain("starlane-menu-item");
    expect(getSidebarAccountMenuItemClass(false)).not.toContain("min-h-11");
    expect(getSidebarAccountMenuItemClass(true)).toContain("min-h-11");
    expect(getSidebarAccountMenuItemClass(true, true)).toContain("text-danger");
  });

  it("keeps desktop identity, Main navigation, and Files in order", () => {
    const html = renderToStaticMarkup(<Sidebar />);

    expect(html.indexOf('aria-label="Starlane Notes"')).toBeLessThan(
      html.indexOf('aria-label="Home"'),
    );
    expect(html).toContain('aria-label="Main navigation"');
    expect(html.indexOf(">Main<")).toBeLessThan(html.indexOf('aria-label="Home"'));
    expect(html.indexOf('aria-label="Home"')).toBeLessThan(html.indexOf(">Pages<"));
    expect(html).not.toContain("Open account menu");
    expect(html).toContain("Search documents");
    expect(html).toContain("No spaces");
    expect(html).not.toContain("uppercase");
    expect(html).not.toContain("tracking-wide");
  });

  it("keeps mobile work utilities ordered and account controls in the mobile footer", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain("<AppearanceToggle touchTarget />");
    expect(source).toContain("getSidebarUtilityActionClass(collapsed, mobileSettingsMenu)");
    expect(source).toContain('className="sidebar-icon-blue"');
    expect(source).toContain('iconClassName="sidebar-icon-blue-mid"');
    expect(source).toContain('className="sidebar-icon-blue-deep"');
    expect(source).toContain('title={collapsed ? "Account" : undefined}');
    expect(source.indexOf('aria-label="Search documents"')).toBeLessThan(
      source.indexOf("<NotificationBell"),
    );
    expect(source.indexOf("<NotificationBell")).toBeLessThan(
      source.indexOf('aria-label="Open command menu"'),
    );
    expect(source.indexOf('aria-label="Open command menu"')).toBeLessThan(
      source.indexOf('aria-label="Sidebar footer"'),
    );
    expect(source.indexOf('aria-label="Sidebar footer"')).toBeLessThan(
      source.indexOf("<AppearanceToggle touchTarget />"),
    );
  });

  it("keeps Main navigation labelled for assistive technology without widening the compact rail", () => {
    const html = renderToStaticMarkup(<Sidebar collapsed />);

    expect(html).toContain('aria-label="Main navigation"');
    expect(html).not.toContain(">Main<");
    expect(html).not.toContain(">More<");
    expect(html).toContain('aria-label="Space Settings"');
  });

  it("uses one divider between Main navigation and the Files tree", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain('className="border-border/60 mb-2 space-y-0.5 border-b pb-2"');
    expect(source).toContain("className={`pt-2 transition-colors duration-100");
    expect(source).not.toContain(
      "className={`border-border/60 border-t pt-2 transition-colors duration-100",
    );
  });

  it("can hide its brand header when the mobile drawer owns the drawer chrome", () => {
    const html = renderToStaticMarkup(
      <Sidebar showHeader={false} mobileSettingsMenu onOpenCommandMenu={() => undefined} />,
    );

    expect(html).not.toContain("Open Starlane Notes home");
    expect(html).not.toContain("<span>Starlane Notes</span>");
    expect(html).toContain('aria-label="Workspace utilities"');
    expect(html).toContain('aria-label="Search documents"');
    expect(html).not.toContain("Notification bell");
    expect(html).toContain('aria-label="Open command menu"');
    expect(html).toContain('aria-label="Sidebar footer"');
    expect(html).toContain('aria-label="Appearance: system. Switch appearance"');
    expect(html).toContain("Home");
    expect(html).toContain("Recent");
    expect(html).toContain('aria-label="New page"');
    expect(html).toContain('aria-label="New folder"');
    expect(html).not.toContain('aria-label="Graph"');
    expect(html).toContain('aria-label="Workflows"');
    expect(html).toContain("Open account menu");
    expect(html).toContain('aria-label="Storage Administration"');
    expect(html).toContain("No spaces");
  });

  it("uses the same account menu trigger in the mobile drawer footer", () => {
    const html = renderToStaticMarkup(<Sidebar showHeader={false} mobileSettingsMenu />);

    expect(html).toContain('aria-label="Open account menu"');
    expect(html).not.toContain('aria-label="Open space settings"');
    expect(html).toContain("min-h-11");
    expect(html).toContain('aria-label="Appearance: system. Switch appearance"');
  });

  it("keeps account and administration access in compact native desktop drawers", () => {
    runtimeFixture.runtime = "tauri-desktop";
    try {
      const html = renderToStaticMarkup(<Sidebar showHeader={false} mobileSettingsMenu />);

      expect(html).toContain('aria-label="Open account menu"');
      expect(html).toContain('aria-label="Storage Administration"');
      expect(html).toContain("min-h-11");
    } finally {
      runtimeFixture.runtime = "web";
    }
  });

  it("gives page and folder action menus their own labelled dismissal contract", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );
    const html = renderToStaticMarkup(
      <RowActionMenu
        open
        onOpenChange={() => undefined}
        label="Folder actions for Roadmap"
        trigger={<button type="button">Actions</button>}
        items={[{ label: "Rename", icon: <span>R</span>, onSelect: () => undefined }]}
      />,
    );

    expect(source).toContain("<Popover open={open} onOpenChange={onOpenChange}>");
    expect(source).toContain('role="menu"');
    expect(source).toContain("aria-label={label}");
    expect(source).toContain('role="menuitem"');
    expect(source).toContain("useMenuKeyboardNavigation(menuRef)");
    expect(html).toContain('aria-haspopup="dialog"');
    expect(html).not.toContain("top-6");
  });

  it("gives Files action menus the same Arrow/Home/End keyboard navigation as the account menu", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );

    expect(source).toContain("useMenuKeyboardNavigation(menuRef)");
    expect(source).toContain("onKeyDown={handleMenuKeyDown}");
  });

  it("positions mobile file action menus below their touch-safe trigger", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/Sidebar.tsx"),
      "utf8",
    );
    const html = renderToStaticMarkup(
      <RowActionMenu
        open
        onOpenChange={() => undefined}
        label="Folder actions for Roadmap"
        trigger={<button type="button">Actions</button>}
        touchActions
        items={[{ label: "Rename", icon: <span>R</span>, onSelect: () => undefined }]}
      />,
    );

    expect(source).toContain("sideOffset={4}");
    expect(source).toContain('touchActions ? "mt-1" : ""');
    expect(html).toContain('aria-haspopup="dialog"');
  });

  it("routes profile settings through the shared profile page instead of the mobile-only path", () => {
    expect(SIDEBAR_PROFILE_SETTINGS_PATH).toBe("/profile");
    expect(SIDEBAR_PROFILE_SETTINGS_PATH).not.toBe("/mobile/profile");
  });

  it("dispatches document move requests for the shared sidebar move dialog", () => {
    const listener = vi.fn();
    const eventTarget = new EventTarget();
    vi.stubGlobal("document", eventTarget);

    eventTarget.addEventListener(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, listener);

    openSidebarDocumentMoveDialog("doc-1");

    expect(listener).toHaveBeenCalledTimes(1);
    expect((listener.mock.calls[0][0] as CustomEvent).detail).toEqual({ documentId: "doc-1" });
    eventTarget.removeEventListener(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, listener);
    vi.unstubAllGlobals();
  });
});

describe("Local reviewed move completion", () => {
  it("refreshes with original checks before finishing a confirmed move", async () => {
    const verifyCurrent = vi.fn();
    const refresh = vi.fn(async (verify) => {
      verify();
    });
    const finish = vi.fn();
    await completeLocalSidebarMove({ verifyCurrent } as never, () => true, false, refresh, finish);
    expect(refresh).toHaveBeenCalledOnce();
    expect(verifyCurrent).toHaveBeenCalledTimes(3);
    expect(finish).toHaveBeenCalledWith(false);
  });
  it("does not refresh or navigate after the original selection is lost", async () => {
    const refresh = vi.fn();
    const finish = vi.fn();
    await expect(
      completeLocalSidebarMove(
        { verifyCurrent: vi.fn() } as never,
        () => false,
        false,
        refresh,
        finish,
      ),
    ).rejects.toThrow("selection changed");
    expect(refresh).not.toHaveBeenCalled();
    expect(finish).not.toHaveBeenCalled();
  });
  it("suppresses completion if selection changes during list refresh", async () => {
    let selected = true;
    const finish = vi.fn();
    await expect(
      completeLocalSidebarMove(
        { verifyCurrent: vi.fn() } as never,
        () => selected,
        false,
        async () => {
          selected = false;
        },
        finish,
      ),
    ).rejects.toThrow("selection changed");
    expect(finish).not.toHaveBeenCalled();
  });
  it("keeps the current canvas when the writer retained newer local edits", async () => {
    const finish = vi.fn();
    await completeLocalSidebarMove(
      { verifyCurrent: vi.fn() } as never,
      () => true,
      true,
      async (verify) => {
        verify();
      },
      finish,
    );
    expect(finish).toHaveBeenCalledWith(true);
  });
});

describe("current account kind administration ceiling", () => {
  it("suppresses known guest and malformed kinds regardless of retained admin role", () => {
    expect(shouldShowSidebarSiteAdministration(false, "admin", "guest")).toBe(false);
    expect(shouldShowSidebarSiteAdministration(false, "admin", null)).toBe(false);
    expect(shouldShowSidebarSiteAdministration(false, "admin", "unknown")).toBe(false);
    expect(shouldShowSidebarSiteAdministration(false, "admin", "member")).toBe(true);
    expect(shouldShowSidebarSiteAdministration(false, "admin")).toBe(true);
  });
});

describe("deep folder navigation without a product ceiling", () => {
  it.each([4, 32, 256])(
    "keeps depth %i folders available for create/move/path/active expansion",
    (depth) => {
      const folders = Array.from({ length: depth }, (_, i) => ({
        id: `deep-${i}`,
        name: `Level ${i}`,
        parent_id: i ? `deep-${i - 1}` : undefined,
      }));
      const before = JSON.stringify(folders),
        last = folders[depth - 1];
      expect(getSidebarFolderDepth(folders, last.id)).toBe(depth);
      expect(canCreateSidebarChildFolder(folders, last.id)).toBe(true);
      expect(getSidebarFolderPathLabel(folders, last.id).split(" / ")).toHaveLength(depth);
      expect(
        getSidebarActiveFolderIds(
          folders,
          [{ id: "routed-page", title: "Page", folder_id: last.id }],
          "/spaces/s/docs/routed-page",
        ),
      ).toHaveLength(depth);
      const other = { id: "other-root", name: "Other" };
      expect(canMoveSidebarFolderToParent([...folders, other], other.id, last.id)).toBe(true);
      expect(canMoveSidebarFolderToParent(folders, folders[0].id, last.id)).toBe(false);
      expect(
        canMoveSidebarFolderAcrossSpaces(
          folders,
          folders[0].id,
          [{ id: "target", name: "Target" }],
          "target",
        ),
      ).toBe(true);
      expect(JSON.stringify(folders)).toBe(before);
    },
  );
  it("does not charge unrelated broad workspace rows to a small operation", () => {
    const folders = [
      { id: "selected", name: "Selected" },
      ...Array.from({ length: 10_001 }, (_, i) => ({ id: `unrelated-${i}`, name: `Other ${i}` })),
    ];
    expect(canCreateSidebarChildFolder(folders, "selected")).toBe(true);
    expect(canMoveSidebarFolderToParent(folders, "selected", "unrelated-0")).toBe(true);
  });
  it("refuses orphan, duplicate and foreign ancestry without changing input", () => {
    for (const folders of [
      [{ id: "selected", name: "Selected", parent_id: "missing" }],
      [
        { id: "selected", name: "Selected" },
        { id: "selected", name: "Duplicate" },
      ],
      [
        { id: "selected", name: "Selected", space_id: "a", parent_id: "foreign" },
        { id: "foreign", name: "Foreign", space_id: "b" },
      ],
    ]) {
      const before = JSON.stringify(folders);
      expect(canCreateSidebarChildFolder(folders, "selected")).toBe(false);
      expect(getSidebarFolderDepth(folders, "selected")).toBe(0);
      expect(JSON.stringify(folders)).toBe(before);
    }
  });
});

describe("shared folder render context", () => {
  it("bounds destination display labels while preserving exact deep target identity and the full path helper", () => {
    const folders = Array.from({ length: 256 }, (_, n) => ({
      id: `long-${n}`,
      name: `${"Ancestor ".repeat(8)}${n}`,
      parent_id: n ? `long-${n - 1}` : undefined,
    }));
    folders[255].name = "Distinct selected deepest target";
    const before = JSON.stringify(folders);
    const catalog = createSidebarFolderCatalog(folders);
    const label = catalog.labels.get("long-255")!;
    expect(label.startsWith("…")).toBe(true);
    expect(label.endsWith("Distinct selected deepest target")).toBe(true);
    expect(label.length).toBeLessThanOrEqual(2048);
    expect(catalog.index.byId.get("long-255")?.parent_id).toBe("long-254");
    expect(getSidebarFolderPathLabel(folders, "long-255").startsWith(folders[0].name)).toBe(true);
    expect(JSON.stringify(folders)).toBe(before);
  });
});

describe("deep child-page navigation", () => {
  it.each([4, 32, 256])(
    "retains exact parents, selectable depth and full ancestry at %i levels",
    (depth) => {
      const rows = Array.from({ length: depth }, (_, i) => ({
        id: `p${i}`,
        title: `Page ${i}`,
        folder_id: "folder",
        parent_id: i ? `p${i - 1}` : undefined,
      }));
      const last = rows[depth - 1];
      const catalog = createSidebarPageCatalog(rows);
      expect(catalog.depths.get(last.id)).toBe(depth);
      expect(catalog.roots.map((row) => row.id)).toEqual(["p0"]);
      expect(catalog.children.get(`p${depth - 2}`)?.map((row) => row.id)).toEqual([last.id]);
      expect(getSidebarDocumentDepth(rows, last.id)).toBe(depth);
      expect(getSidebarDocumentPathLabel([{ id: "folder", name: "Folder" }], rows, last.id)).toBe(
        ["Folder", ...rows.map((row) => row.title)].join(" / "),
      );
      expect(
        getSidebarCreateLocationOptions(
          "Space",
          [{ id: "folder", name: "Folder" }],
          rows,
          "document",
        ).find((row) => row.id === `document:${last.id}`),
      ).toMatchObject({ parentId: last.id, folderId: "folder" });
      expect(
        canMoveSidebarDocumentToParent(
          [...rows, { id: "other", title: "Other", folder_id: "folder" }],
          "other",
          last.id,
          "folder",
        ),
      ).toBe(true);
      expect(canMoveSidebarDocumentToParent(rows, "p0", last.id, "folder")).toBe(false);
    },
  );
  it("refuses missing/private-unlisted, cyclic, foreign and different-folder ancestors", () => {
    const invalid = [
      { id: "hidden-child", title: "Unavailable parent", parent_id: "private-not-listed" },
      { id: "cycle-a", title: "A", parent_id: "cycle-b" },
      { id: "cycle-b", title: "B", parent_id: "cycle-a" },
      { id: "foreign", title: "Foreign", space_id: "elsewhere" },
      { id: "foreign-child", title: "Child", parent_id: "foreign" },
      { id: "folder-parent", title: "Folder parent", folder_id: "one" },
      { id: "folder-child", title: "Child", folder_id: "two", parent_id: "folder-parent" },
    ];
    const options = getSidebarCreateLocationOptions("Space", [], invalid, "document");
    for (const id of [
      "hidden-child",
      "cycle-a",
      "cycle-b",
      "foreign",
      "foreign-child",
      "folder-child",
    ])
      expect(options.some((row) => row.parentId === id)).toBe(false);
    expect(getSidebarDocumentDepth(invalid, "hidden-child")).toBe(0);
    expect(
      canMoveSidebarDocumentToParent(
        [...invalid, { id: "new", title: "New" }],
        "new",
        "hidden-child",
      ),
    ).toBe(false);
  });
  it("indexes wide page catalogs without walking each sibling tree", () => {
    const rows = Array.from({ length: 10001 }, (_, i) => ({ id: `p${i}`, title: `Page ${i}` }));
    const catalog = createSidebarPageCatalog(rows);
    expect(catalog.roots).toHaveLength(10001);
    expect(catalog.children.size).toBe(0);
    expect(catalog.depths.get("p10000")).toBe(1);
  });
});

describe("deep page location identity and disclosure", () => {
  it("bounds long picker labels while preserving the exact deep destination identity", () => {
    const rows = Array.from({ length: 256 }, (_, i) => ({
      id: `page-${i}`,
      title: `${"Long page title ".repeat(4)}${i}`,
      parent_id: i ? `page-${i - 1}` : undefined,
    }));
    const option = getSidebarCreateLocationOptions("Space", [], rows, "document").find(
      (row) => row.parentId === "page-255",
    );
    expect(option).toMatchObject({ id: "document:page-255", parentId: "page-255", folderId: null });
    expect(option?.label.length).toBeLessThanOrEqual(2056);
    expect(option?.label).toContain("…");
    expect(option?.label.endsWith(rows[255].title)).toBe(true);
    expect(getSidebarDocumentPathLabel([], rows, "page-255")).toBe(
      rows.map((row) => row.title).join(" / "),
    );
  });
  it("expands only the exact routed page's valid same-space folder ancestry", () => {
    const folders = [
      { id: "one", name: "One", space_id: "space" },
      { id: "two", name: "Two", space_id: "space" },
    ];
    const rows = [
      { id: "page-1", title: "One", space_id: "space", folder_id: "one" },
      { id: "page-10", title: "Ten", space_id: "space", folder_id: "two" },
    ];
    expect(getSidebarActiveFolderIds(folders, rows, "/spaces/space/docs/page-10")).toEqual(["two"]);
    expect(
      getSidebarActiveFolderIds(
        folders,
        [
          ...rows,
          {
            id: "hidden-child",
            title: "Hidden",
            space_id: "space",
            folder_id: "one",
            parent_id: "unlisted-private",
          },
        ],
        "/spaces/space/docs/hidden-child",
      ),
    ).toEqual([]);
  });
});

describe("desktop pending page creation", () => {
  it.each(["rejected", "unconfirmed"])("retains the exact title/destination after %s creation and exposes a failure message", async outcome => {
    const original = { title: "  Retained title  ", folderId: "folder-32", parentId: "parent-256", query: "Space / Folder / Parent" };
    let dialog: typeof original | null = original, error = "";
    const create = vi.fn(async () => { if (outcome === "rejected") throw new Error("Disk quota"); return false; });
    const result = await settleSidebarPageCreation(create, () => { dialog = null; }, message => { error = message; });
    expect(result).toBe(false); expect(create).toHaveBeenCalledOnce();
    expect(dialog).toBe(original); expect(dialog).toEqual(original);
    expect(error).toContain("creation was not confirmed");
    expect(error).toContain("title and destination are kept");
  });
  it("keeps pending input until the create promise confirms success", async () => {
    let confirm!: (value: boolean) => void;
    const create = new Promise<boolean>(resolve => { confirm = resolve; });
    const close = vi.fn(), failed = vi.fn();
    const pending = settleSidebarPageCreation(() => create, close, failed);
    expect(close).not.toHaveBeenCalled(); expect(failed).not.toHaveBeenCalled();
    confirm(true);
    expect(await pending).toBe(true); expect(close).toHaveBeenCalledOnce(); expect(failed).not.toHaveBeenCalled();
  });
});
