import { describe, expect, it } from "vitest";
import { buildPageTemplateSnapshot, hasDocumentScopedTemplateAssets, loadPublishedTemplateSnapshot, readOriginalTemplateCss } from "./SaveAsTemplateDialog";
import { CUSTOM_CSS_START_MARKER, CUSTOM_CSS_END_MARKER, readCustomCss, replaceCustomCss } from "@/lib/documentContent";
import { templatePresentationAsRecord } from "@/lib/templates";
const document = { id: "page", title: "Recipe", icon: "🍲", published_at: "2026-10-07" };
const choices = {
  name: " Recipe ",
  description: " Family meals ",
  tags: false,
  properties: false,
  presentation: false,
};
describe("published page template snapshot", () => {
  it("rejects a publication that changes during snapshot loading and never reads a private draft", async () => {
    let reads = 0;
    const client = {
      getDocument: async () => ({ ...document, published_at: ++reads === 1 ? "first" : "second" }),
      getDocumentContent: async () => ({ id: "page", title: "Recipe", content: "Published body" }),
      listDocumentTags: async () => [],
      listDocumentProperties: async () => [],
    };
    await expect(loadPublishedTemplateSnapshot("space", "page", client)).rejects.toThrow("changed while loading");
    reads = 1;
    await expect(loadPublishedTemplateSnapshot("space", "page", client)).resolves.toMatchObject({ content: "Published body", document: { published_at: "second" } });
  });

  it("copies only requested defaults and preserves original content", () => {
    const source = "---\npage_width: wide\nprivate: false\n---\n\n## Ingredients\n\nFlour";
    const input = buildPageTemplateSnapshot(
      document,
      source,
      [{ id: "tag", name: "food", color: "" }],
      [{ property_id: "chef", property_name: "Chef", property_type: "text", value: "Eagle" }],
      choices,
    );
    expect(input).toMatchObject({
      name: "Recipe",
      description: "Family meals",
      tags: [],
      content: "## Ingredients\n\nFlour",
    });
    expect(input.properties).toBeUndefined();
    const included = buildPageTemplateSnapshot(
      document,
      source,
      [],
      [{ property_id: "chef", property_name: "Chef", property_type: "text", value: "Eagle" }],
      { ...choices, properties: true, presentation: true },
    );
    expect(included.properties).toMatchObject({
      Chef: "Eagle",
      __starlane_presentation: { page_width: "wide", icon: "🍲" },
    });
    expect(source).toContain("private: false");
  });
  it.each([
    "", " \r\n\t ", "\r\n.callout { color: var(--primary); }\r\n",
    "\0@import 'https://example.invalid/original.css';\r\n.notice { background: url(https://example.invalid/image); }",
    ".notice { color: blue; }".repeat(1000),
  ])("copies the exact original CSS payload into portable presentation metadata: %j", (css) => {
    const source = `Body\n${CUSTOM_CSS_START_MARKER}${css}${CUSTOM_CSS_END_MARKER}`;
    const input = buildPageTemplateSnapshot(document, source, [], [], { ...choices, presentation: true });
    const portable = JSON.parse(JSON.stringify(input));
    expect(templatePresentationAsRecord(portable.properties).custom_css).toBe(css);
    expect(readOriginalTemplateCss(source)).toBe(css);
    expect(input.content).toBe("Body");
    expect(source).toBe(`Body\n${CUSTOM_CSS_START_MARKER}${css}${CUSTOM_CSS_END_MARKER}`);
    const inherited = replaceCustomCss("New page", css);
    expect(readCustomCss(inherited).length).toBeLessThanOrEqual(20_000);
    if (css.includes("@import")) expect(inherited).toBe("New page");
    if (css.includes("var(--primary)")) expect(readCustomCss(inherited)).toBe(".callout { color: var(--primary); }");
  });

  it("distinguishes absent CSS from empty originals and follows the presentation choice", () => {
    expect(readOriginalTemplateCss("Body")).toBeUndefined();
    const absent = buildPageTemplateSnapshot({ ...document, icon: undefined }, "Body", [], [], { ...choices, presentation: true });
    expect(absent.properties).toBeUndefined();
    const source = `Body\n${CUSTOM_CSS_START_MARKER}blocked original${CUSTOM_CSS_END_MARKER}`;
    const excluded = buildPageTemplateSnapshot(document, source, [], [], choices);
    expect(excluded).toMatchObject({ content: "Body" });
    expect(excluded.properties).toBeUndefined();
    expect(readOriginalTemplateCss("<!-- \r\nSTARLANE:CUSTOM-CSS \t-->raw\r\n<!-- /STARLANE:CUSTOM-CSS \t-->" )).toBe("raw\r\n");
  });

  it.each([
    `${CUSTOM_CSS_START_MARKER}one${CUSTOM_CSS_END_MARKER}${CUSTOM_CSS_START_MARKER}two${CUSTOM_CSS_END_MARKER}`,
    `${CUSTOM_CSS_START_MARKER}outer${CUSTOM_CSS_START_MARKER}inner${CUSTOM_CSS_END_MARKER}${CUSTOM_CSS_END_MARKER}`,
    `${CUSTOM_CSS_START_MARKER}missing end`,
    `${CUSTOM_CSS_END_MARKER}missing start`,
    `${CUSTOM_CSS_END_MARKER}reversed${CUSTOM_CSS_START_MARKER}`,
    "<!-- starlane:custom-css missing close delimiter",
  ])("rejects ambiguous presentation copies before creating a template: %s", (source) => {
    expect(() => buildPageTemplateSnapshot(document, source, [], [], { ...choices, presentation: true })).toThrow("ambiguous CSS markers");
  });

  it("blocks document-owned assets while permitting portable external images", () => {
    expect(hasDocumentScopedTemplateAssets("![picture](https://example.com/assets/photo.png)")).toBe(
      false,
    );
    for (const source of [
      "![photo](starlane-attachment://abc)",
      "asset://local",
      "blob:test",
      "/api/attachments/abc",
      "https://notes.example.com/api/v1/files/spaces/recipes/attachments/page/photo.png",
    ])
      expect(() => buildPageTemplateSnapshot(document, source, [], [], choices)).toThrow(
        "document-owned assets",
      );
  });
});
