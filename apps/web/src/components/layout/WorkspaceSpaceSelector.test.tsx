import { createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import {
  getWorkspaceSpaceSelectorInputAction,
  WorkspaceSpaceSelector,
  WorkspaceSpaceCreateDialog,
  WorkspaceSpaceSelectorMenu,
} from "./WorkspaceSpaceSelector";
import { useWorkspaceSpaces } from "./WorkspaceSpaces";

vi.mock("./WorkspaceSpaces", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./WorkspaceSpaces")>();
  return {
    ...actual,
    useWorkspaceSpaces: vi.fn(),
  };
});

describe("WorkspaceSpaceSelector", () => {
  it("names the trigger with the current space identity", () => {
    vi.mocked(useWorkspaceSpaces).mockReturnValue({
      spaces: [{ id: "space-a", name: "Alpha", slug: "alpha", icon: "🚀" }],
      activeSpace: { id: "space-a", name: "Alpha", slug: "alpha", icon: "🚀" },
      recentSpaceIds: ["space-a"],
      loading: false,
      canCreateSpace: true,
      selectSpace: vi.fn(),
      createSpace: vi.fn(),
      refreshSpaces: vi.fn(),
    });

    const html = renderToStaticMarkup(<WorkspaceSpaceSelector />);

    expect(html).toContain('aria-label="Select space, current space Alpha"');
    expect(html).toContain("Alpha");
    expect(html).toContain("🚀");
    expect(html).toContain('aria-haspopup="menu"');
  });

  it("maps Escape to close/focus restoration and ArrowDown to the option list", () => {
    expect(getWorkspaceSpaceSelectorInputAction("Escape")).toBe("close");
    expect(getWorkspaceSpaceSelectorInputAction("ArrowDown")).toBe("focus-options");
    expect(getWorkspaceSpaceSelectorInputAction("ArrowUp")).toBeNull();
  });

  it("renders searchable grouped radio choices with selected state and creation", () => {
    const html = renderToStaticMarkup(
      <WorkspaceSpaceSelectorMenu
        sections={[
          {
            label: "Recent",
            spaces: [{ id: "space-a", name: "Alpha", slug: "alpha" }],
          },
          {
            label: "Owned",
            spaces: [{ id: "space-b", name: "Bravo", slug: "bravo", member_role: "owner" }],
          },
          {
            label: "All spaces",
            spaces: [{ id: "space-c", name: "Charlie", slug: "charlie" }],
          },
        ]}
        activeSpaceId="space-a"
        loading={false}
        canCreateSpace
        menuRef={createRef<HTMLDivElement>()}
        onKeyDown={() => undefined}
        onSelect={() => undefined}
        onCreate={() => undefined}
      />,
    );

    expect(html).toContain('role="menu"');
    expect(html).toContain('aria-label="Recent"');
    expect(html).toContain('aria-label="Owned"');
    expect(html).toContain('aria-label="All spaces"');
    expect(html).toContain('role="menuitemradio" aria-checked="true"');
    expect(html).toContain("New space");
  });

  it("uses the shared labelled dialog anatomy for space creation", () => {
    const html = renderToStaticMarkup(
      <WorkspaceSpaceCreateDialog
        name="Project Atlas"
        creating={false}
        error=""
        inputRef={createRef<HTMLInputElement>()}
        onNameChange={() => undefined}
        onCancel={() => undefined}
        onSubmit={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="workspace-space-create-title"');
    expect(html).toContain('aria-describedby="workspace-space-create-description"');
    expect(html).toContain('id="workspace-space-name"');
    expect(html).toContain("Project Atlas");
    expect(html).toContain("Create space");
    expect(html).toContain("Cancel");
  });

  it("reports loading state and hides creation for unauthorized users", () => {
    const html = renderToStaticMarkup(
      <WorkspaceSpaceSelectorMenu
        sections={[]}
        loading
        canCreateSpace={false}
        menuRef={createRef<HTMLDivElement>()}
        onKeyDown={() => undefined}
        onSelect={() => undefined}
        onCreate={() => undefined}
      />,
    );

    expect(html).toContain('role="status"');
    expect(html).toContain("Loading spaces");
    expect(html).not.toContain("New space");
  });
});
