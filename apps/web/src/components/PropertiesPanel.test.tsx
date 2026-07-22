import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  getPropertyDeleteConfirmationMessage,
  PROPERTIES_PANEL_ERROR_CLASS,
  PropertyTypeSelect,
} from "./PropertiesPanel";

describe("PropertyTypeSelect", () => {
  it("uses semantic danger tokens for workflow errors", () => {
    expect(PROPERTIES_PANEL_ERROR_CLASS).toContain("border-danger");
    expect(PROPERTIES_PANEL_ERROR_CLASS).toContain("bg-danger");
    expect(PROPERTIES_PANEL_ERROR_CLASS).toContain("text-danger");
    expect(PROPERTIES_PANEL_ERROR_CLASS).not.toContain("red-");
  });

  it("uses the shared select control", () => {
    const html = renderToStaticMarkup(
      <PropertyTypeSelect value="text" onChange={() => undefined} />,
    );

    expect(html).toContain('aria-label="Property type"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).toContain('<option value="multi_select">');
  });

  it("uses clear destructive property deletion confirmation copy", () => {
    expect(getPropertyDeleteConfirmationMessage("Priority")).toBe(
      'Delete property "Priority" from this space? Existing values will be removed from documents.',
    );
    expect(getPropertyDeleteConfirmationMessage("   ")).toBe(
      "Delete this property from the space? Existing values will be removed from documents.",
    );
  });
});
