import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  getRightPanelAriaLabel,
  getRightPanelTabToggleState,
  shouldCloseRightPanelFromPointer,
  getWorkspaceDestination,
  isAdminRoute,
  isDocumentPanelRoute,
  isWorkspaceDestinationRoute,
} from "./AppLayout";
import {
  canUseMobileRightPanel,
  MOBILE_SEARCH_PATH,
} from "./MobileAppLayout";
import { getSidebarSpaceSettingsPath } from "./Sidebar";

describe("document right-panel route scoping", () => {
  const documentRoutes = [
    "/spaces/space-1/docs/doc-1",
    "/spaces/space-1/docs/doc-1/",
  ];

  const unrelatedRoutes = [
    "/",
    "/admin",
    "/profile",
    "/spaces/space-1/docs",
    "/spaces/space-1/recent",
    "/spaces/space-1/journal",
    "/spaces/space-1/graph",
    "/spaces/space-1/starred",
    "/spaces/space-1/workflows",
    "/mobile/recent",
    "/mobile/search",
    "/mobile/profile",
  ];

  it("allows the desktop properties panel only on document pages", () => {
    documentRoutes.forEach((route) => {
      expect(isDocumentPanelRoute(route)).toBe(true);
    });

    unrelatedRoutes.forEach((route) => {
      expect(isDocumentPanelRoute(route)).toBe(false);
    });
  });

  it("keeps mobile properties panel availability aligned with document pages", () => {
    documentRoutes.forEach((route) => {
      expect(canUseMobileRightPanel(route)).toBe(true);
    });

    unrelatedRoutes.forEach((route) => {
      expect(canUseMobileRightPanel(route)).toBe(false);
    });
  });

  it("toggles only the requested document panel tab", () => {
    expect(getRightPanelTabToggleState(false, "properties", "comments")).toEqual({
      open: true,
      activeTab: "comments",
    });
    expect(getRightPanelTabToggleState(true, "comments", "properties")).toEqual({
      open: true,
      activeTab: "properties",
    });
    expect(getRightPanelTabToggleState(true, "properties", "properties")).toEqual({
      open: false,
      activeTab: "properties",
    });
  });

  it("labels the active document panel surface", () => {
    expect(getRightPanelAriaLabel("comments")).toBe("Comments panel");
    expect(getRightPanelAriaLabel("properties")).toBe("Properties panel");
    expect(getRightPanelAriaLabel("history")).toBe("Document panel");
  });

  it("closes an open panel when the editor surface is clicked, but not its trigger", () => {
    const editorNode = {
      closest: (selector: string) =>
        selector === "[data-document-editor-surface]" ? {} : null,
    };
    const triggerNode = {
      closest: (selector: string) =>
        selector === "[data-right-panel-trigger]" ? {} : editorNode.closest(selector),
    };

    expect(shouldCloseRightPanelFromPointer(editorNode as unknown as EventTarget)).toBe(true);
    expect(shouldCloseRightPanelFromPointer(triggerNode as unknown as EventTarget)).toBe(false);
  });
});

describe("admin navigation route scoping", () => {
  it("uses the admin shell only for the Site Administration route", () => {
    expect(isAdminRoute("/admin")).toBe(true);
    expect(isAdminRoute("/admin/")).toBe(true);
    expect(isAdminRoute("/")).toBe(false);
    expect(isAdminRoute("/profile")).toBe(false);
    expect(isAdminRoute("/spaces/space-1/docs/doc-1")).toBe(false);
  });
});

describe("workspace destination route scoping", () => {
  it("keeps account and administration outside workspace chrome", () => {
    ["/admin", "/profile", "/profile/security"].forEach((route) => {
      expect(isWorkspaceDestinationRoute(route)).toBe(true);
    });
    expect(isWorkspaceDestinationRoute("/spaces/space-1/docs/doc-1")).toBe(false);
    expect(isWorkspaceDestinationRoute("/spaces/space-1/settings")).toBe(false);
  });

  it("names each destination and preserves an encoded parent-space return path", () => {
    expect(getWorkspaceDestination("/profile")).toEqual({
      title: "Account Settings",
      backHref: "/",
      backLabel: "Back to Notes",
    });
    expect(getWorkspaceDestination("/spaces/space%2Fone/settings", "space/one")).toBeNull();
  });
});

describe("mobile drawer destinations", () => {
  it("keeps mobile search separate from per-space settings", () => {
    expect(MOBILE_SEARCH_PATH).toBe("/mobile/search");
    expect(getSidebarSpaceSettingsPath("space-1")).toBe("/spaces/space-1/settings");
  });
});

describe("workspace sidebar geometry", () => {
  it("uses the approved 272px expanded desktop sidebar", () => {
    const tailwindConfig = readFileSync(resolve(process.cwd(), "tailwind.config.ts"), "utf8");

    expect(tailwindConfig).toContain('sidebar: "272px"');
  });
});

describe("persistent workspace top bar", () => {
  const source = readFileSync(resolve(process.cwd(), "src/components/layout/AppLayout.tsx"), "utf8");

  it("mounts TopBar for ordinary desktop routes without a route-mode guard", () => {
    expect(source).toContain("<TopBar");
    expect(source).not.toMatch(/\{\(isMobile \|\| isWorkspaceDestination\) && \(\s*<TopBar/);
    expect(source).toContain("mobile={isMobile}");
  });

  it("preserves destination and mobile adaptations on the persistent bar", () => {
    expect(source).toContain("showSidebarToggle={!isWorkspaceDestination}");
    expect(source).toContain("destination={destination ?? undefined}");
    expect(source).toContain("sidebarOpen={isMobile ? sidebarOpen : !sidebarCollapsed}");
  });

  it("keeps the global bar before the main region so editor-owned toolbars render beneath it", () => {
    const topBarIndex = source.indexOf("<TopBar");
    const mainContentIndex = source.indexOf('id="main-content"');

    expect(topBarIndex).toBeGreaterThan(-1);
    expect(mainContentIndex).toBeGreaterThan(topBarIndex);
  });
});
