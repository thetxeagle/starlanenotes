import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { Schema } from "prosemirror-model";
import { EditorState, TextSelection } from "prosemirror-state";
import {
  EDITOR_DANGER_MENU_ITEM_CLASS,
  EDITOR_IMAGE_OPTIONS,
  EDITOR_HIGHLIGHT_COLOR_OPTIONS,
  EDITOR_TEXT_COLOR_OPTIONS,
  DEFAULT_MOBILE_TOOLBAR_ACTIONS,
  EditorUploadWarning,
  SectionDocument,
  buildWikiDocumentHref,
  formatEditorUploadLimitWarning,
  MOBILE_EDITOR_TOOLBAR_ACTION_CLASS,
  MOBILE_EDITOR_TOOLBAR_BASE_CLASS,
  MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS,
  MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS,
  MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS,
  MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS,
  MOBILE_TABLE_TOOLBAR_BASE_CLASS,
  TABLE_EDITOR_TOOLBAR_SURFACE_CLASS,
  getMobileKeyboardInsetMeasurement,
  getMobileToolbarAction,
  getMobileToolbarPlacement,
  getBookmarkProvider,
  getBookmarkRepository,
  getToolbarPopoverPosition,
  isEditorSelectionObscured,
  isMobileViewportContracted,
  filterEditorFontGroups,
  getAvailableMobileToolbarActionIds,
  getEditorFontFamilyCount,
  getEditorToolbarActionTone,
  getSelectionToolbarTop,
  shouldShowSelectionToolbar,
  serializeFileAttachmentMarkdown,
  isCommandPaletteShortcut,
  getBlockMoveShortcut,
  markdownToHtml,
  parseCodeFenceInfo,
  parseWikiDocumentHref,
  serializeWikiLinkMarkdown,
  parseMarkdownTableBlock,
  serializeCodeFenceInfo,
  serializeMarkdownImage,
  serializeBookmarkCardDirective,
  splitMarkdownSectionSegments,
  stripRuntimeBlockIdentity,
  shouldApplyExternalEditorContent,
  shouldClaimEditorCanvasPointer,
  shouldRestoreEditorSelectionAfterKeyboardClose,
  createSiblingParagraphTransaction,
  isUploadMenuOwnedSelection,
} from "./Editor";
import { serializeTemplateMarker } from "@/lib/templateMarkers";
import { decodeTemplateValueHtmlText, serializeTemplateValue } from "@/lib/templateValues";
import {
  COLUMNS_END_MARKER,
  COLUMNS_START_MARKER,
  SECTION_COLUMN_MARKER,
  SECTION_COLUMN_MARKER_PREFIX,
  SECTION_END_MARKER,
  SECTION_LAYOUT_OPTIONS,
  SECTION_START_MARKER,
  EditorSection,
  EditorColumns,
  EditorSectionColumn,
  getSectionColumnCount,
  inferSectionLayoutFromColumnCount,
  getSectionLabelSlug,
  serializeEditorColumnsMarkdown,
  serializeEditorSectionMarkdown,
} from "./EditorSection";
import { TABLE_OF_CONTENTS_DIRECTIVE } from "./TableOfContents";
import { serializeCalloutDirective } from "./Callout";
import { parseExpandDirective, serializeExpandDirective } from "./Expand";
import { serializeMathBlockDirective } from "./MathBlock";
import { normalizeImageCaption } from "./ImageBlock";
import {
  normalizeTableStyle,
  parseTableStyleDirective,
  serializeTableStyleDirective,
} from "./tableStyles";

