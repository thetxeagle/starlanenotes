import { MemoryRouter } from "react-router-dom";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { STARLANE_VERSION } from "@/lib/appMetadata";
import { shouldOpenTopBarSearchShortcut, TopBar } from "./TopBar";

let userRole = "admin";
let launchMode: "server" | "local" = "server";

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    user: {
      id: "admin-user",
      email: "admin@example.test",
      display_name: "Admin User",
      role: userRole,
      totp_enabled: false,
    },
    config: { site_name: "Starlane Notes" },
    logout: vi.fn(),
  }),
}));

vi.mock("@/lib/launchMode", () => ({
  getCurrentLaunchMode: () => launchMode,
}));

vi.mock("@/components/NotificationBell", () => ({
  NotificationBell: () => <span>Notification bell</span>,
}));

vi.mock("./WorkspaceSpaceSelector", () => ({
  WorkspaceSpaceSelector: () => <span>Workspace space selector</span>,
}));

vi.mock("./WorkspaceProfileMenu", () => ({
  WorkspaceProfileMenu: () => <span>Workspace profile menu</span>,
}));

vi.mock("./AppearanceToggle", () => ({
  AppearanceToggle: () => (
    <button type="button" aria-label="Appearance: system. Switch appearance">
      Appearance selector
    </button>
  ),
}));

vi.mock("@/components/SyncStatus", () => ({
  SyncStatus: ({ hideSynced }: { hideSynced?: boolean }) => (
    <span>{hideSynced ? "Quiet sync status" : "Sync status"}</span>
  ),
}));

vi.mock("@/components/SearchModal", () => ({
  SearchModal: () => null,
}));

function renderTopBar(props: Partial<Parameters<typeof TopBar>[0]> = {}) {
  return renderToStaticMarkup(
    <MemoryRouter><TopBar
      sidebarOpen
      onToggleSidebar={() => undefined}
      rightPanelOpen={false}
      onToggleRightPanel={() => undefined}
      {...props}
    /></MemoryRouter>,
  );
}

