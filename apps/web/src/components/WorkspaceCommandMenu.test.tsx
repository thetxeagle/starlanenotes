import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import {
  getWorkspaceCommandShortcutLabel,
  getNextWorkspaceCommandIndex,
  WORKSPACE_COMMAND_GROUP_HEADING_CLASS,
  WorkspaceCommandMenu,
} from "./WorkspaceCommandMenu";

describe("WorkspaceCommandMenu", () => {
  it("labels shared workspace shortcuts for the user's platform", () => {
    expect(getWorkspaceCommandShortcutLabel("search", true)).toBe("⌘K");
    expect(getWorkspaceCommandShortcutLabel("new-document", false)).toBe("Ctrl+N");
    expect(getWorkspaceCommandShortcutLabel("new-space", true)).toBe("⌘⇧N");
    expect(getWorkspaceCommandShortcutLabel("new-space", false)).toBe("Ctrl+Shift+N");
  });

  it("cycles its active command index with the documented command-menu keys", () => {
    expect(getNextWorkspaceCommandIndex(0, 8, "ArrowDown")).toBe(1);
    expect(getNextWorkspaceCommandIndex(7, 8, "ArrowDown")).toBe(0);
    expect(getNextWorkspaceCommandIndex(0, 8, "ArrowUp")).toBe(7);
    expect(getNextWorkspaceCommandIndex(4, 8, "ArrowUp")).toBe(3);
    expect(getNextWorkspaceCommandIndex(4, 6, "Home")).toBe(0);
    expect(getNextWorkspaceCommandIndex(1, 8, "End")).toBe(7);
    expect(getNextWorkspaceCommandIndex(0, 0, "ArrowDown")).toBe(-1);
  });

  it("renders a grouped, keyboard-addressable command surface", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <WorkspaceCommandMenu open onClose={() => undefined} spaceId="space/one" />
      </MemoryRouter>,
    );

    expect(html).toContain('aria-label="Search workspace commands"');
    expect(html).toContain('id="workspace-command-menu-title"');
    expect(html).toContain("Workspace commands");
    expect(html).not.toContain('id="workspace-command-menu-title" class="sr-only"');
    expect(html).toContain('aria-label="Close command menu"');
    expect(html).toContain("h-11 w-11");
    expect(html).toContain("lg:h-7 lg:w-7");
    expect(html).toContain('aria-label="Workspace commands"');
    expect(html).toContain('aria-label="Create commands"');
    expect(html).toContain('aria-label="Navigate commands"');
    expect(html).toContain('role="menuitem"');
    expect(html).toContain('tabindex="0"');
    expect(html).toContain("Search documents");
    expect(html).toContain("New page");
    expect(html).toContain("New folder");
    expect(html).toContain("New space");
    expect(html).toContain("Open favorites");
    expect(html).toContain("Open daily note");
    expect(html).not.toContain("Open graph");
    expect(html).toContain("Open workflows");
  });

  it("uses quiet readable group labels instead of decorative eyebrow styling", () => {
    expect(WORKSPACE_COMMAND_GROUP_HEADING_CLASS).toContain("text-xs");
    expect(WORKSPACE_COMMAND_GROUP_HEADING_CLASS).not.toContain("uppercase");
    expect(WORKSPACE_COMMAND_GROUP_HEADING_CLASS).not.toContain("tracking-wide");
  });
});