describe("markdownToHtml", () => {
  it("renders structured guidance and field markers without deleting similar prose", () => {
    const guidance = serializeTemplateMarker({
      kind: "guidance",
      id: "impact_prompt",
      text: "Describe the user impact. Include approximate duration.",
    });
    const field = serializeTemplateMarker({ kind: "field", id: "service_slot", field_id: "service" });
    const source = ["Literal comment-like text stays.", guidance, field].join("\n\n");
    const html = markdownToHtml(source, false, [{ id: "service", label: "Service" }]);

    expect(html).toContain('data-starlane-template-marker="guidance"');
    expect(html).toContain('data-starlane-template-marker="field"');
    expect(html).toContain('data-template-marker-field-label="Service"');
    expect(html).toContain("Literal comment-like text stays.");
    expect(html).toContain('data-template-marker-id="impact_prompt"');
    expect(html).toContain('data-template-marker-id="service_slot"');
  });

  it("keeps guidance marker examples inside fenced and indented code inert", () => {
    const guidance = serializeTemplateMarker({ kind: "guidance", id: "example", text: "Explain it" });
    const fenced = `\`\`\`md\n${guidance}\n\`\`\``;
    const indented = `    ${guidance}`;
    const tabIndented = `  \t${guidance}`;

    expect(markdownToHtml(fenced)).not.toContain('data-starlane-template-marker="guidance"');
    expect(markdownToHtml(indented)).not.toContain('data-starlane-template-marker="guidance"');
    expect(markdownToHtml(tabIndented)).not.toContain('data-starlane-template-marker="guidance"');
    expect(markdownToHtml(fenced)).toContain("starlane-template:v1:guidance");
    expect(markdownToHtml(indented)).toContain("starlane-template:v1:guidance");
    expect(markdownToHtml(tabIndented)).toContain("starlane-template:v1:guidance");
  });

  it("renders hostile and multiline resolved field values as escaped literal HTML", () => {
    const value = '**literal** <img src=x onerror="alert(1)"> "quoted"\\path\nsecond line {{date}} — café';
    const source = `Before ${serializeTemplateValue(value)} after`;
    const html = markdownToHtml(source);

    expect(html).toContain('data-starlane-template-value="v1"');
    expect(html).toContain("&lt;img src=x onerror=");
    expect(html).not.toContain('<img src=x');
    const serializedValue = html.match(/data-starlane-template-value="v1"[^>]*>([\s\S]*?)<\/span>/)?.[1] || "";
    expect(decodeTemplateValueHtmlText(serializedValue)).toBe(value);
    expect(html).toContain("second line {{date}}");
    expect(html).toContain("café");
    expect(html.match(/<img\b/g)).toBeNull();
  });

  it("keeps uploaded images as attachment references instead of embedding base64 bytes", () => {
    const html = markdownToHtml("![Screenshot](starlane-attachment://attachment-123)");

    expect(html).toContain('data-attachment-id="attachment-123"');
    expect(html).toContain('<img src=""');
  });

  it("keeps an immediate image preview when the parent echoes the same attachment Markdown", () => {
    const attachmentMarkdown = "![Screenshot](starlane-attachment://attachment-123)";

    expect(shouldApplyExternalEditorContent(attachmentMarkdown, attachmentMarkdown)).toBe(false);
  });

  it("still applies genuinely different external editor content", () => {
    expect(
      shouldApplyExternalEditorContent("Current local content", "Replacement from another source"),
    ).toBe(true);
  });

  it("inserts image nodes atomically and keeps shallow section selections safe", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain('type: "image"');
    expect(source).toContain("attrs: { src: assetUrl, alt: assetName, attachmentId }");
    expect(source).toContain("resolveMovableBlock(editor.state)");
  });

  it("accepts ordinary blocks and structural containers without a transaction repair loop", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(SectionDocument.config.topNode).toBe(true);
    expect(SectionDocument.config.content).toBe("block+");
    expect(source).toContain("document: false");
    expect(source).not.toContain("appendTransaction:");
  });

  it("opens ordinary Markdown directly on the writing canvas", () => {
    const html = markdownToHtml("## Legacy page\n\nStill here.");

    expect(html).not.toContain('data-starlane-section="true"');
    expect(html).toContain("<h2>Legacy page</h2>");
    expect(html).toContain("<p>Still here.</p>");
  });

  it("keeps runtime SlashEditor block identity out of canonical Markdown input", () => {
    expect(
      stripRuntimeBlockIdentity(
        '<section data-starlane-section="true"><p data-block-id="block-1" data-block-type="paragraph">Mission</p></section>',
      ),
    ).toBe('<section data-starlane-section="true"><p>Mission</p></section>');
  });

  it("mounts runtime block identity without enabling drag before its UI slice", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("...createStarlaneBlockInteractionExtensions(),");
    expect(source).not.toContain("blockDrag(");
  });

  it("unwraps a plain legacy writing section without changing its content", () => {
    const markdown = [
      "Before",
      "",
      SECTION_START_MARKER,
      "",
      "## Mission",
      "",
      "Keep the blocks together.",
      "",
      SECTION_END_MARKER,
      "",
      "After",
    ].join("\n");

    const html = markdownToHtml(markdown);

    expect(html).toContain("<p>Before</p>");
    expect(html).not.toContain('data-starlane-section="true"');
    expect(html).toContain("<h2>Mission</h2>");
    expect(html).toContain("<p>Keep the blocks together.</p>");
    expect(html).toContain("<p>After</p>");
  });

  it("preserves styled, nested, and multi-column legacy sections", () => {
    const styled = markdownToHtml(
      "<!-- starlane:section border=#2563eb -->\n\nStyled\n\n<!-- /starlane:section -->",
    );
    const nested = markdownToHtml(
      serializeEditorSectionMarkdown(serializeEditorSectionMarkdown("Nested")),
    );
    const multiColumn = markdownToHtml(
      serializeEditorSectionMarkdown(["Left", "Right"], "two-equal"),
    );

    expect(styled).toContain('data-starlane-section="true"');
    expect(nested.match(/data-starlane-section="true"/g)).toHaveLength(2);
    expect(multiColumn).toContain('data-layout="two-equal"');
    expect(multiColumn.match(/data-starlane-section-column="true"/g)).toHaveLength(2);
  });

  it("round-trips sanitized section colors through portable markers", () => {
    const html = markdownToHtml(
      "<!-- starlane:section border=#2563eb background=#111827 -->\n\nStyled\n\n<!-- /starlane:section -->",
    );
    expect(html).toContain('data-border-color="#2563eb"');
    expect(html).toContain('data-background-color="#111827"');
    expect(html).toContain("border-color:#2563eb");
  });

  it("does not add a default background when only the section border changes", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorSection.tsx"),
      "utf8",
    );

    expect(source).toContain("storedBackgroundColor");
    expect(source).toContain("backgroundColor: storedBackgroundColor");
    expect(source).toContain("borderColor: storedBorderColor");
  });

  it("preserves the text color toolbar action in the default mobile set", () => {
    expect(DEFAULT_MOBILE_TOOLBAR_ACTIONS).toContain("color");
  });

  it("keeps text color black available and uses theme-aware highlight tokens", () => {
    expect(EDITOR_TEXT_COLOR_OPTIONS).toContainEqual({ label: "Black", value: "#000000" });
    expect(EDITOR_HIGHLIGHT_COLOR_OPTIONS.map(({ value }) => value)).toContain(
      "var(--editor-highlight-yellow)",
    );
  });

  it("keeps section markers inside code fences as code and supports nested sections", () => {
    const nested = serializeEditorSectionMarkdown(
      ["Outer", "", serializeEditorSectionMarkdown("Inner")].join("\n"),
    );
    const fenced = ["```markdown", SECTION_START_MARKER, SECTION_END_MARKER, "```"].join("\n");

    expect(splitMarkdownSectionSegments(fenced)).toEqual([{ type: "markdown", content: fenced }]);
    expect(markdownToHtml(nested).match(/data-starlane-section="true"/g)).toHaveLength(2);
  });

  it("serializes section content with portable invisible boundaries", () => {
    expect(serializeEditorSectionMarkdown("## Mission\n\nKeep moving.")).toBe(
      [
        "<!-- starlane:section layout=single -->",
        "",
        "## Mission",
        "",
        "Keep moving.",
        "",
        SECTION_END_MARKER,
      ].join("\n"),
    );
  });

  it("round-trips every approved section column layout without hiding ordinary Markdown", () => {
    for (const option of SECTION_LAYOUT_OPTIONS) {
      const columnCount = getSectionColumnCount(option.id);
      const columns = Array.from(
        { length: columnCount },
        (_, index) => `## Column ${index + 1}\n\nContent ${index + 1}`,
      );
      const markdown = serializeEditorSectionMarkdown(columns, option.id);
      const html = markdownToHtml(markdown);

      if (option.id === "single") {
        expect(html).not.toContain('data-starlane-section="true"');
      } else {
        expect(html).toContain(`data-layout="${option.id}"`);
        expect(html).toContain("--section-grid-template-columns:");
      }
      expect(html.match(/data-starlane-section-column="true"/g) ?? []).toHaveLength(
        option.id === "single" ? 0 : columnCount,
      );
      for (const column of columns) {
        expect(markdown).toContain(column);
      }
      if (columnCount > 1) {
        expect(markdown.match(new RegExp(SECTION_COLUMN_MARKER_PREFIX, "g"))).toHaveLength(
          columnCount - 1,
        );
      }
    }
  });

  it("round-trips first-class portable column blocks without section styling", () => {
    const markdown = serializeEditorColumnsMarkdown(
      ["## Left\n\nAlpha", "## Right\n\nBravo"],
      "two-equal",
    );
    const html = markdownToHtml(markdown);

    expect(markdown).toContain(COLUMNS_START_MARKER);
    expect(markdown).toContain(COLUMNS_END_MARKER);
    expect(markdown).not.toContain("starlane:section");
    expect(html).toContain('data-starlane-columns="true"');
    expect(html).not.toContain('data-starlane-section="true"');
    expect(html.match(/data-starlane-section-column="true"/g)).toHaveLength(2);
    expect(html).toContain("<h2>Left</h2>");
    expect(html).toContain("<h2>Right</h2>");
  });

  it("recovers legacy column markers that were saved without a layout attribute", () => {
    const legacy = [
      "<!-- starlane:section -->",
      "",
      "# Overview",
      "",
      "# Prerequisites",
      "",
      SECTION_COLUMN_MARKER,
      "",
      "column?",
      "",
      SECTION_END_MARKER,
    ].join("\n");

    expect(inferSectionLayoutFromColumnCount(2)).toBe("two-equal");
    const html = markdownToHtml(legacy);
    expect(html).toContain('data-layout="two-equal"');
    expect(html.match(/data-starlane-section-column="true"/g)).toHaveLength(2);
  });

  it("keeps newly selected column lanes visible while they are empty", () => {
    const css = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");
    const sectionSource = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorSection.tsx"),
      "utf8",
    );
    expect(css).toContain(".tiptap-editor .editor-section__column,");
    expect(css).toContain(
      '[data-starlane-section="true"]\n  > .editor-section__content\n  > [data-node-view-wrapper]',
    );
    expect(css).toContain("min-height: 3rem;");
    expect(css).toContain("newly-created lane collapses");
    expect(sectionSource).toContain(
      '"--section-grid-template-columns": getSectionGridTemplateColumns(layout)',
    );
    expect(sectionSource).toContain("currentColumns.length === 1 && targetCount > 1");
    expect(sectionSource).toContain(
      "distributeSectionBlocks(editor, getSectionColumnBlocks(currentColumns), targetCount)",
    );
  });

  it("keeps the default single-column section visually open without hiding authored sections", () => {
    const css = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");
    const sectionSource = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorSection.tsx"),
      "utf8",
    );

    expect(sectionSource).toContain(
      'data-plain-writing-section={plainWritingSection ? "true" : undefined}',
    );
    expect(sectionSource).toContain('layout === "single"');
    expect(sectionSource).toContain("!headerText.trim()");
    expect(css).toContain('[data-plain-writing-section="true"]');
    expect(css).toContain(":not([data-content-background-color]):not([data-header-text])");
    expect(css).toContain("border-color: transparent;");
    expect(css).toContain("background: transparent;");
  });

  it("exposes labeled sections as stable CSS hooks", () => {
    const html = markdownToHtml(
      "<!-- starlane:section header=Project%20Notes -->\n\nContent\n\n<!-- /starlane:section -->",
    );

    expect(getSectionLabelSlug("Project Notes")).toBe("project-notes");
    expect(html).toContain('data-section-label="Project Notes"');
    expect(html).toContain('data-section-label-slug="project-notes"');
  });

  it("does not treat column markers inside fenced code as section dividers", () => {
    const markdown = serializeEditorSectionMarkdown(
      ["```markdown", SECTION_COLUMN_MARKER, "```"].join("\n"),
      "two-equal",
    );
    const html = markdownToHtml(markdown);

    expect(html.match(/data-starlane-section-column="true"/g)).toHaveLength(2);
    expect(html).toContain("starlane:column");
  });

  it("preserves an empty heading while the user begins a new sibling block", () => {
    expect(markdownToHtml("##")).toContain("<h2></h2>");
  });

  it("uses the polished directory presentation for ordinary Markdown tables", () => {
    const html = markdownToHtml(
      ["| Name | Status |", "| --- | --- |", "| Church | Active |"].join("\n"),
    );

    expect(html).toContain('<table data-table-style="directory">');
  });

  it("parses a portable table-style directive without changing the Markdown table", () => {
    const markdown = [
      "<!-- starlane-table: striped -->",
      "| Name | Status |",
      "| --- | --- |",
      "| Church | Active |",
    ].join("\n");

    expect(parseMarkdownTableBlock(markdown)).toEqual({
      markdown: ["| Name | Status |", "| --- | --- |", "| Church | Active |"].join("\n"),
      style: "striped",
    });
    expect(markdownToHtml(markdown)).toContain('<table data-table-style="striped">');
  });

  it("parses portable display math into an editable math block", () => {
    const html = markdownToHtml(serializeMathBlockDirective("E = mc^2"));

    expect(html).toContain('data-math-block="true"');
    expect(html).toContain('data-expression="E = mc^2"');
  });

  it("parses a portable callout directive into its editable callout block", () => {
    const html = markdownToHtml(
      serializeCalloutDirective("warning", "Review this before deploying."),
    );

    expect(html).toContain('data-callout="true"');
    expect(html).toContain('data-callout-kind="warning"');
    expect(html).toContain("Review this before deploying.");
  });

  it("parses and serializes collapsible expand blocks", () => {
    const markdown = serializeExpandDirective("Deployment notes", "Keep this section compact.");
    const html = markdownToHtml(markdown);

    expect(parseExpandDirective(markdown)).toEqual({
      title: "Deployment notes",
      content: "Keep this section compact.",
    });
    expect(html).toContain('data-expand-block="true"');
    expect(html).toContain('data-expand-title="Deployment notes"');
    expect(html).toContain("Keep this section compact.");
  });

  it("parses the portable generated table of contents directive into its block node", () => {
    expect(markdownToHtml(TABLE_OF_CONTENTS_DIRECTIVE)).toContain('data-table-of-contents="true"');
  });

  it("renders normal and collapsible child-page directives as explicit block metadata", () => {
    const html = markdownToHtml("[[children]]\n\n[[children:collapsed]]");
    expect(html).toContain('data-child-pages-variant="list"');
    expect(html).toContain('data-child-pages-variant="collapsible"');
  });

  it("uses semantic danger tokens for editor destructive context actions", () => {
    expect(EDITOR_DANGER_MENU_ITEM_CLASS).toContain("text-danger");
    expect(EDITOR_DANGER_MENU_ITEM_CLASS).toContain("hover:bg-danger");
    expect(EDITOR_DANGER_MENU_ITEM_CLASS).not.toContain("red-");
  });

  it("formats upload-limit warnings for in-editor feedback", () => {
    expect(formatEditorUploadLimitWarning("archive.zip", 25)).toBe(
      "archive.zip is larger than the current 25 MB upload limit.",
    );
  });

  it("renders upload-limit warnings as Starlane in-editor feedback", () => {
    const html = renderToStaticMarkup(
      createElement(EditorUploadWarning, {
        message: "archive.zip is larger than the current 25 MB upload limit.",
        onDismiss: () => undefined,
      }),
    );

    expect(html).toContain('role="alert"');
    expect(html).toContain("border-warning");
    expect(html).toContain("bg-warning");
    expect(html).toContain("text-warning");
    expect(html).toContain('aria-label="Dismiss upload warning"');
    expect(html).not.toMatch(/alert\(|window\.alert/);
  });

  it("preserves fenced code blocks with blank lines and indentation", () => {
    const html = markdownToHtml(
      [
        "```powershell",
        "function Test-Thing {",
        "  if ($true) {",
        '    Write-Host "ok"',
        "  }",
        "",
        '  Write-Host "done"',
        "}",
        "```",
      ].join("\n"),
    );

    expect(html).toContain('<code class="language-powershell">');
    const codeText = html.replace(/<[^>]+>/g, "");
    expect(codeText).toContain("  if ($true) {");
    expect(codeText).toContain("\n\n  Write-Host");
  });

  it("supports hyphenated code fence language names", () => {
    const html = markdownToHtml("```objective-c\n#import <Foundation/Foundation.h>\n```");

    expect(html).toContain('class="language-objective-c"');
    expect(html).toContain("#import &lt;Foundation/Foundation.h&gt;");
  });

  it("preserves an optional portable code-block name alongside its language", () => {
    const info = serializeCodeFenceInfo("typescript", "workspace.ts");
    const parsed = parseCodeFenceInfo(info);
    const html = markdownToHtml(`\`\`\`${info}\nexport const ready = true;\n\`\`\``);

    expect(parsed).toEqual({ language: "typescript", title: "workspace.ts" });
    expect(html).toContain('data-code-title="workspace.ts"');
    expect(html).toContain('class="language-typescript"');
  });

  it.each(["left", "right", "expand"])(
    "preserves %s code placement with a named fence",
    (placement) => {
      const info = serializeCodeFenceInfo("typescript", "workspace.ts", placement);
      expect(parseCodeFenceInfo(info)).toEqual({
        language: "typescript",
        title: "workspace.ts",
        placement,
      });
      expect(markdownToHtml(`\`\`\`${info}\nconst ready = true;\n\`\`\``)).toContain(
        `data-code-placement="${placement}"`,
      );
    },
  );

  it.each([undefined, "notes"])("preserves placement without a language, with title %s", (title) => {
    const info = serializeCodeFenceInfo("", title, "right");
    expect(parseCodeFenceInfo(info)).toEqual({ language: "", ...(title ? { title } : {}), placement: "right" });
    expect(markdownToHtml(`\`\`\`${info}\nplain words\n\`\`\``)).toContain('data-code-placement="right"');
  });

  it("keeps legacy fences compact and omits default placement metadata", () => {
    expect(serializeCodeFenceInfo("bash", null, "center")).toBe("bash");
    expect(serializeCodeFenceInfo("bash", null, "bogus")).toBe("bash");
    expect(parseCodeFenceInfo("bash placement=left")).toEqual({
      language: "bash",
      placement: "left",
    });
  });

  it.each(["left", "right"])("hydrates %s table placement without changing cells", (placement) => {
    const table = "| Name | Status |\n| --- | --- |\n| Church | Active |";
    const markdown = `<!-- starlane-table: striped align=${placement} -->\n${table}`;
    expect(parseMarkdownTableBlock(markdown)).toEqual({
      markdown: table,
      style: "striped",
      placement,
    });
    expect(markdownToHtml(markdown)).toContain(`data-table-placement="${placement}"`);
    expect(markdownToHtml(markdown)).toContain("Church");
  });

  it("hydrates Carousel 7 fences into an editor block", () => {
    const html = markdownToHtml(
      [
        "```carousel",
        "title: Screenshots",
        "---",
        "![One](https://example.com/one.png)",
        "```",
      ].join("\n"),
    );

    expect(html).toContain('data-carousel-block="true"');
    expect(html).toContain('data-carousel-title="Screenshots"');
    expect(html).toContain("https://example.com/one.png");
  });

  it("hydrates gallery fences into an editable gallery block", () => {
    const html = markdownToHtml(
      [
        "```gallery",
        "layout: masonry",
        "title: Screenshots",
        "---",
        "![One](https://example.com/one.png)",
        "```",
      ].join("\n"),
    );

    expect(html).toContain('data-gallery-block="true"');
    expect(html).toContain('data-gallery-layout="masonry"');
    expect(html).toContain('data-gallery-title="Screenshots"');
  });

  it("parses images before links", () => {
    const html = markdownToHtml("![Alt text](starlane://asset/logo.png)");

    expect(html).toContain('<img src="starlane://asset/logo.png" alt="Alt text"');
    expect(html).not.toContain("<a href=");
  });

  it("keeps native attachment references in portable Markdown", () => {
    expect(
      serializeFileAttachmentMarkdown(
        "diagram.png",
        "asset://localhost/%2Fvault%2Fattachments%2Fdiagram.png",
        51200,
      ),
    ).toBe(
      '[diagram.png](asset://localhost/%2Fvault%2Fattachments%2Fdiagram.png "starlane:size=51200")',
    );
    expect(serializeFileAttachmentMarkdown("notes.txt")).toBe("notes.txt");
    expect(
      markdownToHtml(
        '[diagram.png](asset://localhost/%2Fvault%2Fattachments%2Fdiagram.png "starlane:size=51200")',
      ),
    ).toContain('data-size="51200"');
  });

  it("preserves server attachment size metadata and supports legacy references", () => {
    const markdown = serializeFileAttachmentMarkdown(
      "report.pdf",
      "starlane-attachment://attachment-123",
      50123,
    );
    const html = markdownToHtml(markdown);
    const legacyHtml = markdownToHtml("[legacy.pdf](starlane-attachment://legacy-attachment)");

    expect(markdown).toBe(
      '[report.pdf](starlane-attachment://attachment-123 "starlane:size=50123")',
    );
    expect(html).toContain('data-attachment-id="attachment-123"');
    expect(html).toContain('data-size="50123"');
    expect(html).toContain("48.9 KB");
    expect(html).toContain('data-file-attachment-download="true"');
    expect(html).toContain('aria-label="Download report.pdf"');
    expect(legacyHtml).toContain('data-attachment-id="legacy-attachment"');
    expect(legacyHtml).toContain('data-size="0"');
  });

  it("keeps image captions in the portable Markdown alt field", () => {
    const caption = normalizeImageCaption("  Deployment\n preview  ");
    const html = markdownToHtml(`![${caption}](starlane://asset/deployment.png)`);

    expect(caption).toBe("Deployment preview");
    expect(html).toContain('alt="Deployment preview"');
  });

  it("round-trips Starlane image alignment through portable Markdown metadata", () => {
    const markdown =
      '![Deployment diagram](starlane://asset/deployment.png "starlane:align=center")';
    const html = markdownToHtml(markdown);

    expect(html).toContain('data-starlane-align="center"');
    expect(
      serializeMarkdownImage("starlane://asset/deployment.png", "Deployment diagram", "center"),
    ).toBe(markdown);
  });

  it("parses inline markdown links", () => {
    const html = markdownToHtml("Read [the docs](https://example.com/docs).");

    expect(html).toContain('<a href="https://example.com/docs"');
    expect(html).toContain('class="starlane-link"');
    expect(html).toContain('target="_blank"');
    expect(html).toContain('rel="noopener noreferrer"');
    expect(html).toContain(">the docs</a>");
  });

  it("renders stable wiki links as same-space document links and round-trips their IDs", () => {
    const markdown = "Continue in [[Release plan|doc-123]].";
    const html = markdownToHtml(markdown);

    expect(html).toContain('href="starlane:document/doc-123"');
    expect(html).toContain('class="starlane-link starlane-wiki-link"');
    expect(html).toContain('target="_self"');
    expect(html).not.toContain('target="_blank"');
    expect(serializeWikiLinkMarkdown("Release plan", "doc-123")).toBe("[[Release plan|doc-123]]");
    expect(buildWikiDocumentHref("doc/123")).toBe("starlane:document/doc%2F123");
    expect(parseWikiDocumentHref("starlane:document/doc%2F123")).toBe("doc/123");
  });

  it("keeps legacy title-only wiki links honest and portable", () => {
    const markdown = "Review [[Release plan]] before launch.";
    const html = markdownToHtml(markdown);

    expect(html).toContain('class="starlane-wiki-link starlane-wiki-link--unresolved"');
    expect(html).toContain('title="Page target unavailable"');
    expect(html).not.toContain("href=");
    expect(serializeWikiLinkMarkdown("Release plan")).toBe("[[Release plan]]");
  });

  it("keeps wiki-link marks mounted in TipTap edit mode", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain('protocols: ["starlane"]');
    expect(source).toContain('name: "wikiLinkUnresolved"');
    expect(source).toContain("WikiLinkUnresolved,");
  });

  it("parses portable member mentions without losing the member ID", () => {
    const html = markdownToHtml("Please ask @[Harley Frank](user-123).");

    expect(html).toContain('data-type="content-mention"');
    expect(html).toContain('data-user-id="user-123"');
    expect(html).toContain("@Harley Frank");
  });

  it("renders hostile HTML and unsafe URLs as inert text", () => {
    const html = markdownToHtml(
      "<script>alert(1)</script> [bad](javascript:alert(1)) ![bad](data:text/html;base64,PHNjcmlwdD4)",
    );

    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).not.toContain("<script>");
    expect(html).not.toContain('href="javascript:');
    expect(html).not.toContain('src="data:text/html');
  });

  it("keeps approved links and local image data usable", () => {
    const html = markdownToHtml(
      "[mail](mailto:hello@example.com) ![pixel](data:image/png;base64,aGVsbG8=)",
    );

    expect(html).toContain('href="mailto:hello@example.com"');
    expect(html).toContain('src="data:image/png;base64,aGVsbG8="');
  });

  it("keeps the edit parser aligned with safe image-only data URLs in source view", () => {
    expect(EDITOR_IMAGE_OPTIONS.allowBase64).toBe(true);
    expect(EDITOR_IMAGE_OPTIONS.HTMLAttributes.class).toContain("rounded-card");

    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");
    expect(source).toContain("selectable: false");
    expect(source).toContain("draggable: false");
  });

  it("keeps atomic blocks out of whole-node selection", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");
    for (const block of [
      "BookmarkCard",
      "FileAttachment",
      "MathBlock",
      "TableOfContents",
      "StarlaneCodeBlock",
    ]) {
      const declaration = source.slice(source.indexOf(`const ${block} =`));
      const selectable = ["BookmarkCard", "FileAttachment", "StarlaneCodeBlock"].includes(block);
      expect(declaration.slice(0, declaration.indexOf("});"))).toContain(
        `selectable: ${selectable}`,
      );
    }
  });

  it("parses a persisted bookmark directive as a bookmark card", () => {
    const html = markdownToHtml("[bookmark:Starlane Notes](https://starlane.example)");

    expect(html).toContain('data-bookmark-card="true"');
    expect(html).toContain('href="https://starlane.example"');
    expect(html).toContain('title="Starlane Notes"');
    expect(html).toContain("Starlane Notes");
    expect(html).toContain('target="_blank"');
    expect(html).toContain("starlane.example");
  });

  it("detects GitHub and Gitea bookmark providers without mislabeling ordinary links", () => {
    expect(getBookmarkProvider("https://github.com/hbftechnologies/starlanNotes")).toBe("GitHub");
    expect(getBookmarkProvider("https://gitea.example.com/hft-applications/StarlaneNotes")).toBe(
      "Gitea",
    );
    expect(getBookmarkProvider("https://example.com/docs")).toBeNull();
  });

  it("derives provider repositories for bookmark cards", () => {
    expect(getBookmarkRepository("https://github.com/hbftechnologies/starlanNotes.git")).toBe(
      "hbftechnologies/starlanNotes",
    );
    expect(getBookmarkRepository("https://gitea.example.com/hft-applications/StarlaneNotes")).toBe(
      "hft-applications/StarlaneNotes",
    );
    expect(
      markdownToHtml("[bookmark:github.com](https://github.com/hbftechnologies/starlanotes)"),
    ).toContain("bookmark-card__provider-name");
  });

  it("persists and renders a bookmark preview image", () => {
    const imageUrl = "https://starlane.example/preview.png";
    const markdown = serializeBookmarkCardDirective(
      "Starlane Notes",
      "https://starlane.example",
      "A notes workspace",
      imageUrl,
    );
    const html = markdownToHtml(markdown);

    expect(html).toContain(`image-url="${imageUrl}"`);
    expect(
      readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8"),
    ).toContain("src: imageUrl");
  });

  it("preserves an optional bookmark description in the portable directive", () => {
    const html = markdownToHtml(
      '[bookmark:Tea CLI](https://gitea.com "The official command line client for Gitea.")',
    );

    expect(html).toContain('description="The official command line client for Gitea."');
    expect(
      serializeBookmarkCardDirective(
        "Tea CLI",
        "https://gitea.com",
        "The official command line client for Gitea.",
      ),
    ).toBe('[bookmark:Tea CLI](https://gitea.com "The official command line client for Gitea.")');
  });

  it("serializes bookmark cards back to the portable directive", () => {
    expect(serializeBookmarkCardDirective("Starlane Docs", "https://starlane.example/docs")).toBe(
      "[bookmark:Starlane Docs](https://starlane.example/docs)",
    );
    expect(serializeBookmarkCardDirective(null, null)).toBe("[bookmark:Bookmark]()");
  });

  it("parses inline formatting without swallowing following text", () => {
    const html = markdownToHtml("**Bold** plain `code`");

    expect(html).toContain("<strong>Bold</strong> plain <code>code</code>");
  });

  it("preserves nested unordered lists", () => {
    const html = markdownToHtml(
      ["- Parent", "  - Child", "    - Grandchild", "- Second"].join("\n"),
    );

    expect(html).toContain("<ul><li><p>Parent</p><ul><li><p>Child</p>");
    expect(html).toContain("<ul><li><p>Grandchild</p></li></ul>");
    expect(html).toContain("<li><p>Second</p></li>");
  });

  it("preserves nested task lists", () => {
    const html = markdownToHtml(["- [ ] Parent task", "  - [x] Child task"].join("\n"));

    expect(html).toContain('<ul data-type="taskList">');
    expect(html).toContain('<li data-type="taskItem" data-checked="false">');
    expect(html).toContain('<li data-type="taskItem" data-checked="true">');
    expect(html).toContain("<span>Child task</span>");
  });
});

