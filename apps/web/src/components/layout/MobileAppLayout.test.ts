import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  canUseMobileRightPanel,
  getMobileDestination,
  getMobileDestinationBackHref,
  getMobileDestinationBackLabel,
  getMobileNavigationToggleState,
  MobileAppLayout,
  shouldRenderMobileWorkspaceNavigation,
} from "./MobileAppLayout";
import {
  getWorkspaceShortcutAction,
  isWorkspaceOverlayOpen,
  isWorkspaceTextEntryTarget,
  shouldHandleWorkspaceShortcutEvent,
} from "./workspaceShortcuts";
import { MOBILE_DOCUMENT_PANEL_BODY_CLASS } from "./MobileDocumentPanel";

let launchMode: "server" | "local" = "server";

vi.mock("@/components/BrandLogo", () => ({
  BrandLogo: () => "Brand logo",
}));

vi.mock("./Sidebar", () => ({
  Sidebar: () => createElement("div", { "aria-label": "Shared sidebar" }, "Shared sidebar"),
}));

vi.mock("@/components/NotificationBell", () => ({
  NotificationBell: () => createElement("span", null, "Notification bell"),
}));

vi.mock("@/lib/launchMode", () => ({
  getCurrentLaunchMode: () => launchMode,
}));

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    user: null,
  }),
}));

describe("canUseMobileRightPanel", () => {
  it("allows the document panel on mobile document routes", () => {
    expect(canUseMobileRightPanel("/spaces/space-1/docs/doc-1")).toBe(true);
    expect(canUseMobileRightPanel("/spaces/space-1/docs/doc-1/")).toBe(true);
  });

  it("blocks the document panel on unrelated mobile and app routes", () => {
    expect(canUseMobileRightPanel("/profile")).toBe(false);
    expect(canUseMobileRightPanel("/mobile/profile")).toBe(false);
    expect(canUseMobileRightPanel("/mobile/search")).toBe(false);
    expect(canUseMobileRightPanel("/mobile/recent")).toBe(false);
    expect(canUseMobileRightPanel("/spaces/space-1/recent")).toBe(false);
    expect(canUseMobileRightPanel("/spaces/space-1/graph")).toBe(false);
  });
});

describe("getMobileNavigationToggleState", () => {
  it("uses the same header toggle semantics for opening and closing navigation", () => {
    expect(getMobileNavigationToggleState(false)).toBe(true);
    expect(getMobileNavigationToggleState(true)).toBe(false);
  });
});

describe("mobile destination route ownership", () => {
  it("keeps long-lived account and administration destinations out of the workspace drawer", () => {
    expect(shouldRenderMobileWorkspaceNavigation("/admin")).toBe(false);
    expect(shouldRenderMobileWorkspaceNavigation("/admin/")).toBe(false);
    expect(shouldRenderMobileWorkspaceNavigation("/profile")).toBe(false);
    expect(shouldRenderMobileWorkspaceNavigation("/profile/security")).toBe(false);
    expect(shouldRenderMobileWorkspaceNavigation("/mobile/profile")).toBe(false);
    expect(shouldRenderMobileWorkspaceNavigation("/mobile/profile/security")).toBe(false);
    expect(shouldRenderMobileWorkspaceNavigation("/mobile/app-settings")).toBe(false);
    expect(shouldRenderMobileWorkspaceNavigation("/spaces/space-1/settings")).toBe(false);
    expect(shouldRenderMobileWorkspaceNavigation("/spaces/space-1/docs/doc-1")).toBe(true);
  });

  it("uses one named destination header for account and administration routes", () => {
    expect(getMobileDestination("/admin")).toEqual({
      title: "Site Administration",
      headerLabel: "Site administration top bar",
      backTarget: "notes",
      showUtilities: false,
      showTitle: false,
    });
    expect(getMobileDestination("/profile")).toEqual({
      title: "Account Settings",
      headerLabel: "Account settings top bar",
      backTarget: "notes",
      showUtilities: false,
      showTitle: false,
    });
    expect(getMobileDestination("/profile/security")).toEqual({
      title: "Account Settings",
      headerLabel: "Account settings top bar",
      backTarget: "notes",
      showUtilities: false,
      showTitle: false,
    });
    expect(getMobileDestination("/mobile/app-settings")).toEqual({
      title: "Settings",
      headerLabel: "Settings top bar",
      backTarget: "notes",
      showUtilities: false,
      showTitle: false,
    });
    expect(getMobileDestination("/spaces/space-1/settings")).toEqual({
      title: "Space settings",
      headerLabel: "Space settings top bar",
      backTarget: "space",
      showUtilities: false,
      showTitle: false,
    });
  });

  it("returns a named parent path for space settings and Notes for global destinations", () => {
    const spaceDestination = getMobileDestination("/spaces/space%2Fone/settings");
    expect(getMobileDestinationBackHref(spaceDestination, "space/one")).toBe(
      "/spaces/space%2Fone/docs",
    );
    expect(getMobileDestinationBackLabel(spaceDestination)).toBe("Back to space");
    expect(getMobileDestinationBackHref(getMobileDestination("/profile"), "space-1")).toBe("/");
    expect(getMobileDestinationBackLabel(getMobileDestination("/profile"))).toBe("Back to Notes");
  });
});

