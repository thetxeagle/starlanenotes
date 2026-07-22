import { describe, expect, it } from "vitest";
import { isAdminRoute, isDocumentPanelRoute } from "./AppLayout";
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

describe("mobile drawer destinations", () => {
  it("keeps mobile search separate from per-space settings", () => {
    expect(MOBILE_SEARCH_PATH).toBe("/mobile/search");
    expect(getSidebarSpaceSettingsPath("space-1")).toBe("/spaces/space-1/settings");
  });
});