describe("table style metadata", () => {
  it("supports four restrained table presets and keeps the default source clean", () => {
    expect(normalizeTableStyle("compact")).toBe("compact");
    expect(normalizeTableStyle("caboose")).toBe("directory");
    expect(parseTableStyleDirective("<!-- starlane-table: grid -->")).toBe("grid");
    expect(serializeTableStyleDirective("directory")).toBe("");
    expect(serializeTableStyleDirective("striped")).toBe("<!-- starlane-table: striped -->");
  });
});

describe("selection toolbar placement", () => {
  const schema = new Schema({
    nodes: {
      doc: { content: "paragraph+" },
      paragraph: { content: "text*" },
      text: {},
    },
  });
  const doc = schema.nodes.doc.create(
    null,
    schema.nodes.paragraph.create(null, schema.text("Reach")),
  );
  const selectedText = TextSelection.create(doc, 1, 3);
  const caret = TextSelection.create(doc, 2);

  it("prefers the space above selected text when it fits", () => {
    expect(getSelectionToolbarTop(120, 144, 40, 800)).toBe(72);
  });

  it("moves below top-edge text instead of covering the selection", () => {
    expect(getSelectionToolbarTop(24, 48, 40, 800)).toBe(56);
  });

  it("stays viewport-bounded when neither side has room", () => {
    expect(getSelectionToolbarTop(24, 784, 40, 800)).toBe(8);
  });

  it("keeps contextual formatting for legacy desktop callers and yields to the persistent toolbar", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain('aria-label="Editor toolbar"');
    expect(source).toContain('label: "Font family"');
    expect(source).toContain('aria-label="Text formatting"');
    expect(source).toContain('data-starlane-selection-toolbar="true"');
    expect(source).toContain("persistentToolbar: Boolean(desktopToolbarHost)");
    expect(source).toContain("<PersistentDocumentToolbar");
    expect(source).toContain("onKeyDown={handleSelectionTypeMenuKeyDown}");
    expect(source).toContain('aria-label="Edit link"');
    expect(source).toContain('aria-label="Comment on selection"');
    expect(source).toContain("setSelectionToolbarPosition(null)");
    expect(
      shouldShowSelectionToolbar({
        mobile: false,
        readOnly: false,
        selection: selectedText,
        inTable: false,
      }),
    ).toBe(true);
    expect(
      shouldShowSelectionToolbar({
        mobile: true,
        readOnly: false,
        selection: selectedText,
        inTable: false,
      }),
    ).toBe(false);
    expect(
      shouldShowSelectionToolbar({
        mobile: false,
        readOnly: false,
        persistentToolbar: true,
        selection: selectedText,
        inTable: false,
      }),
    ).toBe(false);
    expect(
      shouldShowSelectionToolbar({
        mobile: false,
        readOnly: false,
        selection: caret,
        inTable: false,
      }),
    ).toBe(false);
    expect(
      shouldShowSelectionToolbar({
        mobile: false,
        readOnly: false,
        selection: selectedText,
        inTable: true,
      }),
    ).toBe(false);
  });

  it("offers 25 bundled document fonts with searchable family categories", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");
    const css = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");

    expect(getEditorFontFamilyCount()).toBe(25);
    expect(filterEditorFontGroups("serif").map((group) => group.label)).toEqual(["Serif"]);
    expect(
      filterEditorFontGroups("source").flatMap((group) =>
        group.options.map((option) => option.label),
      ),
    ).toEqual(["Source Sans 3", "Source Serif 4", "Source Code Pro"]);
    expect(filterEditorFontGroups("script").map((group) => group.label)).toEqual(["Handwriting"]);
    expect(filterEditorFontGroups("covenant archive")).toEqual([]);

    for (const group of ["Sans serif", "Serif", "Monospace", "Display", "Handwriting"]) {
      expect(source).toContain(`label: "${group}"`);
    }
    for (const family of [
      "Atkinson Hyperlegible",
      "DM Sans",
      "Source Sans 3",
      "Playfair Display",
      "Source Code Pro",
      "Bebas Neue",
      "Caveat",
    ]) {
      expect(source).toContain(`value: "${family}"`);
    }
    for (const packageName of [
      "atkinson-hyperlegible",
      "dm-sans",
      "source-sans-3",
      "playfair-display",
      "source-code-pro",
      "bebas-neue",
      "caveat",
    ]) {
      expect(css).toContain(`@fontsource/${packageName}/latin-400.css`);
    }
    expect(source).toContain("bundled families · available offline");
    expect(source).toContain('aria-label="Search font families"');
    expect(source).not.toContain("Self-hosted Starlane fonts");
  });
});

