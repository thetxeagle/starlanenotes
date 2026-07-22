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
});
