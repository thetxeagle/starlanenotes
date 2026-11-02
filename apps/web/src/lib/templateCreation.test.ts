import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { TEMPLATE_PRESENTATION_KEY, type TemplatePresentation } from "./templates";
import { prepareTemplateCreation } from "./templateCreation";

const OPEN = "<!-- starlane:custom-css -->";
const CLOSE = "<!-- /starlane:custom-css -->";
const propertiesWithCss = (custom_css: string) => ({
  [TEMPLATE_PRESENTATION_KEY]: { custom_css } satisfies TemplatePresentation,
});

describe("template creation preflight", () => {
  it("keeps a retained body block byte-for-byte and does not interpolate inside it", () => {
    const rawBlock = `<!--\tSTARLANE:custom-css -->\r\n.notice::after { content: "{{title}}"; }\0\r\n<!-- /STARLANE:custom-css\t-->`;
    const result = prepareTemplateCreation({
      content: `# {{title}}\r\n${rawBlock}\r\nEnd`,
      context: { title: "Resolved" },
    });
    expect(result.kind).toBe("ready");
    if (result.kind !== "ready") return;
    expect(result.resolved.content).toBe(`# Resolved\r\n${rawBlock}\r\nEnd`);
  });

  it.each(["", "  \r\n.notice { color: red; }\0  "])(
    "inherits metadata-only CSS with its exact inner string (%j)",
    (css) => {
      const result = prepareTemplateCreation({
        content: "Body {{title}}",
        properties: propertiesWithCss(css),
        context: { title: "Resolved" },
      });
      expect(result).toMatchObject({ kind: "ready" });
      if (result.kind !== "ready") return;
      expect(result.resolved.content).toBe(`Body Resolved\n\n${OPEN}${css}${CLOSE}`);
    },
  );

  it("preserves the original block when body and metadata agree exactly", () => {
    const css = "\r\n.notice { content: '{{date}}'; }\r\n";
    const rawBlock = `${OPEN}${css}${CLOSE}`;
    const result = prepareTemplateCreation({
      content: `Before${rawBlock}After`,
      properties: propertiesWithCss(css),
      context: { date: new Date("2026-10-21T12:00:00Z") },
    });
    expect(result.kind).toBe("ready");
    if (result.kind !== "ready") return;
    expect(result.resolved.content).toBe(`Before${rawBlock}After`);
  });

  it("holds conflicting body and metadata CSS before returning creatable content", () => {
    expect(
      prepareTemplateCreation({
        content: `${OPEN}body${CLOSE}`,
        properties: propertiesWithCss("metadata"),
      }),
    ).toMatchObject({ kind: "held", reason: "conflicting_css" });
  });

  it("holds marker-shaped metadata instead of creating ambiguous source", () => {
    expect(
      prepareTemplateCreation({
        content: "Body",
        properties: propertiesWithCss(`before${CLOSE}after`),
      }),
    ).toMatchObject({ kind: "held", reason: "ambiguous_css" });
  });

  it.each([
    `${OPEN}one${CLOSE}${OPEN}two${CLOSE}`,
    `${OPEN}unclosed`,
    "<!-- starlane:custom-css",
  ])("holds ambiguous source without producing a creation payload (%#)", (content) => {
    const result = prepareTemplateCreation({ content });
    expect(result).toMatchObject({ kind: "held", reason: "ambiguous_css" });
    expect(result).not.toHaveProperty("resolved");
  });

  it("leaves templates with absent CSS metadata and markers on the normal resolver path", () => {
    const result = prepareTemplateCreation({ content: "# {{title}}", context: { title: "Page" } });
    expect(result).toMatchObject({ kind: "ready", resolved: { content: "# Page" } });
  });

  it.each([
    ["src/components/layout/Sidebar.tsx", "const createDocumentFromTemplate", "const handleCreateDocument"],
    ["src/components/mobile/MobileScreens.tsx", "const createDocumentFromTemplate", "const openNewDocument"],
    ["src/pages/JournalPage.tsx", "const openOrCreateEntry", "const selectEntry"],
  ])("preflights daily creation before the first folder or page mutation in %s", (path, start, end) => {
    const source = readFileSync(resolve(process.cwd(), path), "utf8");
    const flow = source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));
    const preflight = flow.indexOf("prepareTemplateCreation(");
    expect(preflight).toBeGreaterThanOrEqual(0);
    expect(flow.indexOf("dataClient.createFolder(")).toBeGreaterThan(preflight);
    expect(flow.indexOf("dataClient.createDocument(")).toBeGreaterThan(preflight);
    expect(flow).not.toContain("replaceCustomCss");
  });
});
