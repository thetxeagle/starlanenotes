import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { TemplateDraftRecord, TemplateRecord } from "@/lib/dataClient";
import {
  buildTemplateEditorInput,
  getTemplateEditorDraft,
  hydrateTemplateEditorDraft,
  hasRetainedTemplateCss,
  TemplateCssRecoveryDialog,
  setTemplatePropertyDefault,
} from "./TemplateEditorPage";
import { getTemplateEditorPath } from "@/lib/templates";

vi.mock("@/components/editor/Editor", () => ({ Editor: () => null, highlightedCodeToHtml: (value: string) => value }));
vi.mock("@/components/editor/MarkdownPreview", () => ({ MarkdownPreview: () => null }));

describe("TemplateEditorPage helpers", () => {
  it("keeps CSS recovery available for nonempty retained originals without an empty opener", () => {
    const blank = { name: "New", description: "", content: "Body", tags: "", properties: "", customCss: "" };
    expect(hasRetainedTemplateCss(blank)).toBe(false);
    expect(hasRetainedTemplateCss({ ...blank, customCss: " \n " })).toBe(true);
    expect(hasRetainedTemplateCss({ ...blank, reservedMetadata: { __starlane_presentation: { custom_css: "" } } })).toBe(false);
    expect(hasRetainedTemplateCss({ ...blank, reservedMetadata: { __starlane_presentation: { icon: "🚀" } } })).toBe(false);
  });

  it("renders stored CSS as selectable read-only text without authoring controls", () => {
    const value = '\n@import "https://example.com/original.css"; <style>legacy</style>\n';
    const html = renderToStaticMarkup(<TemplateCssRecoveryDialog value={value} onClose={() => undefined} />);
    expect(html).toContain('aria-label="Retained template CSS"');
    expect(html.toLowerCase()).toContain('readonly=""');
    expect(html).toContain("@import &quot;https://example.com/original.css&quot;; &lt;style&gt;legacy&lt;/style&gt;");
    expect(html).not.toContain("<style>legacy</style>");
    expect(html).not.toContain("Save CSS");
    expect(html).not.toContain("Indent CSS");
    expect(html).not.toContain("Outdent CSS");
  });


  it("keeps writing in the document workspace with on-demand template defaults", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/TemplateEditorPage.tsx"), "utf8");

    expect(source).toContain('className="starlane-document-viewport relative h-full min-h-0 flex-1 overflow-y-auto"');
    expect(source).toContain('aria-labelledby="template-editor-name"');
    expect(source).not.toContain('sticky top-0 z-30 flex min-h-14');
    expect(source).not.toContain('aria-label="Back to templates"');
    expect(source).toContain('<Breadcrumb spaceName={spaceName} title={draft.name || "Template"}');
    expect(source).toContain('labelledBy="template-tag-picker-title"');
    expect(source).toContain('labelledBy="template-property-picker-title"');
    expect(source).toContain('<CoverImage');
  });

  it("shares the document viewport, width hooks and persistent toolbar host", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/TemplateEditorPage.tsx"), "utf8");

    expect(source).toContain("getDocumentPageWidthClass(pageWidth)");
    expect(source).toContain('className="starlane-document-viewport relative h-full min-h-0 flex-1 overflow-y-auto"');
    expect(source).toContain("data-document-page-width={pageWidth}");
    expect(source).toContain('data-document-editing={viewMode === "edit" && !lockLost ? "true" : undefined}');
    expect(source).toContain("starlane-document-scope mx-auto w-full");
    expect(source).not.toContain("starlane-document-width-scope");
    expect(source).toContain("desktopToolbarHost={!mobileLayout ? desktopToolbarHost : null}");
    expect(source).toContain("ref={setDesktopToolbarHost}");
    expect(source).toContain("<DocumentContents");
  });

  it("focuses template Markdown source on desktop when Source view opens", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/TemplateEditorPage.tsx"), "utf8");

    expect(source).toContain('id="template-editor-source"');
    expect(source).toContain("autoFocus={!mobileLayout}");
  });

  it("keeps template save and lock state visible while collapsing diagnostics", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/TemplateEditorPage.tsx"), "utf8");

    expect(source).toContain('aria-label="Template status"');
    expect(source).toContain('aria-live="polite"');
    expect(source).toContain('lockLost ? "Read-only: editing lock lost"');
    expect(source).toContain('role="alert"');
    expect(source).toContain("This template is no longer locked to your editing session.");
    expect(source).toContain('<details className="border-border border-t px-5 py-3 sm:px-6">');
    expect(source).toContain(">Advanced details</span>");
    expect(source).toContain("Draft ID {templateDraft.id}");
    expect(source).toContain("Editing lock until");
    expect(source).not.toContain('<details open className="border-border border-t pt-2">');
  });

  it("labels the primary desktop template controls and keeps mobile controls touch sized", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/TemplateEditorPage.tsx"), "utf8");

    expect(source).toContain('["preview", "Read", Eye]');
    expect(source).toContain('["edit", "Edit", Pencil]');
    expect(source).toContain('["source", "Source view", Code2]');
    expect(source).toContain('aria-label="Template controls"');
    expect(source).toContain('mobileLayout ? "sr-only" : "text-xs"');
    expect(source).toContain('mobileLayout ? "h-11 w-11" : "h-8 px-2"');
    expect(source).toContain('<span className={mobileLayout ? "sr-only" : "text-xs"}>{saving ? "Saving" : "Save"}</span>');
    expect(source).toContain('<span className={mobileLayout ? "sr-only" : "text-xs"}>Revisions</span>');
  });

  it("keeps template details fluid and removes page width controls and summaries", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/TemplateEditorPage.tsx"), "utf8");

    expect(source).not.toContain("<DocumentWidthPicker");
    expect(source).not.toContain("<dt>Page width</dt>");
    expect(source).toContain('className="min-h-0 overflow-y-auto p-0"');
    expect(source).toContain('className="border-border border-b py-5" aria-labelledby="template-fields-heading"');
    expect(source).toContain('className="border-border grid min-w-0 gap-3 border-t pt-4"');
    expect(source).not.toContain('aria-labelledby="template-fields-heading">\n                <div className="mb-3 flex items-start justify-between gap-3">');
  });

  it("normalizes editor metadata without replacing the page body", () => {
    expect(
      buildTemplateEditorInput({
        name: "  Project brief  ",
        description: "  Reusable brief  ",
        content: "# Scope\n",
        tags: "project, planning, project",
        properties: '{"status":"draft"}',
      }),
    ).toEqual({
      name: "Project brief",
      description: "Reusable brief",
      content: "# Scope\n",
      tags: ["project", "planning"],
      properties: { status: "draft" },
    });
  });

  it("keeps an explicit empty text default until the user removes the property", () => {
    expect(setTemplatePropertyDefault({ status: "draft" }, "Summary", "", true)).toEqual({
      status: "draft",
      Summary: "",
    });
    expect(setTemplatePropertyDefault({ Summary: "Old value" }, "Summary", "", false)).toEqual({});
  });

  it("keeps presentation settings with the template without exposing them as properties", () => {
    const input = buildTemplateEditorInput({
      name: "Project brief",
      description: "",
      content: "# Scope",
      tags: "",
      properties: '{"status":"draft"}',
      icon: "🚀",
      coverImage: "https://example.com/cover.jpg",
      customCss: ".callout { color: var(--primary); }",
    });

    expect(input.properties).toEqual({
      status: "draft",
      __starlane_presentation: {
        icon: "🚀",
        cover_image: "https://example.com/cover.jpg",
        custom_css: ".callout { color: var(--primary); }",
      },
    });
  });

  it.each([
    ["explicit empty", ""],
    ["surrounding whitespace", "  .callout { color: var(--primary); }\n "],
    ["whitespace only", " \t\r\n "],
    ["blocked external CSS", '@import "https://example.com/legacy.css"; .callout { background: url(https://example.com/image.png); }'],
    ["null character", ".callout { color: red; }\0"],
    ["over sanitizer limit", ".callout { color: red; }".repeat(1000)],
  ])("preserves raw %s CSS through ordinary template changes", (_label, customCss) => {
    const stored: TemplateRecord = {
      id: "template-1", space_id: "space-1", name: "Original", content: "Original body",
      tags: [], created_at: "2026-10-08T00:00:00Z", updated_at: "2026-10-08T00:00:00Z",
      properties: { __starlane_presentation: { custom_css: customCss } },
    };
    const draft = getTemplateEditorDraft(stored);
    expect(draft.customCss).toBe(customCss);
    for (const change of [{ name: "Renamed" }, { content: "Changed body" }, { pageWidth: "wide" as const }]) {
      const input = buildTemplateEditorInput({ ...draft, ...change });
      expect(((input.properties as Record<string, unknown>).__starlane_presentation as { custom_css: string }).custom_css).toBe(customCss);
      expect(getTemplateEditorDraft({ ...stored, ...input }).customCss).toBe(customCss);
    }
  });

  it("does not add a blank presentation envelope to a new template", () => {
    expect(buildTemplateEditorInput({ name: "New", description: "", content: "Body", tags: "", properties: "", customCss: "" }).properties).toBeUndefined();
  });

  it("preserves server draft CSS when a newer local draft only changes the title", async () => {
    const customCss = ' \n@import "https://example.com/legacy.css";\n ';
    const stored: TemplateRecord = {
      id: "template-1", space_id: "space-1", name: "Published", content: "Published body",
      tags: [], properties: {}, created_at: "2026-10-08T00:00:00Z", updated_at: "2026-10-08T00:00:00Z",
    };
    const serverDraft: TemplateDraftRecord = {
      id: "draft-1", template_id: stored.id, space_id: stored.space_id, owner_id: "user-1",
      snapshot: { name: "Server draft", content: "Draft body", tags: [], properties: { __starlane_presentation: { custom_css: customCss } } },
      created_at: "2026-10-08T00:01:00Z", updated_at: "2026-10-08T00:02:00Z", last_autosaved_at: "2026-10-08T00:02:00Z",
    };
    const result = await hydrateTemplateEditorDraft(stored, {
      savedAt: "2026-10-08T00:03:00Z", draft: { name: "Local title" },
    }, async () => serverDraft);
    expect(result.draft.name).toBe("Local title");
    expect(result.draft.customCss).toBe(customCss);
    expect(((buildTemplateEditorInput(result.draft).properties as Record<string, unknown>).__starlane_presentation as { custom_css: string }).custom_css).toBe(customCss);
  });

  it("persists field configuration and preserves Journal plus unknown envelope metadata", () => {
    const input = buildTemplateEditorInput({
      name: "Incident",
      description: "",
      content: "# Summary",
      tags: "",
      properties: '{"status":"draft"}',
      templateExperience: {
        schema_version: 1,
        fields: [{ id: "service", label: "Service", type: "text", required: true }],
        title: [{ kind: "text", value: "Incident — " }, { kind: "field", field_id: "service" }],
      },
      reservedMetadata: {
        __starlane_journal: { cadence: "weekly", enabled: true },
        __starlane_template: {
          schema_version: 1,
          fields: [
            { id: "service", label: "Old label", type: "text", future_field_key: "keep" },
            { id: "future-field", label: "Future", type: "unknown-v2-type", private_option: "keep" },
          ],
          future_envelope_key: { value: "keep" },
        },
      },
    });

    expect(input.properties).toMatchObject({
      status: "draft",
      __starlane_journal: { cadence: "weekly", enabled: true },
      __starlane_template: {
        schema_version: 1,
        fields: [
          { id: "future-field", label: "Future", type: "unknown-v2-type", private_option: "keep" },
          { id: "service", label: "Service", type: "text", required: true, future_field_key: "keep" },
        ],
        title: [{ kind: "text", value: "Incident — " }, { kind: "field", field_id: "service" }],
        future_envelope_key: { value: "keep" },
      },
    });
  });

  it("rejects invalid property metadata", () => {
    expect(() =>
      buildTemplateEditorInput({
        name: "Project brief",
        description: "",
        content: "",
        tags: "",
        properties: "[]",
      }),
    ).toThrow("Template properties must be a JSON object.");
  });

  it("hydrates saved templates and builds encoded editor routes", () => {
    expect(
      getTemplateEditorDraft({
        id: "template/one",
        space_id: "space/one",
        name: "Project brief",
        description: "Reusable brief",
        content: "# Scope",
        tags: ["project"],
        properties: { status: "draft" },
        created_at: "2026-07-26T00:00:00Z",
        updated_at: "2026-07-26T00:00:00Z",
      }),
    ).toMatchObject({
      name: "Project brief",
      content: "# Scope",
      tags: "project",
      properties: '{\n  "status": "draft"\n}',
    });
    expect(getTemplateEditorPath("space/one")).toBe("/spaces/space%2Fone/templates/new");
    expect(getTemplateEditorPath("space/one", "template/one")).toBe(
      "/spaces/space%2Fone/templates/template%2Fone/edit",
    );
  });

  it("hydrates Journal preferences and field metadata without exposing them as page defaults", () => {
    const draft = getTemplateEditorDraft({
      id: "template-meta",
      space_id: "space-1",
      name: "Incident",
      content: "# Summary",
      tags: [],
      properties: {
        status: "draft",
        __starlane_journal: { cadence: "weekly", enabled: true },
        __starlane_template: { schema_version: 1, fields: [{ id: "service", label: "Service", type: "text" }] },
      },
      created_at: "2026-07-26T00:00:00Z",
      updated_at: "2026-07-26T00:00:00Z",
    });

    expect(draft.properties).toBe('{\n  "status": "draft"\n}');
    expect(draft.templateExperience?.fields).toEqual([{ id: "service", label: "Service", type: "text" }]);
    expect(draft.reservedMetadata?.__starlane_journal).toEqual({ cadence: "weekly", enabled: true });
  });

  it("waits for server draft acquisition and preserves a newer local draft", async () => {
    const template: TemplateRecord = {
      id: "template-1",
      space_id: "space-1",
      name: "Published name",
      description: "Published description",
      content: "Published content",
      tags: [],
      properties: {},
      created_at: "2026-07-26T00:00:00.000Z",
      updated_at: "2026-07-26T00:00:00.000Z",
    };
    const serverDraft: TemplateDraftRecord = {
      id: "draft-1",
      template_id: template.id,
      space_id: template.space_id,
      owner_id: "user-1",
      snapshot: {
        name: "Server draft name",
        description: "Server draft description",
        content: "Server draft content",
        properties: { status: "server" },
        tags: [],
      },
      created_at: "2026-07-26T00:05:00.000Z",
      updated_at: "2026-07-26T00:30:00.000Z",
      last_autosaved_at: "2026-07-26T00:30:00.000Z",
    };
    let resolveServerDraft!: (draft: TemplateDraftRecord) => void;
    const serverRequest = new Promise<TemplateDraftRecord>((resolve) => {
      resolveServerDraft = resolve;
    });
    let settled = false;
    const hydrationPromise = hydrateTemplateEditorDraft(
      template,
      {
        savedAt: "2026-07-26T00:45:00.000Z",
        draft: {
          name: "Local working name",
          description: "Local working description",
          content: "Local working content",
          properties: '{\n  "status": "local"\n}',
        },
      },
      () => serverRequest,
    ).then((hydration) => {
      settled = true;
      return hydration;
    });

    await Promise.resolve();
    expect(settled).toBe(false);

    resolveServerDraft(serverDraft);
    const hydration = await hydrationPromise;

    expect(hydration.draft).toMatchObject({
      name: "Local working name",
      description: "Local working description",
      content: "Local working content",
      properties: '{\n  "status": "local"\n}',
    });
    expect(hydration.lastAutoSavedAt).toBe("2026-07-26T00:45:00.000Z");
  });
});