describe("getWorkspaceShortcutAction", () => {
  it("leaves already-owned overlay keyboard events to their local lifecycle", () => {
    expect(shouldHandleWorkspaceShortcutEvent({ defaultPrevented: false })).toBe(true);
    expect(shouldHandleWorkspaceShortcutEvent({ defaultPrevented: true })).toBe(false);
    expect(shouldHandleWorkspaceShortcutEvent({ defaultPrevented: false }, true)).toBe(false);
  });

  it("does not hand global shortcuts to the shell while a dialog or menu owns the surface", () => {
    expect(isWorkspaceOverlayOpen({ querySelector: () => null })).toBe(false);
    expect(isWorkspaceOverlayOpen({ querySelector: () => ({}) })).toBe(true);
  });

  it("keeps unmodified shell shortcuts out of document and form entry", () => {
    expect(isWorkspaceTextEntryTarget({ tagName: "INPUT", isContentEditable: false })).toBe(true);
    expect(isWorkspaceTextEntryTarget({ tagName: "TEXTAREA", isContentEditable: false })).toBe(true);
    expect(isWorkspaceTextEntryTarget({ tagName: "DIV", isContentEditable: true })).toBe(true);
    expect(isWorkspaceTextEntryTarget({ tagName: "DIV", isContentEditable: false })).toBe(false);
  });

  it("keeps mobile workspace shortcuts out of the editable document surface", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/layout/MobileAppLayout.tsx"), "utf8");

    expect(source).toContain("isWorkspaceTextEntryTarget(event.target as HTMLElement)");
  });

  it("keeps desktop and iPad hardware-keyboard workspace shortcuts aligned", () => {
    expect(getWorkspaceShortcutAction({ key: "k", metaKey: true, ctrlKey: false, shiftKey: false })).toBe("search");
    expect(getWorkspaceShortcutAction({ key: "P", metaKey: false, ctrlKey: true, shiftKey: false })).toBe("command");
    expect(getWorkspaceShortcutAction({ key: "n", metaKey: true, ctrlKey: false, shiftKey: false })).toBe("new-document");
    expect(getWorkspaceShortcutAction({ key: "n", metaKey: true, ctrlKey: false, shiftKey: true })).toBe("new-space");
    expect(getWorkspaceShortcutAction({ key: "\\", metaKey: true, ctrlKey: false, shiftKey: false })).toBe("navigation");
    expect(getWorkspaceShortcutAction({ key: "d", metaKey: true, ctrlKey: false, shiftKey: true })).toBe("toggle-theme");
  });

  it("does not claim unmodified or shifted keys as workspace shortcuts", () => {
    expect(getWorkspaceShortcutAction({ key: "k", metaKey: false, ctrlKey: false, shiftKey: false })).toBeNull();
    expect(getWorkspaceShortcutAction({ key: "p", metaKey: true, ctrlKey: false, shiftKey: true })).toBeNull();
  });
});