describe("upload slash selection ownership", () => {
  it("keeps the upload dialog's selected command text from opening the format toolbar", () => {
    const uploadRange = { from: 4, to: 11 };
    expect(isUploadMenuOwnedSelection(uploadRange, { empty: false, from: 4, to: 11 })).toBe(true);
    expect(isUploadMenuOwnedSelection(uploadRange, { empty: false, from: 4, to: 12 })).toBe(false);
    expect(isUploadMenuOwnedSelection(uploadRange, { empty: true, from: 11, to: 11 })).toBe(false);
  });
});

describe("editor canvas focus ownership", () => {
  it("claims a primary click on blank editable canvas space", () => {
    expect(
      shouldClaimEditorCanvasPointer({
        readOnly: false,
        primaryButton: true,
        targetInsideEditor: false,
        targetInsideChrome: false,
      }),
    ).toBe(true);
  });

  it("leaves editor content and interactive chrome to their native focus behavior", () => {
    expect(
      shouldClaimEditorCanvasPointer({
        readOnly: false,
        primaryButton: true,
        targetInsideEditor: true,
        targetInsideChrome: false,
      }),
    ).toBe(false);
    expect(
      shouldClaimEditorCanvasPointer({
        readOnly: false,
        primaryButton: true,
        targetInsideEditor: false,
        targetInsideChrome: true,
      }),
    ).toBe(false);
  });

  it("does not claim secondary clicks or read-only page space", () => {
    expect(
      shouldClaimEditorCanvasPointer({
        readOnly: false,
        primaryButton: false,
        targetInsideEditor: false,
        targetInsideChrome: false,
      }),
    ).toBe(false);
    expect(
      shouldClaimEditorCanvasPointer({
        readOnly: true,
        primaryButton: true,
        targetInsideEditor: false,
        targetInsideChrome: false,
      }),
    ).toBe(false);
  });

  it("keeps table controls selection-owned through editor blur", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");
    const styles = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");

    expect(source).toContain('"aria-label": "Document editor"');
    expect(source).toContain('data-starlane-editor-canvas="true"');
    expect(source).toContain("activeElement.blur()");
    expect(source).toContain('data-starlane-table-toolbar="true"');
    expect(source).toContain('editor.commands.focus(position?.pos ?? "end")');
    expect(source).toContain("editor.view.focus()");
    expect(source).toContain("updateTableToolbar(activeEditor)");
    expect(source.match(/setTableToolbarPosition\(null\)/g)).toHaveLength(1);
    expect(styles).toContain(".starlane-editor-writing-surface");
    expect(styles).toContain("min-height: max(300px, calc(100dvh - 18rem))");
  });
});

