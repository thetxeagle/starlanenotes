import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SpaceRolePermissions } from "./SpaceRolePermissions";

describe("SpaceRolePermissions", () => {
  it("renders the enforced built-in role matrix as a responsive data table", () => {
    const html = renderToStaticMarkup(<SpaceRolePermissions />);

    expect(html).toContain("Space roles");
    expect(html).toContain("Space admin");
    expect(html).toContain("Commenter");
    expect(html).toContain("Manage members");
    expect(html).toContain("overflow-x-auto");
    expect(html).toContain("Built in");
  });

  it("distinguishes editable space roles from locked built-ins in the shared table schema", () => {
    const html = renderToStaticMarkup(
      <SpaceRolePermissions
        definitions={[
          {
            id: "space_admin",
            name: "Space admin",
            description: "Built in",
            permissions: ["view", "manage_space"],
            built_in: true,
          },
          {
            id: "custom:reviewer",
            name: "Reviewer",
            description: "Reviews content",
            permissions: ["view", "comment"],
            built_in: false,
          },
        ]}
        query=""
        onQueryChange={() => undefined}
        onCreate={() => undefined}
        onEdit={() => undefined}
        onDelete={() => undefined}
      />,
    );

    expect(html).toContain("Create role");
    expect(html).toContain("Search roles or permissions");
    expect(html).toContain("This space");
    expect(html).toContain('aria-label="Edit Reviewer"');
    expect(html).toContain('aria-label="Delete Reviewer"');
    expect(html).not.toContain('aria-label="Edit Space admin"');
  });
});
