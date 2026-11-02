import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { EditorMobileToolbarCustomizeDialog } from "./EditorMobileToolbarCustomizeDialog";

describe("EditorMobileToolbarCustomizeDialog", () => {
  it("uses the shared labelled dialog shell for touch-safe toolbar configuration", () => {
    const html = renderToStaticMarkup(
      <EditorMobileToolbarCustomizeDialog
        activeActions={[{ id: "bold", label: "Bold", icon: <span>B</span> }]}
        availableActions={[
          { id: "sectionControls", label: "Section controls", icon: <span>S</span> },
          { id: "upload", label: "Upload", icon: <span>U</span> },
        ]}
        onClose={() => undefined}
        onMove={() => undefined}
        onRemove={() => undefined}
        onAdd={() => undefined}
        onReorder={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="mobile-toolbar-customize-title"');
    expect(html).toContain("Current actions");
    expect(html).toContain("Available actions");
    expect(html).toContain("Section controls");
    expect(html).toContain("Done");
    expect(html).not.toContain("uppercase");
    expect(html).not.toContain("tracking-wide");
  });
});