describe("block edge insertion", () => {
  const schema = new Schema({
    nodes: {
      doc: { content: "block+" },
      paragraph: { content: "inline*", group: "block" },
      heading: { content: "inline*", group: "block", attrs: { level: { default: 1 } } },
      text: { group: "inline" },
    },
    marks: {
      strong: {},
    },
  });

  const createState = () => {
    const strong = schema.marks.strong.create();
    const first = schema.nodes.heading.create(
      { level: 1 },
      schema.text("Selected heading", [strong]),
    );
    const second = schema.nodes.paragraph.create(null, schema.text("Existing neighbour"));
    const doc = schema.nodes.doc.create(null, [first, second]);
    return EditorState.create({
      doc,
      selection: TextSelection.create(doc, 2),
      storedMarks: [strong],
    });
  };

  it("creates a clean paragraph below the targeted block without transforming its neighbour", () => {
    const result = createSiblingParagraphTransaction(createState(), 0, "below");

    expect(result).not.toBeNull();
    expect(result?.transaction.doc.childCount).toBe(3);
    expect(result?.transaction.doc.child(0).type.name).toBe("heading");
    expect(result?.transaction.doc.child(1).type.name).toBe("paragraph");
    expect(result?.transaction.doc.child(1).textContent).toBe("");
    expect(result?.transaction.doc.child(2).textContent).toBe("Existing neighbour");
    expect(result?.transaction.selection.from).toBe(result?.selectionPosition);
    expect(result?.transaction.storedMarks).toBeNull();
  });

  it("creates a clean paragraph above the first block from its current document index", () => {
    const result = createSiblingParagraphTransaction(createState(), 0, "above");

    expect(result).not.toBeNull();
    expect(result?.transaction.doc.childCount).toBe(3);
    expect(result?.transaction.doc.child(0).type.name).toBe("paragraph");
    expect(result?.transaction.doc.child(1).textContent).toBe("Selected heading");
    expect(result?.transaction.selection.from).toBe(1);
  });
});

