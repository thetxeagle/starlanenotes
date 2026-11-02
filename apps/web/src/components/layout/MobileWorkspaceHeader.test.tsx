import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MobileDestinationHeader, MobileWorkspaceHeader } from "./MobileWorkspaceHeader";

let launchMode: "server" | "local" = "server";

vi.mock("@/components/BrandLogo", () => ({
  BrandLogo: ({ variant }: { variant?: string }) => (
    <span data-variant={variant}>Starlane Notes</span>
  ),
}));

vi.mock("@/components/NotificationBell", () => ({
  NotificationBell: () => <span>Notification bell</span>,
}));

vi.mock("@/lib/launchMode", () => ({
  getCurrentLaunchMode: () => launchMode,
}));

function renderHeader() {
  return renderToStaticMarkup(
    <MobileWorkspaceHeader
      onToggleNavigation={() => undefined}
      containerClassName="test-header"
    />,
  );
}

function renderAdminHeader() {
  return renderToStaticMarkup(
    <MobileDestinationHeader
      title="Site Administration"
      ariaLabel="Site administration top bar"
      onOpenSearch={() => undefined}
      showUtilities={false}
      showTitle={false}
      containerClassName="test-header"
    />,
  );
}

function renderProfileHeader() {
  return renderToStaticMarkup(
    <MobileDestinationHeader
      title="Profile"
      ariaLabel="Profile top bar"
      onOpenSearch={() => undefined}
      showUtilities={false}
      showTitle={false}
      containerClassName="test-header"
    />,
  );
}

function renderSpaceSettingsHeader() {
  return renderToStaticMarkup(
    <MobileDestinationHeader
      title="Space settings"
      ariaLabel="Space settings top bar"
      backHref="/spaces/space-1/docs"
      backLabel="Back to space"
      onOpenSearch={() => undefined}
      showUtilities={false}
      showTitle={false}
      containerClassName="test-header"
    />,
  );
}

function renderSettingsHeader() {
  return renderToStaticMarkup(
    <MobileDestinationHeader
      title="Settings"
      ariaLabel="Settings top bar"
      onOpenSearch={() => undefined}
      showUtilities={false}
      showTitle={false}
      containerClassName="test-header"
    />
  );
}

describe("MobileWorkspaceHeader", () => {
  beforeEach(() => {
    launchMode = "server";
  });

  it("keeps the workspace header focused on navigation and product identity", () => {
    const html = renderHeader();

    const navigation = html.indexOf('aria-label="Open navigation"');
    const brand = html.indexOf("Starlane Notes");

    expect(html).toContain('aria-label="Mobile top bar"');
    expect(navigation).toBeGreaterThan(-1);
    expect(navigation).toBeLessThan(brand);
    expect(html).toContain('data-variant="mobile-header"');
    expect(html).not.toContain('aria-label="Open command menu"');
    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain('aria-label="Open search"');
  });

  it("uses the same minimal header in local mode", () => {
    launchMode = "local";
    const html = renderHeader();

    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain('aria-label="Open search"');
  });

  it("gives mobile Site Administration its own return-focused header", () => {
    const html = renderAdminHeader();

    const back = html.indexOf('aria-label="Back to Notes"');
    expect(html).toContain('aria-label="Site administration top bar"');
    expect(back).toBeGreaterThan(-1);
    expect(html).not.toContain("Site Administration");
    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain('aria-label="Open search"');
    expect(html).not.toContain('aria-label="Open navigation"');
    expect(html).not.toContain('aria-label="Open command menu"');
  });

  it("uses the same return-focused chrome for Profile without restoring workspace controls", () => {
    const html = renderProfileHeader();

    expect(html).toContain('aria-label="Profile top bar"');
    expect(html).toContain('aria-label="Back to Notes"');
    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain('aria-label="Open search"');
    expect(html).not.toContain('aria-label="Open navigation"');
    expect(html).not.toContain('aria-label="Open command menu"');
  });

  it("lets Space settings return to its parent space without restoring workspace controls", () => {
    const html = renderSpaceSettingsHeader();

    expect(html).toContain('aria-label="Space settings top bar"');
    expect(html).toContain('aria-label="Back to space"');
    expect(html).not.toContain('aria-label="Open navigation"');
    expect(html).not.toContain('aria-label="Open command menu"');
  });

  it("keeps Settings focused on the return path and product identity", () => {
    const html = renderSettingsHeader();

    expect(html).toContain('aria-label="Settings top bar"');
    expect(html).toContain('aria-label="Back to Notes"');
    expect(html).not.toContain("Notification bell");
    expect(html).not.toContain('aria-label="Open search"');
  });
});
