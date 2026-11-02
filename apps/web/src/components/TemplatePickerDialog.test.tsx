import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { TemplatePickerDialog } from "./TemplatePickerDialog";

describe("TemplatePickerDialog", () => {
  it("keeps template selection in one bounded, touch-safe shared dialog", () => {
    const html = renderToStaticMarkup(
      <TemplatePickerDialog
        templates={[
          {
            id: "template-1",
            space_id: "space-1",
            name: "Project brief",
            description: "A short project starting point.",
            content: "# Project brief",
            tags: [],
            created_at: "2026-07-23T00:00:00.000Z",
            updated_at: "2026-07-23T00:00:00.000Z",
          },
        ]}
        onClose={() => undefined}
        onSelect={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="template-picker-title"');
    expect(html).toContain('aria-describedby="template-picker-description"');
    expect(html).toContain('aria-label="Close template picker"');
    expect(html).toContain('aria-label="Search templates"');
    expect(html).toContain("Destination");
    expect(html).toContain("border-b");
    expect(html).toContain("Blank page");
    expect(html).toContain("Project brief");
    expect(html).toContain("min-h-11");
    expect(html).toContain("overflow-y-auto");
  });

  it("can require a saved template when opened from the command menu", () => {
    const html = renderToStaticMarkup(
      <TemplatePickerDialog
        templates={[
          {
            id: "template-1",
            space_id: "space-1",
            name: "Project brief",
            content: "# Project brief",
            tags: [],
            created_at: "2026-07-23T00:00:00.000Z",
            updated_at: "2026-07-23T00:00:00.000Z",
          },
        ]}
        allowBlank={false}
        onClose={() => undefined}
        onSelect={() => undefined}
      />,
    );

    expect(html).not.toContain("Blank page");
    expect(html).toContain("Project brief");
  });

  it("shows distinct loading, retryable failure, and empty search states", () => {
    const loading = renderToStaticMarkup(
      <TemplatePickerDialog templates={[]} loading onClose={() => undefined} onSelect={() => undefined} />,
    );
    const failure = renderToStaticMarkup(
      <TemplatePickerDialog
        templates={[]}
        loadError="Templates could not be loaded."
        onRetry={() => undefined}
        onClose={() => undefined}
        onSelect={() => undefined}
      />,
    );
    const empty = renderToStaticMarkup(
      <TemplatePickerDialog templates={[]} allowBlank={false} onClose={() => undefined} onSelect={() => undefined} />,
    );

    expect(loading).toContain('role="status"');
    expect(loading).toContain("Loading templates");
    expect(failure).toContain('role="alert"');
    expect(failure).toContain("Retry");
    expect(empty).toContain("No saved templates are available in this space.");
  });

  it("uses the reusable-section operation without offering a blank page", () => {
    const html = renderToStaticMarkup(
      <TemplatePickerDialog
        operation="insert"
        templates={[]}
        destination="Incident 42"
        onClose={() => undefined}
        onSelect={() => undefined}
      />,
    );

    expect(html).toContain("Insert reusable section");
    expect(html).toContain("Current page");
    expect(html).toContain("Incident 42");
    expect(html).not.toContain("Blank page");
  });
});