describe("isCommandPaletteShortcut", () => {
  const shortcut = (
    key: string,
    code: string,
    modifiers: Partial<Pick<KeyboardEvent, "altKey" | "ctrlKey" | "metaKey">> = {},
  ) => ({
    altKey: false,
    code,
    ctrlKey: true,
    key,
    metaKey: false,
    ...modifiers,
  });

  it("accepts Ctrl plus the slash key across browser key variants", () => {
    expect(isCommandPaletteShortcut(shortcut("/", "Slash"))).toBe(true);
    expect(isCommandPaletteShortcut(shortcut("?", "Slash"))).toBe(true);
    expect(isCommandPaletteShortcut(shortcut("Divide", "NumpadDivide"))).toBe(true);
  });

  it("rejects non-Ctrl or non-slash shortcuts", () => {
    expect(isCommandPaletteShortcut(shortcut("/", "Slash", { ctrlKey: false }))).toBe(false);
    expect(isCommandPaletteShortcut(shortcut("/", "Slash", { metaKey: true }))).toBe(false);
    expect(isCommandPaletteShortcut(shortcut("/", "Slash", { altKey: true }))).toBe(false);
    expect(isCommandPaletteShortcut(shortcut("k", "KeyK"))).toBe(false);
  });
});

describe("getBlockMoveShortcut", () => {
  const shortcut = (
    key: string,
    modifiers: Partial<Pick<KeyboardEvent, "altKey" | "ctrlKey" | "metaKey" | "shiftKey">> = {},
  ) => ({
    altKey: true,
    ctrlKey: false,
    key,
    metaKey: false,
    shiftKey: true,
    ...modifiers,
  });

  it("maps Alt/Option plus Shift arrows to accessible block movement", () => {
    expect(getBlockMoveShortcut(shortcut("ArrowUp"))).toBe("up");
    expect(getBlockMoveShortcut(shortcut("ArrowDown"))).toBe("down");
  });

  it("does not steal plain or command-modified arrow keys", () => {
    expect(getBlockMoveShortcut(shortcut("ArrowUp", { altKey: false }))).toBeNull();
    expect(getBlockMoveShortcut(shortcut("ArrowDown", { shiftKey: false }))).toBeNull();
    expect(getBlockMoveShortcut(shortcut("ArrowUp", { ctrlKey: true }))).toBeNull();
    expect(getBlockMoveShortcut(shortcut("ArrowDown", { metaKey: true }))).toBeNull();
    expect(getBlockMoveShortcut(shortcut("ArrowLeft"))).toBeNull();
  });
});

describe("mobile editor toolbar actions", () => {
  it("keeps comments out of edit mode and exposes Commands as a toolbar action", () => {
    expect(DEFAULT_MOBILE_TOOLBAR_ACTIONS).not.toContain("comment");
    expect(DEFAULT_MOBILE_TOOLBAR_ACTIONS).toContain("commands");
    expect(getMobileToolbarAction("commands", {} as never)).toMatchObject({
      label: "Command menu",
    });
  });

  it("makes section controls a customizable mobile toolbar action group", () => {
    expect(DEFAULT_MOBILE_TOOLBAR_ACTIONS).toContain("sectionControls");
    expect(getMobileToolbarAction("sectionControls", {} as never)).toMatchObject({
      label: "Section controls",
    });
    expect(
      getAvailableMobileToolbarActionIds(
        DEFAULT_MOBILE_TOOLBAR_ACTIONS.filter((action) => action !== "sectionControls"),
      )[0],
    ).toBe("sectionControls");
  });

  it("reflects the active heading level and font size in the toolbar icons", () => {
    const editor = {
      isActive: (name: string, attributes?: { level?: number }) =>
        name === "heading" && attributes?.level === 2,
      getAttributes: (name: string) => (name === "fontSize" ? { fontSize: "18px" } : {}),
    } as never;

    expect(getMobileToolbarAction("heading", editor)).toMatchObject({ label: "Heading 2" });
    expect(getMobileToolbarAction("fontSize", editor)).toMatchObject({ label: "Font size 18px" });
  });

  it("anchors desktop toolbar popovers below their trigger", () => {
    expect(
      getToolbarPopoverPosition(
        { top: 40, bottom: 76, left: 320, width: 36 },
        { innerWidth: 1200, innerHeight: 800 },
      ),
    ).toEqual({ top: 80, left: 320 });
  });

  it("anchors the desktop command palette below its toolbar trigger", () => {
    expect(
      getToolbarPopoverPosition(
        { top: 24, bottom: 60, left: 780, width: 36 },
        { innerWidth: 1280, innerHeight: 900 },
        { width: 384, height: 420 },
      ),
    ).toEqual({ top: 64, left: 780 });
  });

  it("keeps block controls and formatting actions on the persistent toolbar", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(DEFAULT_MOBILE_TOOLBAR_ACTIONS).toContain("blockControls");
    expect(DEFAULT_MOBILE_TOOLBAR_ACTIONS).toContain("color");
    expect(source).toContain('aria-label="Block controls"');
    expect(source).toContain("aria-label={sectionControlsLabel}");
    expect(source).toContain('aria-label="Block actions"');
    expect(source).toContain('"Move block up"');
    expect(source).toContain('"Move block down"');
    expect(source).toMatch(/>\s*Delete block\s*<\/button>/);
    expect(source).not.toContain('["Command menu...", () => openSearchableCommandPalette(editor)]');
    expect(source).toContain('aria-label="Text color"');
    expect(source).toContain("bottom: mobileMenuBottom");
    expect(source).toContain('unsetMark("textColor")');
    expect(source).not.toContain("colorPickerRef.current?.click()");
  });

  it("keeps the complete action catalog mobile-only while desktop stays contextual", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain('data-starlane-editor-toolbar="true"');
    expect(source).toContain('aria-label="Editor toolbar"');
    expect(source).toContain("!readOnly && showMobileCommandButton && (");
    expect(source).not.toContain("desktopToolbarExpanded");
    expect(source).not.toContain("selectionAdvancedToolbarHost");
    expect(source).not.toContain('aria-label="More formatting tools"');
    expect(source).not.toContain("desktopToolbarPortalTarget");
    expect(source).toContain('side={showMobileCommandButton ? "top" : "bottom"}');
    expect(source).not.toContain(': { top: "4.75rem" }');
  });
});

