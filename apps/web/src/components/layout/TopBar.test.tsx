import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  LOCAL_SETTINGS_PATH,
  SITE_ADMIN_PATH,
  TOP_BAR_DANGER_MENU_ITEM_CLASS,
  TopBar,
  WEB_PROFILE_PATH,
} from "./TopBar";

const navigateMock = vi.fn();
let launchMode: "server" | "local" = "server";

vi.mock("react-router-dom", () => ({
  useNavigate: () => navigateMock,
}));

vi.mock("@/hooks/useTheme", () => ({
  useTheme: () => ({
    theme: "light",
    setTheme: vi.fn(),
  }),
}));

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    user: {
      id: "admin-user",
      email: "admin@example.test",
      display_name: "Admin User",
      role: "admin",
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
  NotificationBell: () => null,
}));

vi.mock("@/components/SyncStatus", () => ({
  SyncStatus: () => null,
}));

vi.mock("@/components/SearchModal", () => ({
  SearchModal: () => null,
}));

vi.mock("@/components/BrandLogo", () => ({
  BrandLogo: () => <span>Starlane Notes</span>,
}));

function renderTopBar(props: Partial<Parameters<typeof TopBar>[0]> = {}) {
  return renderToStaticMarkup(
    <TopBar
      sidebarOpen
      onToggleSidebar={() => undefined}
      rightPanelOpen={false}
      onToggleRightPanel={() => undefined}
      initialUserMenuOpen
      {...props}
    />,
  );
}

describe("TopBar", () => {
  beforeEach(() => {
    launchMode = "server";
  });

  it("separates profile from site administration in the web user menu", () => {
    const html = renderTopBar();

    expect(WEB_PROFILE_PATH).toBe("/profile");
    expect(SITE_ADMIN_PATH).toBe("/admin");
    expect(html).toContain("Profile");
    expect(html).toContain("Site administration");
    expect(html).not.toContain(">Settings<");
  });

  it("does not render a duplicate web settings button outside local mode", () => {
    const html = renderTopBar();

    expect(html).not.toContain("Open local settings");
    expect(html).not.toContain("Open settings");
  });

  it("labels the local-only settings button as local settings", () => {
    launchMode = "local";
    const html = renderTopBar();

    expect(LOCAL_SETTINGS_PATH).toBe("/admin?section=storage");
    expect(LOCAL_SETTINGS_PATH).not.toBe(SITE_ADMIN_PATH);
    expect(html).toContain("Open local settings");
    expect(html).not.toContain("Open settings");
  });

  it("can hide the duplicate properties panel toggle", () => {
    const html = renderTopBar({ showRightPanelToggle: false });

    expect(html).not.toContain("Open properties panel");
    expect(html).not.toContain("Close properties panel");
  });

  it("can hide the workspace sidebar toggle while retaining the Notes home action", () => {
    const html = renderTopBar({ showSidebarToggle: false });

    expect(html).not.toContain("Toggle sidebar");
    expect(html).toContain("Open Starlane Notes home");
  });

  it("keeps brand and a centered SNQL search field in the app header", () => {
    const html = renderTopBar();

    expect(html).toContain("Toggle sidebar");
    expect(html).toContain("Open Starlane Notes home");
    expect(html).toContain("Starlane Notes");
    expect(html).toContain("Search Starlane Notes with SNQL");
    expect(html).toContain('Search SNQL: &quot;notes&quot; tag:work folder:project has:attachment');
    expect(html).toContain("grid-cols-[1fr_auto_1fr]");
  });

  it("uses semantic danger tokens for the sign-out menu item", () => {
    expect(TOP_BAR_DANGER_MENU_ITEM_CLASS).toContain("text-danger");
    expect(TOP_BAR_DANGER_MENU_ITEM_CLASS).toContain("hover:bg-danger");
    expect(TOP_BAR_DANGER_MENU_ITEM_CLASS).not.toContain("red-");
  });
});