describe("TopBar", () => {
  beforeEach(() => {
    launchMode = "server";
    userRole = "admin";
  });

  it("composes navigation and utilities across the two desktop top-bar regions", () => {
    const html = renderTopBar();

    expect(html).not.toContain("Workspace space selector");
    expect(html).toContain("Workspace profile menu");
    expect(html).toContain('aria-label="About Starlane Notes"');
    expect(html).toContain(`v${STARLANE_VERSION}`);
    expect(html.indexOf("Appearance selector")).toBeLessThan(html.indexOf("Notification bell"));
    expect(html.indexOf("Notification bell")).toBeLessThan(html.indexOf("Workspace profile menu"));
    expect(html).toContain("grid-cols-[minmax(0,1fr)_minmax(0,auto)]");
    expect(html).toContain("overflow-x-auto");
    expect(html).toContain("Quiet sync status");
  });

  it("places an administration gear beside the profile only for server admins", () => {
    const admin = renderTopBar();
    expect(admin).toContain('aria-label="Admin settings"');
    expect(admin).toContain('href="/admin"');
    expect(admin.indexOf('aria-label="Admin settings"')).toBeLessThan(admin.indexOf("Workspace profile menu"));
    userRole = "member";
    expect(renderTopBar()).not.toContain('aria-label="Admin settings"');
    userRole = "admin";
    launchMode = "local";
    expect(renderTopBar()).not.toContain('aria-label="Admin settings"');
  });

  it("reserves the document action slot before global utilities", () => {
    const html = renderTopBar();

    const actionsSlot = html.indexOf('data-workspace-top-bar-slot="actions"');
    const appearance = html.indexOf("Appearance selector");

    expect(html).not.toContain('data-workspace-top-bar-slot="breadcrumb"');
    expect(actionsSlot).toBeLessThan(appearance);
  });

  it("can hide the duplicate properties panel toggle", () => {
    const html = renderTopBar({ showRightPanelToggle: false });

    expect(html).not.toContain("Open properties panel");
    expect(html).not.toContain("Close properties panel");
  });

  it("can hide the workspace sidebar toggle without adding duplicate product identity", () => {
    const html = renderTopBar({ showSidebarToggle: false });

    expect(html).not.toContain("Toggle sidebar");
    expect(html).not.toContain("Open Starlane Notes home");
  });

  it("renders a named return context for a destination instead of workspace controls", () => {
    const html = renderTopBar({
      showSidebarToggle: false,
      destination: {
        title: "Space settings",
        backHref: "/spaces/space-1/docs",
        backLabel: "Back to space",
      },
    });

    expect(html).toContain('href="/spaces/space-1/docs"');
    expect(html).toContain('aria-label="Back to space"');
    expect(html).toContain("Space settings");
    expect(html).not.toContain('aria-label="Collapse sidebar"');
    expect(html).not.toContain('aria-label="Open command menu"');
    expect(html).not.toContain("Workspace space selector");
    expect(html).toContain("Workspace profile menu");
  });

  it("keeps the legacy command-menu button out of desktop chrome", () => {
    expect(renderTopBar()).not.toContain("Open command menu");
  });

  it("places the appearance selector beside notifications and profile", () => {
    const html = renderTopBar();

    expect(html).toContain('aria-label="Appearance: system. Switch appearance"');
    expect(html.indexOf('aria-label="Open search"')).toBeLessThan(
      html.indexOf('aria-label="Appearance: system. Switch appearance"'),
    );
  });

  it("keeps desktop utilities separate from sidebar Search", () => {
    const html = renderTopBar();

    expect(html).toContain('aria-label="Collapse sidebar"');
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain('title="Collapse sidebar"');
    expect(html).not.toContain("Open Starlane Notes home");
    expect(html).not.toContain('aria-label="Open search"');
    expect(html).not.toContain("Search Starlane Notes with SNQL");
    expect(html).not.toContain("Search SNQL:");
    expect(html).toContain("grid-cols-[minmax(0,1fr)_minmax(0,auto)]");

  });

  it("delegates desktop Search input and dismissal to the shared dialog", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/layout/TopBar.tsx"), "utf8");

    expect(source).toContain('presentation="dialog"');
    expect(source).not.toContain("<Popover");
    expect(source).not.toContain("popoverInputRef");
    expect(source).not.toContain("searchQuery");
    expect(source).not.toContain("<SearchInput");
  });

  it("names the compact-rail restore action explicitly", () => {
    const html = renderTopBar({ sidebarOpen: false });

    expect(html).toContain('aria-label="Expand sidebar"');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain('title="Expand sidebar"');
  });

  it("matches the native compact header on responsive mobile web", () => {
    const html = renderTopBar({ mobile: true });

    expect(html).toContain('aria-label="Mobile top bar"');
    expect(html).toContain('aria-label="Close navigation"');
    expect(html).not.toContain('aria-label="Open command menu"');
    expect(html).not.toContain('aria-label="Open search"');
    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain("Workspace space selector");
    expect(html).not.toContain("Workspace profile menu");
    expect(html).toContain("Starlane Notes");
    expect(html).not.toContain("Search Starlane Notes with SNQL");
    expect(html).not.toContain("grid-cols-[minmax(0,1fr)_minmax(18rem,3fr)_minmax(0,1fr)]");
  });

  it("keeps local mode profile access without server notifications", () => {
    launchMode = "local";
    const html = renderTopBar();

    expect(html).not.toContain("Notification bell");
    expect(html).toContain("Workspace profile menu");
  });

  it("does not open global Search underneath an active dialog or menu", () => {
    expect(
      shouldOpenTopBarSearchShortcut(
        { key: "k", metaKey: true, ctrlKey: false, defaultPrevented: false },
        false,
      ),
    ).toBe(true);
    expect(
      shouldOpenTopBarSearchShortcut(
        { key: "k", metaKey: true, ctrlKey: false, defaultPrevented: false },
        true,
      ),
    ).toBe(false);
    expect(
      shouldOpenTopBarSearchShortcut(
        { key: "k", metaKey: true, ctrlKey: false, defaultPrevented: true },
        false,
      ),
    ).toBe(false);
  });
});
