import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { Settings, UserRound } from "lucide-react";
import { describe, expect, it } from "vitest";
import { SETTINGS_WORKSPACE_NAV_CLASS, SettingsDialog } from "./SettingsDialog";

describe("SettingsDialog", () => {
  it("keeps the compact settings rail scrollable without exposing browser chrome", () => {
    expect(SETTINGS_WORKSPACE_NAV_CLASS).toContain("overflow-x-auto");
    expect(SETTINGS_WORKSPACE_NAV_CLASS).toContain("scrollbar-none");
  });

  it("renders named section navigation with one current section and a labeled content panel", () => {
    const html = renderToStaticMarkup(
      <SettingsDialog
        title="Account settings"
        description="Manage your personal workspace preferences."
        sections={[
          { id: "account", label: "Account", icon: createElement(UserRound, { size: 16 }) },
          { id: "appearance", label: "Appearance", icon: createElement(Settings, { size: 16 }) },
        ]}
        activeSection="account"
        onSectionChange={() => undefined}
        onClose={() => undefined}
      >
        <p>Account content</p>
      </SettingsDialog>,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-label="Account settings sections"');
    expect(html).toContain('aria-current="page"');
    expect(html).toContain('aria-controls="settings-workspace-panel"');
    expect(html).toContain('aria-label="Account"');
    expect(html).not.toContain('role="tab"');
    expect(html).not.toContain('role="tabpanel"');
    expect(html).toContain("max-w-6xl");
    expect(html).toContain("h-[min(54rem,calc(100dvh-2rem))]");
    expect(html).toContain("Account content");
  });
});
