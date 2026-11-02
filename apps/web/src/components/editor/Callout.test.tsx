import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  CalloutHeader,
  normalizeCalloutKind,
  parseCalloutDirective,
  serializeCalloutDirective,
} from "./Callout";

describe("callout directives", () => {
  it("parses the portable GitHub-compatible alert form", () => {
    expect(parseCalloutDirective("> [!WARNING]\n> Confirm the migration before continuing.")).toEqual({
      kind: "warning",
      content: "Confirm the migration before continuing.",
    });
  });

  it("rejects malformed blocks and normalizes unknown callout kinds", () => {
    expect(parseCalloutDirective("> [!MAYBE]\n> Nope")).toBeNull();
    expect(parseCalloutDirective("> [!NOTE]\nPlain text")).toBeNull();
    expect(normalizeCalloutKind("custom")).toBe("note");
  });

  it("serializes a callout without changing its text", () => {
    expect(serializeCalloutDirective("tip", "Keep the source portable.\nIt matters.")).toBe(
      "> [!TIP]\n> Keep the source portable.\n> It matters.",
    );
  });

  it("uses semantic Starlane state colors and a named type selector", () => {
    const html = renderToStaticMarkup(
      createElement(CalloutHeader, { kind: "caution", editable: true, onKindChange: () => undefined }),
    );

    expect(html).toContain("Potentially harmful action");
    expect(html).toContain("text-danger");
    expect(html).toContain('aria-label="Callout type"');
  });
});
