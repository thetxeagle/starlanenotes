import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import {
  canCreateSidebarChildFolder,
  canDropSidebarFolderOnParent,
  canDropSidebarFolderOnRoot,
  canMoveSidebarDocumentToParent,
  canMoveSidebarFolderToRoot,
  canMoveSidebarFolderToParent,
  getSidebarDocumentDepth,
  getSidebarDocumentPathLabel,
  getSidebarDocumentPath,
  getSidebarDeleteConfirmationMessage,
  getSidebarDeleteConfirmationTitle,
  getSidebarFolderMoveTargets,
  getSidebarFolderPathLabel,
  getSidebarFolderDepth,
  getSidebarGraphPath,
  getSidebarHomePath,
  getSidebarJournalPath,
  getSidebarRecentPath,
  getSidebarSpaceSettingsPath,
  getSidebarStarredPath,
  getSidebarWorkflowPath,
  isSidebarRouteActive,
  isSidebarDocumentDescendant,
  OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT,
  openSidebarDocumentMoveDialog,
  SIDEBAR_DANGER_MENU_ITEM_CLASS,
  SIDEBAR_PROFILE_SETTINGS_PATH,
  SIDEBAR_SITE_ADMINISTRATION_PATH,
  SIDEBAR_TREE_ICON_SIZE,
  SIDEBAR_TREE_TOGGLE_LANE_CLASS,
  Sidebar,
  shouldShowSidebarSiteAdministration,
  sortSidebarDocuments,
  sortSidebarFolders,
} from "./Sidebar";

vi.mock("react-router-dom", () => ({
  useNavigate: () => vi.fn(),
  useLocation: () => ({ pathname: "/" }),
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
  getCurrentLaunchMode: () => "local",
}));

vi.mock("@/components/BrandLogo", () => ({
  BrandLogo: () => <span>Starlane Notes</span>,
}));

vi.mock("@/lib/starredDocuments", () => ({
  loadStarredDocuments: vi.fn(async () => []),
  readStarredDocuments: vi.fn(() => []),
}));

describe("Sidebar ordering", () => {
  it("only exposes Site Administration to server-connected administrators", () => {
    expect(SIDEBAR_SITE_ADMINISTRATION_PATH).toBe("/admin");
    expect(shouldShowSidebarSiteAdministration(false, "admin")).toBe(true);
    expect(shouldShowSidebarSiteAdministration(false, "member")).toBe(false);
    expect(shouldShowSidebarSiteAdministration(true, "admin")).toBe(false);
    expect(shouldShowSidebarSiteAdministration(false, undefined)).toBe(false);
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
    expect(canCreateSidebarChildFolder(folders, "grandchild")).toBe(false);
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

    expect(canMoveSidebarFolderToParent(folders, "root-a", "root-b")).toBe(false);
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
    expect(canDropSidebarFolderOnParent(folders, "root-a", "root-b", true)).toBe(false);
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
    expect(getSidebarFolderPathLabel(folders, "cycle-a")).toBe("Cycle B / Cycle A");
  });

  it("validates document move targets without cycles or excess depth", () => {
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
    expect(canMoveSidebarDocumentToParent(documents, "other", "grandchild", "folder")).toBe(false);
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

  it("keeps the desktop sidebar tree chevron lane and icon size uniform", () => {
    expect(SIDEBAR_TREE_TOGGLE_LANE_CLASS).toContain("w-5");
    expect(SIDEBAR_TREE_TOGGLE_LANE_CLASS).toContain("shrink-0");
    expect(SIDEBAR_TREE_ICON_SIZE).toBe(14);
  });

  it("uses in-app sidebar delete confirmation copy for pages and folders", () => {
    expect(getSidebarDeleteConfirmationTitle("document")).toBe("Delete page");
    expect(getSidebarDeleteConfirmationTitle("folder")).toBe("Delete folder");
    expect(getSidebarDeleteConfirmationMessage("document", "Launch Plan")).toBe(
      'Delete page "Launch Plan"? This cannot be undone.',
    );
    expect(getSidebarDeleteConfirmationMessage("document", "   ")).toBe(
      "Delete this page? This cannot be undone.",
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

  it("keeps shared navigation and footer controls in order", () => {
    const html = renderToStaticMarkup(<Sidebar />);

    expect(html).toContain("Open space settings");
    expect(html).toContain("Starlane Notes");
    expect(html).toContain("Open Starlane Notes home");
    expect(html).toContain("Search documents");
    expect(html).not.toContain("Search spaces");
    expect(html).toContain("flex h-full min-h-0 flex-col");
    expect(html).toContain("flex shrink-0 items-center justify-between");
    expect(html).toContain("min-h-0 flex-1 overflow-y-auto");
    expect(html).toContain("env(safe-area-inset-bottom)");
    expect(html.indexOf("Starlane Notes")).toBeLessThan(html.indexOf("Home"));
    expect(html.indexOf("Search documents")).toBeLessThan(html.indexOf("Home"));
    expect(html.indexOf("Home")).toBeLessThan(html.indexOf("Recent"));
    expect(html.indexOf("Recent")).toBeLessThan(html.indexOf("Journal"));
    expect(html.indexOf("Journal")).toBeLessThan(html.indexOf("Graph"));
    expect(html.indexOf("Graph")).toBeLessThan(html.indexOf("Workflows"));
    expect(html.indexOf("New Doc")).toBeLessThan(html.indexOf("Open space settings"));
  });

  it("can hide its brand header when the mobile drawer owns the drawer chrome", () => {
    const html = renderToStaticMarkup(<Sidebar showHeader={false} />);

    expect(html).not.toContain("Open Starlane Notes home");
    expect(html).not.toContain("Search documents");
    expect(html).not.toContain("Starlane Notes");
    expect(html).toContain("Home");
    expect(html).toContain("Recent");
    expect(html).toContain("New Doc");
    expect(html).toContain("Open space settings");
  });

  it("uses a settings menu trigger for the mobile drawer footer", () => {
    const html = renderToStaticMarkup(<Sidebar showHeader={false} mobileSettingsMenu />);

    expect(html).toContain('aria-label="Open settings menu"');
    expect(html).not.toContain('aria-label="Open space settings"');
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
