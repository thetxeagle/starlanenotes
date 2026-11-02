import { createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import type { User } from "@/hooks/useAuth";
import {
  getWorkspaceSettingsNavigationOptions,
  WORKSPACE_SITE_ADMINISTRATION_PATH,
  WorkspaceProfileMenuContent,
} from "./WorkspaceProfileMenu";

const user: User = {
  id: "user-1",
  email: "eagle@example.test",
  display_name: "Eagle",
  role: "member",
  totp_enabled: false,
};

function renderMenu({
  localMode = false,
  menuUser = user,
}: {
  localMode?: boolean;
  menuUser?: User | null;
} = {}) {
  return renderToStaticMarkup(
    <WorkspaceProfileMenuContent
      localMode={localMode}
      user={menuUser}
      menuRef={createRef<HTMLDivElement>()}
      onKeyDown={() => undefined}
      onNavigate={() => undefined}
      onSignOut={() => undefined}
    />,
  );
}

describe("WorkspaceProfileMenu", () => {
  it("keeps the authenticated profile menu focused on personal account actions", () => {
    const html = renderMenu();

    expect(html).toContain("Eagle");
    expect(html).toContain("eagle@example.test");
    expect(html).toContain("Account settings");
    expect(html).toContain("Sign out");
    expect(html).not.toContain("Space settings");
    expect(html).not.toContain("App settings");
    expect(html).not.toContain("Site administration");
  });

  it("keeps server administration out of the profile menu for administrators", () => {
    expect(renderMenu()).not.toContain("Site administration");
    expect(renderMenu({ menuUser: { ...user, role: "admin" } })).not.toContain(
      "Site administration",
    );
  });

  it("renders the local identity and App settings without server account actions", () => {
    const html = renderMenu({ localMode: true, menuUser: null });

    expect(html).toContain("Local workspace");
    expect(html).toContain("Stored on this device");
    expect(html).toContain("App settings");
    expect(html).toContain("Storage administration");
    expect(html).not.toContain("Space settings");
    expect(html).not.toContain("Account settings");
    expect(html).not.toContain("Sign out");
  });

  it("opens administration as a full desktop workspace and keeps other settings modal", () => {
    const location = {
      pathname: "/",
      search: "",
      hash: "",
      state: null,
      key: "workspace",
    };

    expect(getWorkspaceSettingsNavigationOptions(WORKSPACE_SITE_ADMINISTRATION_PATH, location))
      .toBeUndefined();
    expect(getWorkspaceSettingsNavigationOptions("/profile", location)).toEqual({
      state: { backgroundLocation: location },
    });
  });

  it("uses the shared popover and menu keyboard contract", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/WorkspaceProfileMenu.tsx"),
      "utf8",
    );

    expect(source).toContain("<Popover open={open} onOpenChange={setOpen}>");
    expect(source).toContain("useMenuKeyboardNavigation(menuRef)");
    expect(source).toContain('event.key === "Escape"');
    expect(renderMenu()).toContain('role="menu"');
    expect(renderMenu()).toContain('role="menuitem"');
  });
});