describe("getMobileKeyboardInsetMeasurement", () => {
  it("detects when the selected block is covered by the keyboard-safe toolbar area", () => {
    expect(isEditorSelectionObscured({ top: 720, bottom: 812 }, { top: 0, bottom: 844 })).toBe(
      true,
    );
    expect(isEditorSelectionObscured({ top: 500, bottom: 580 }, { top: 0, bottom: 844 })).toBe(
      false,
    );
  });

  it("measures keyboard overlap from the visual viewport", () => {
    const result = getMobileKeyboardInsetMeasurement(
      { layoutHeight: 844, visualHeight: 520, visualTop: 0, visualWidth: 390 },
      { baseline: 844, width: 390 },
    );

    expect(result.inset).toBe(324);
    expect(result.state).toEqual({ baseline: 844, width: 390 });
  });

  it("accounts for visual viewport scroll offset", () => {
    const result = getMobileKeyboardInsetMeasurement(
      { layoutHeight: 844, visualHeight: 520, visualTop: 44, visualWidth: 390 },
      { baseline: 844, width: 390 },
    );

    expect(result.inset).toBe(280);
  });

  it("keeps the keyboard open while iOS pans a contracted visual viewport", () => {
    const result = getMobileKeyboardInsetMeasurement(
      { layoutHeight: 844, visualHeight: 520, visualTop: 300, visualWidth: 390 },
      { baseline: 844, width: 390 },
    );

    expect(result.inset).toBe(24);
    expect(isMobileViewportContracted(844, 520)).toBe(true);
  });

  it("treats small viewport differences as a closed keyboard", () => {
    const result = getMobileKeyboardInsetMeasurement(
      { layoutHeight: 844, visualHeight: 812, visualTop: 0, visualWidth: 390 },
      { baseline: 844, width: 390 },
    );

    expect(result.inset).toBe(0);
  });

  it("resets the baseline after a large width change", () => {
    const result = getMobileKeyboardInsetMeasurement(
      { layoutHeight: 430, visualHeight: 430, visualTop: 0, visualWidth: 844 },
      { baseline: 844, width: 390 },
    );

    expect(result.inset).toBe(0);
    expect(result.state).toEqual({ baseline: 430, width: 844 });
  });

  it("restores selection only after the visual viewport actually expands", () => {
    expect(
      shouldRestoreEditorSelectionAfterKeyboardClose({
        wasOpen: true,
        keyboardOpen: false,
        viewportContracted: true,
      }),
    ).toBe(false);
    expect(
      shouldRestoreEditorSelectionAfterKeyboardClose({
        wasOpen: true,
        keyboardOpen: false,
        viewportContracted: false,
      }),
    ).toBe(true);
  });

  it("places the mobile toolbar by keyboard overlap when the keyboard is open", () => {
    expect(getMobileToolbarPlacement(280)).toEqual({
      keyboardOpen: true,
      toolbarBottom: "280px",
      menuBottom: "calc(280px + 3.25rem)",
      reason: "keyboard-overlap",
    });
  });

  it("falls back to safe-area placement when the keyboard is closed", () => {
    expect(getMobileToolbarPlacement(0)).toEqual({
      keyboardOpen: false,
      toolbarBottom:
        "max(calc(0.75rem + var(--starlane-mobile-statusbar-height, 0px)), calc(env(safe-area-inset-bottom) + 0.75rem + var(--starlane-mobile-statusbar-height, 0px)))",
      menuBottom:
        "max(calc(4.75rem + var(--starlane-mobile-statusbar-height, 0px)), calc(env(safe-area-inset-bottom) + 4.75rem + var(--starlane-mobile-statusbar-height, 0px)))",
      reason: "safe-area",
    });
  });

  it("keeps the mobile editor toolbar keyboard-safe and horizontally scrollable", () => {
    expect(MOBILE_EDITOR_TOOLBAR_BASE_CLASS).toContain("mobile-editor-toolbar");
    expect(MOBILE_EDITOR_TOOLBAR_BASE_CLASS).toContain("fixed");
    expect(MOBILE_EDITOR_TOOLBAR_BASE_CLASS).toContain("transition-[bottom]");
    expect(MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS).toContain("left-0");
    expect(MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS).toContain("right-0");
    expect(MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS).toContain("bg-surface");
    expect(MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS).toContain("left-3");
    expect(MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS).toContain("right-3");
    expect(MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS).toContain("h-11");
    expect(MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS).toContain("overflow-x-auto");
    expect(MOBILE_EDITOR_TOOLBAR_ACTION_CLASS).toContain("h-11");
    expect(MOBILE_EDITOR_TOOLBAR_ACTION_CLASS).toContain("w-11");
    expect(MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS).toContain("h-11");
    expect(MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS).toContain("w-11");
    expect(MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS).toContain("shrink-0");
    expect(MOBILE_TABLE_TOOLBAR_BASE_CLASS).toContain("fixed");
    expect(MOBILE_TABLE_TOOLBAR_BASE_CLASS).toContain("z-[60]");
    expect(MOBILE_TABLE_TOOLBAR_BASE_CLASS).toContain("transition-[bottom]");
    expect(TABLE_EDITOR_TOOLBAR_SURFACE_CLASS).toContain("h-11");
    expect(TABLE_EDITOR_TOOLBAR_SURFACE_CLASS).toContain("overflow-x-auto");
    expect(TABLE_EDITOR_TOOLBAR_SURFACE_CLASS).toContain("starlane-control-chrome-surface");
    expect(TABLE_EDITOR_TOOLBAR_SURFACE_CLASS).toContain(MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS);
    expect(DEFAULT_MOBILE_TOOLBAR_ACTIONS).toContain("font");
  });

  it("keeps ordinary editor commands muted until their shared active state applies", () => {
    for (const action of [...DEFAULT_MOBILE_TOOLBAR_ACTIONS, "comment"] as const) {
      expect(getEditorToolbarActionTone(action)).toBe("text-text-muted");
    }
  });

  it("keeps table controls icon-only with accessible action names on every layout", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("aria-label={action.label}");
    expect(source).toContain("title={action.label}");
    expect(source).toContain('<TooltipContent side="top">{action.label}</TooltipContent>');
    expect(source).toContain("MOBILE_EDITOR_TOOLBAR_ACTION_CLASS");
    expect(source).not.toContain('.replace("Sort column ", "Sort ")');
  });

  it("marks live editor tables for header-aware striped rows", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");
    const css = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");

    expect(source).toContain("this.table.dataset.hasHeader");
    expect(source).toContain('type.name === "tableHeader"');
    expect(css).toContain('[data-has-header="false"]');
    expect(css).toContain('[data-has-header="true"]');
    expect(css).toContain("border-right:");
  });

  it("centers natural-width tables and contains wide tables in the document scroll frame", () => {
    const css = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");

    expect(css).toContain(":is(.tiptap-editor, .markdown-preview) .tableWrapper");
    expect(css).toContain("width: max-content");
    expect(css).toContain("max-width: 100%");
    expect(css).toContain("margin-inline: auto");
    expect(css).toContain("overflow-x: auto");
    expect(css).toContain("overscroll-behavior-inline: contain");
    expect(css).toContain("min-width: 0");
  });

  it("gives the mobile heading menu the shared overlay lifecycle", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("useOverlayFocusLifecycle");
    expect(source).toContain("triggerRef: mobileHeadingMenuTriggerRef");
    expect(source).toContain("ref={mobileHeadingMenuRef}");
    expect(source).toContain('aria-label="Heading level"');
    expect(source).not.toContain("handleHeadingMenuEscape");
  });

  it("gives the dedicated link editor the shared overlay lifecycle", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("open: Boolean(linkMenu)");
    expect(source).toContain("containerRef: linkMenuRef");
    expect(source).toContain("triggerRef: linkMenuTriggerRef");
    expect(source).toContain('closest("a[href]")');
    expect(source).toContain("onClick={closeAndRestoreLinkMenu}");
    expect(source).toContain('aria-label={readOnly ? "Link details" : "Link editor"}');
    expect(source).toContain('aria-label="Link URL"');
    expect(source).toContain("aria-invalid={");
    expect(source).toContain("Use an HTTPS, HTTP, email, telephone, Starlane, or relative link.");
    expect(source).toContain('aria-label="Open link"');
    expect(source).toContain('aria-label="Remove link"');
    expect(source).toContain('linkMenu.href ? "Update link" : "Add link"');
    expect(source).toContain("focusInsideLinkMenu");
  });

  it("keeps inserted code blocks followed by an editable paragraph", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("const insertParagraphAfterActiveBlock");
    expect(source).toContain('nodeAfter?.type.name === "paragraph"');
    expect(source).toContain("insertParagraphAfterActiveBlock(activeEditor);");
    expect(source).toContain("if (depth <= 0) return false;");
  });

  it("keeps adjacent link editing anchored to the clicked mark", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("function getLinkRangeAtAnchor(");
    expect(source).toContain("getLinkRangeAtAnchor(activeEditor, anchor, href)");
    expect(source).toContain("const attempts = [pos + 1, pos, pos - 1]");
    expect(source).toMatch(/createEditorLinkTransaction\(\s*activeEditor\.state/);
    expect(source).toContain("createEditorUnlinkTransaction(activeEditor.state, state)");
  });

  it("styles code metadata fields in edit mode and keeps Copy in read-only mode", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/CodeBlockNodeView.tsx"),
      "utf8",
    );

    expect(source).toContain('data-starlane-code-frame="true"');
    expect(source).toContain('data-code-block-edit-controls="true"');
    expect(source).toContain("onPointerDown={keepMetadataControlInteraction}");
    expect(source).toContain("onClick={keepMetadataControlInteraction}");
    expect(source).toContain("code-block-metadata-field");
    expect(source).not.toContain(">Language</span>");
    expect(source).not.toContain(">Code block name</span>");
    expect(source).toContain("<CodeLanguagePicker");
    expect(source).toContain('role="menuitemradio"');
    expect(source).toContain('aria-label="Code languages"');
    expect(source).toContain('"bg-primary/15 text-primary"');
    expect(source).toContain('event.key !== "Enter" && event.key !== " "');
    expect(source).not.toContain('aria-label="Code block placement"');
    expect(source).not.toContain('<option value="left">Left</option>');
    expect(source).toContain('className="bg-surface-raised h-10 min-w-0 pl-9 text-sm"');
    expect(source).toContain('aria-label="Code language"');
    expect(source).toContain('aria-label="Code block name"');
    expect(source).toContain("copyCode");
    expect(source).toContain(") : (");
    expect(source).toContain("NodeViewContent");
  });

  it("keeps code metadata controls outside ProseMirror event handling", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");
    const toolbar = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorToolbar.tsx"),
      "utf8",
    );

    expect(source).toContain("target.closest(\"[data-code-block-edit-controls='true']\")");
    expect(toolbar).toContain('editor.commands.updateAttributes("codeBlock", { language })');
    expect(toolbar).not.toContain(
      'editor.chain().focus().updateAttributes("codeBlock", { language }).run()',
    );
  });

  it("anchors block actions to the shared popover primitive", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("<Popover");
    expect(source).toContain("<PopoverTrigger asChild>");
    expect(source).toContain("<PopoverContent");
    expect(source).toContain('side="bottom"');
    expect(source).toContain('aria-label="Block actions"');
    expect(source).toContain("open: open ?? (sameTarget ? Boolean(current?.open) : false)");
    expect(source).toContain("onMouseDown={claimEditorControlPointer}");
    expect(source).toContain('position: "fixed"');
    expect(source).toContain("top: blockMenu.menuPosition.top");
    expect(source).toContain("left: blockMenu.menuPosition.left");
    expect(source).toContain("blockMenuFocusTriggerRef.current?.focus()");
    expect(source).toContain("blockMenuFocusTriggerRef.current = editor.view.dom");
    expect(source).toContain("TextSelection.near(editor.state.doc.resolve(selectionPosition))");
    expect(source).toContain('aria-live="polite"');
  });

  it("keeps every structural block action in More across pointer modes", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");
    const menuSource = source.slice(source.indexOf("{blockMenu.open && ("));

    expect(menuSource).not.toContain("{showMobileCommandButton && (");
    expect(menuSource).not.toContain("...(showMobileCommandButton");
    expect(menuSource).toContain('["Add block above"');
    expect(menuSource).toContain('["Add block below"');
    expect(menuSource).toContain('"Move block up"');
    expect(menuSource).toContain('"Move block down"');
    expect(menuSource).toContain('["Duplicate block"');
    expect(menuSource).toContain("Turn into...");
    expect(menuSource).toMatch(/>\s*Delete block\s*<\/button>/);
  });

  it("keeps the targeted block menu available through the editor blur handoff", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("!blockMenuOpenRef.current");
    expect(source).toContain("updateBlockMenuForBlock(activeEditor, block, true, menuPosition)");
    expect(source).toContain('placement: "above" | "below"');
    expect(source).toContain("createSiblingParagraphTransaction");
    expect(source).toContain("target.index");
    expect(source).toContain("selectionOverride");
    expect(source).toContain("transaction.setStoredMarks(null)");
    expect(source).not.toContain('.insertContentAt(insertAt, { type: "paragraph" })');
  });

  it("does not mount duplicate block-edge rails beside the complete toolbar menu", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");
    const styles = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");

    expect(source).toContain("updateBlockMenuForBlock");
    expect(source).not.toContain('data-block-edge="top"');
    expect(source).not.toContain('data-block-edge="bottom"');
    expect(source).not.toContain('data-block-edge-controls="true"');
    expect(source).not.toContain("getTopLevelBlockAtPointerTarget");
    expect(source).not.toContain("<GripVertical");
    expect(source).not.toContain('aria-label="Insert block"');
    expect(styles).not.toContain(".block-edge-toolbar");
    expect(source).not.toContain('data-editor-toolbar-trailing-control="true"');
    expect(source).not.toContain('data-starlane-selection-comment="true"');
    expect(source).not.toContain("is-active-editor-block");
    expect(styles).not.toContain("is-active-editor-block");
    expect(styles).toContain(
      '.starlane-editor-writing-surface > *:not([data-starlane-section="true"]):hover',
    );
    expect(styles).not.toContain(".tiptap-editor > *:hover::before");
    expect(styles).toContain(".starlane-editor-writing-surface .ProseMirror-selectednode");
  });

  it("gives sections their own shared add, move, and delete control rail", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorSection.tsx"),
      "utf8",
    );
    const editorSource = readFileSync(
      resolve(process.cwd(), "src/components/editor/Editor.tsx"),
      "utf8",
    );
    const styles = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");

    expect(source).toContain('columnsBlock ? "Column block" : "Section"');
    expect(source).toContain('containerLabel = columnsBlock ? "column block" : "section"');
    expect(source).toContain("label={`Add ${containerLabel} above`}");
    expect(source).not.toContain('label="Add block to section"');
    expect(source).toContain("SectionLayoutAction");
    expect(source).toContain(
      'aria-label={`Choose ${columnsBlock ? "column block" : "section"} layout`}',
    );
    expect(source).toContain('columnsBlock ? "Column block layouts" : "Section layouts"');
    expect(source).toContain("onKeyDown={handleLayoutMenuKeyDown}");
    expect(source).toContain("aria-label={`${layoutLabel} ${containerLabel} content`}");
    expect(source).toContain('"two-equal"');
    expect(source).toContain('"three-equal"');
    expect(source).toContain('"wide-left"');
    expect(source).toContain('"wide-right"');
    expect(source).toContain('HTMLAttributes["data-layout"] ?? HTMLAttributes.layout');
    expect(source).toContain("label={`Move ${containerLabel} up`}");
    expect(source).toContain("label={`Move ${containerLabel} down`}");
    expect(source).toContain("label={`Add ${containerLabel} below`}");
    expect(source).toContain("label={`Delete ${containerLabel}`}");
    expect(source).toContain("CONTROL_CHROME_RAIL_CLASS");
    expect(source).toContain("CONTROL_CHROME_ACTION_CLASS");
    expect(EditorSection.config.selectable).toBe(false);
    expect(EditorColumns.config.selectable).toBe(false);
    expect(EditorSectionColumn.config.selectable).toBe(false);
    expect(editorSource).toContain('block.node.type.name === "section"');
    expect(editorSource).toContain(
      'data-mobile-editor-toolbar={showMobileCommandButton ? "true" : undefined}',
    );
    expect(editorSource).toContain(
      'label: `Add ${activeSection.kind === "columns" ? "column block" : "section"} above`',
    );
    expect(editorSource).toContain('label: "Block controls"');
    expect(editorSource).toContain('action === "blockControls"');
    expect(editorSource).toContain(
      'label: `Add ${activeSection.kind === "columns" ? "column block" : "section"} below`',
    );
    expect(editorSource).toContain('action === "sectionControls"');
    expect(editorSource).toContain("<Popover");
    expect(editorSource).toContain("open={sectionControlsOpen}");
    expect(editorSource).toContain("aria-label={sectionControlsLabel}");
    expect(editorSource).toContain('aria-haspopup="menu"');
    expect(editorSource).toContain("sectionControlActions.map");
    expect(editorSource).toContain('role="menuitemradio"');
    expect(editorSource).toContain('onClick={() => runSectionControl("delete")}');
    expect(editorSource).not.toContain("mobileSectionControls");
    expect(editorSource).not.toContain("order-first");
    expect(editorSource).toContain("SECTION_LAYOUT_OPTIONS.filter");
    expect(styles).toContain('[data-starlane-section="true"]');
    expect(styles).toContain(".editor-section__toolbar");
    expect(styles).toContain(".editor-section__column > :first-child");
    expect(styles).toContain('[data-mobile-editor-toolbar="true"] .editor-section__toolbar');
    expect(styles).toContain('[data-layout="two-equal"]');
    expect(styles).toContain('[data-layout="three-equal"]');
    expect(styles).toContain('[data-layout="wide-left"]');
    expect(styles).toContain('[data-layout="wide-right"]');
    expect(styles).toContain('.editor-section__content[data-layout="two-equal"]');
    expect(styles).toContain("@media (max-width: 720px)");
  });

  it("keeps editor commands and table controls inside the active editor surface", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain('$position.node(depth).type.name !== "table"');
    expect(source).toContain("getActiveTableElement");
    expect(source).toContain("editor.view.nodeDOM(tablePosition)");
    expect(source).toContain("!editor.view.dom.contains(target)");
    expect(source).toContain(
      'editor.view.dom.addEventListener("keydown", handleEditorCanvasKeyDown)',
    );
    expect(source).toContain("event.stopPropagation()");
    expect(source).not.toContain('!tipTapEditor.isFocused || !tipTapEditor.isActive("table")');
  });

  it("gives the editor context menu the shared overlay lifecycle", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("<ContextMenu");
    expect(source).toContain("<ContextMenuTrigger asChild>");
    expect(source).toContain('<ContextMenuContent aria-label="Editor context menu">');
    expect(source).toContain('aria-label="Editor context menu"');
    expect(source).toContain("Link...");
    expect(source).toContain("prepareEditorContextMenu(event.nativeEvent)");
    expect(source).not.toContain("contextMenu.position");
    expect(source).not.toContain('className="fixed z-50 w-48"');
    expect(source).toContain("contextMenuOpeningRef.current = true;");
    expect(source).toContain("if (contextMenuOpeningRef.current)");
  });

  it("gives non-form editor action menus shared directional keyboard navigation", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("useMenuKeyboardNavigation");
    expect(source).toContain("handleMobileHeadingMenuKeyDown");
    expect(source).toContain("handleBlockMenuKeyDown");
  });
});