describe("MobileAppLayout", () => {
  it("owns a conditional bottom-status inset for fixed mobile editor chrome", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/layout/MobileAppLayout.tsx"), "utf8");
    const css = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");

    expect(source).toContain("starlane-mobile-app");
    expect(css).toContain(".starlane-mobile-app:has(> .starlane-status-bar)");
    expect(css).toContain("--starlane-mobile-statusbar-height: 1.5rem");
  });

  it("keeps the mobile document panel body bounded under the fixed header", () => {
    expect(MOBILE_DOCUMENT_PANEL_BODY_CLASS).toContain("min-h-0");
    expect(MOBILE_DOCUMENT_PANEL_BODY_CLASS).toContain("flex-1");
    expect(MOBILE_DOCUMENT_PANEL_BODY_CLASS).toContain("overflow-hidden");
  });

  it("keeps workspace utilities out of the persistent mobile header", () => {
    const html = renderToStaticMarkup(
      createElement(
        MemoryRouter,
        null,
        createElement(MobileAppLayout, null, createElement("main", null, "Content")),
      ),
    );

    expect(html).toContain('aria-label="Open navigation"');
    expect(html).toContain('aria-label="Mobile top bar"');
    expect(html).not.toContain('aria-label="Open command menu"');
    expect(html).not.toContain('aria-label="Open search"');
    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain('aria-label="Open app settings"');
    expect(html).toContain("Brand logo");
    expect(html).not.toContain("Shared sidebar");
  });

  it("does not mount the server-backed notification inbox in local mode", () => {
    launchMode = "local";
    const html = renderToStaticMarkup(
      createElement(
        MemoryRouter,
        null,
        createElement(MobileAppLayout, null, createElement("main", null, "Content")),
      ),
    );

    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain('aria-label="Open search"');
    launchMode = "server";
  });

  it("uses a destination header instead of the normal workspace drawer on Site Administration", () => {
    const html = renderToStaticMarkup(
      createElement(
        MemoryRouter,
        { initialEntries: ["/admin"] },
        createElement(MobileAppLayout, null, createElement("main", null, "Content")),
      ),
    );

    expect(html).toContain('aria-label="Site administration top bar"');
    expect(html).toContain('aria-label="Back to Notes"');
    expect(html).not.toContain("Site Administration");
    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain('aria-label="Open search"');
    expect(html).not.toContain('aria-label="Open navigation"');
    expect(html).not.toContain('aria-label="Open command menu"');
    expect(html).not.toContain("Shared sidebar");
  });

  it("uses a destination header instead of the normal workspace drawer on Profile", () => {
    const html = renderToStaticMarkup(
      createElement(
        MemoryRouter,
        { initialEntries: ["/profile"] },
        createElement(MobileAppLayout, null, createElement("main", null, "Content")),
      ),
    );

    expect(html).toContain('aria-label="Account settings top bar"');
    expect(html).toContain('aria-label="Back to Notes"');
    expect(html).not.toContain("Account Settings");
    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain('aria-label="Open search"');
    expect(html).not.toContain('aria-label="Open navigation"');
    expect(html).not.toContain('aria-label="Open command menu"');
    expect(html).not.toContain("Shared sidebar");
  });

  it("uses a space return path instead of the normal workspace drawer on Space settings", () => {
    const html = renderToStaticMarkup(
      createElement(
        MemoryRouter,
        { initialEntries: ["/spaces/space-1/settings"] },
        createElement(MobileAppLayout, null, createElement("main", null, "Content")),
      ),
    );

    expect(html).toContain('aria-label="Space settings top bar"');
    expect(html).toContain('aria-label="Back to space"');
    expect(html).not.toContain('aria-label="Open navigation"');
    expect(html).not.toContain('aria-label="Open command menu"');
    expect(html).not.toContain("Shared sidebar");
  });
});
