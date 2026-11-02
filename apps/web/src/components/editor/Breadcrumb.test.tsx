import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { Breadcrumb, buildBreadcrumbLabels } from "./Breadcrumb";

describe("Breadcrumb", () => {
  it("links the space crumb while keeping folder crumbs as plain labels", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <Breadcrumb
          spaceName="Personal Space"
          folderPath={["Projects", "Launch"]}
          title="Plan"
          spaceHref="/spaces/space-1/docs"
        />
      </MemoryRouter>,
    );

    expect(html).toContain('aria-label="Breadcrumb"');
    expect(html).toContain('href="/spaces/space-1/docs"');
    expect(html).toContain("flex-1");
    expect(html).toContain('aria-current="page"');
    expect(html).toContain("Personal Space");
    expect(html).toContain("Projects");
    expect(html).toContain("Launch");
    expect(html).toContain("Plan");
    expect(html).not.toContain("cursor-pointer");
  });

  it("links ordered parent pages between folders and the current page", () => {
    const parentPages = [
      { id: "parent-1", title: "Knowledge", href: "/spaces/space-1/docs/parent-1" },
      { id: "parent-2", title: "Long parent title", href: "/spaces/space-1/docs/parent-2" },
    ];
    expect(buildBreadcrumbLabels("Space", ["Folder"], "Child", parentPages)).toEqual([
      "Space",
      "Folder",
      "Knowledge",
      "Long parent title",
      "Child",
    ]);
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <Breadcrumb
          spaceName="Space"
          folderPath={["Folder"]}
          parentPages={parentPages}
          title="Child"
          spaceHref="/spaces/space-1/docs"
        />
      </MemoryRouter>,
    );
    expect(html).toContain('href="/spaces/space-1/docs/parent-1"');
    expect(html).toContain('href="/spaces/space-1/docs/parent-2"');
    expect(html).toContain('title="Long parent title"');
    expect(html).toContain('aria-current="page"');
  });

  it("allows dense header rows to remove the default bottom margin", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <Breadcrumb
          spaceName="Personal Space"
          title="Plan"
          spaceHref="/spaces/space-1/docs"
          className="mb-0"
        />
      </MemoryRouter>,
    );

    expect(html).toContain("mb-0");
    expect(html).not.toContain("mb-4");
  });

  it("can omit the redundant space crumb beside the workspace selector", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <Breadcrumb
          spaceName="Personal Space"
          folderPath={["Projects"]}
          title="Plan"
          spaceHref="/spaces/space-1/docs"
          showSpace={false}
        />
      </MemoryRouter>,
    );

    expect(html).not.toContain("Personal Space");
    expect(html).not.toContain('href="/spaces/space-1/docs"');
    expect(html).toContain("Projects");
    expect(html).toContain("Plan");
    expect(html).toContain('aria-current="page"');
  });

  it("keeps the current document crumb when the title is blank", () => {
    expect(buildBreadcrumbLabels(" Personal Space ", [" Projects ", " ", "Launch"], " ")).toEqual([
      "Personal Space",
      "Projects",
      "Launch",
      "Untitled",
    ]);

    const html = renderToStaticMarkup(
      <MemoryRouter>
        <Breadcrumb
          spaceName="Personal Space"
          folderPath={["Projects", "Launch"]}
          title=" "
          spaceHref="/spaces/space-1/docs"
        />
      </MemoryRouter>,
    );

    expect(html).toContain("Untitled");
    expect(html).toContain('aria-current="page"');
  });
  it("keeps the current page and deepest inline context for a deep path with a full-path owner", () => {
    const folders = Array.from({ length: 256 }, (_, i) => `Level ${i + 1}`);
    expect(buildBreadcrumbLabels("Space", folders, "Current")).toHaveLength(258);
    const html = renderToStaticMarkup(
      <MemoryRouter><Breadcrumb spaceName="Space" folderPath={folders} title="Current" /></MemoryRouter>,
    );
    expect(html).toContain('aria-label="Show full page path"');
    expect(html).toContain("Level 256");
    expect(html).toContain('title="Current"');
    expect(html).not.toContain('title="Level 1"');
  });

});
