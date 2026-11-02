import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { EditorTemplateSectionDialog } from "./EditorTemplateSectionDialog";

describe("EditorTemplateSectionDialog", () => {
  it("collects body values in a page-scoped shared dialog", () => {
    const html = renderToStaticMarkup(
      <EditorTemplateSectionDialog
        template={{
          id: "template-1",
          space_id: "space-1",
          name: "Incident summary",
          content: "<!-- starlane-template:v1:field:eyJpZCI6ImJvZHkiLCJmaWVsZF9pZCI6InNlcnZpY2UifQ -->",
          properties: {
            __starlane_template: {
              schema_version: 1,
              fields: [{ id: "service", label: "Service", type: "text", required: true }],
              title: [{ kind: "text", value: "Ignored title " }],
            },
          },
          tags: [],
          created_at: "2026-10-07T00:00:00.000Z",
          updated_at: "2026-10-07T00:00:00.000Z",
        }}
        renderContext={{ title: "Current page" }}
        currentPageLabel="Incident 42"
        onInsert={() => true}
        onClose={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain("Insert section");
    expect(html).toContain("Incident 42");
    expect(html).toContain('for="section-template-field-service"');
    expect(html).not.toContain("Ignored title");
  });
});
