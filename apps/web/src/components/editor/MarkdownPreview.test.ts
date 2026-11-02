import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { MemoryRouter } from "react-router-dom";
import {
  evaluateDataviewComputedColumn,
  getDataviewResultPath,
  getPreviewSelectionCommentPosition,
  groupDataviewResults,
  MarkdownPreview,
  parseDataviewBlockConfig,
  renderMarkdownPreviewHtml,
  replaceMarkdownAttachmentReferences,
  sortDataviewResults,
  splitMarkdownDataviewBlocks,
  wrapPreviewTables,
} from "./MarkdownPreview";
import { markdownToHtml } from "./Editor";
import {
  parsePortableCodeTabs,
  parsePortableGalleryBlock,
  parsePortableStatusBlock,
  parsePortableVideoBlock,
} from "./PortableBlocks";
import { SECTION_END_MARKER, SECTION_START_MARKER } from "./EditorSection";

describe("splitMarkdownDataviewBlocks", () => {
  it("disables preview task checkboxes while preserving their Markdown checked state", () => {
    const markdown = "- [x] Complete\n- [ ] In progress";
    const editorHtml = markdownToHtml(markdown);
    const previewHtml = renderMarkdownPreviewHtml(markdown);

    expect(editorHtml).toContain('<input type="checkbox" checked>');
    expect(editorHtml).toContain('<input type="checkbox">');
    expect(editorHtml).not.toContain("disabled");
    expect(previewHtml).toContain('<input type="checkbox" checked disabled>');
    expect(previewHtml).toContain('<input type="checkbox" disabled>');
  });

  it("hydrates image attachment references without replacing file download records", () => {
    const content = [
      "![Screenshot](starlane-attachment://image-123)",
      "",
      '[report.pdf](starlane-attachment://file-456 "starlane:size=51200")',
    ].join("\n");
    const hydrated = replaceMarkdownAttachmentReferences(
      content,
      new Map([
        ["image-123", "data:image/png;base64,aW1hZ2U="],
        ["file-456", "data:application/pdf;base64,ZmlsZQ=="],
      ]),
    );

    expect(hydrated).toContain("![Screenshot](data:image/png;base64,aW1hZ2U=)");
    expect(hydrated).toContain(
      '[report.pdf](starlane-attachment://file-456 "starlane:size=51200")',
    );
  });

  it("positions the Page-view inline-comment action above or below the selection", () => {
    expect(
      getPreviewSelectionCommentPosition(
        { top: 120, bottom: 140, left: 160, right: 240, width: 80 },
        400,
        800,
      ),
    ).toEqual({ top: 76, left: 182 });
    expect(
      getPreviewSelectionCommentPosition(
        { top: 12, bottom: 32, left: 2, right: 42, width: 40 },
        320,
        480,
      ),
    ).toEqual({ top: 40, left: 8 });
  });

  it("offers a comment-only action for selected Page-view text", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/MarkdownPreview.tsx"),
      "utf8",
    );

    expect(source).toContain('document.addEventListener("selectionchange"');
    expect(source).toContain('aria-label="Page selection comment action"');
    expect(source).toContain('aria-label="Comment on selected text"');
    expect(source).toContain("selection_text: commentDialog.selectionText");
    expect(source).toContain("<EditorCommentDialog");
    expect(source).not.toContain("contentEditable");
  });

  it("renders portable code tabs, status, video, and gallery blocks in page view", () => {
    const html = renderToStaticMarkup(
      createElement(MarkdownPreview, {
        content: [
          "```code-tabs",
          "title: Deploy",
          "tab: TypeScript | ts",
          "const ready = true;",
          "```",
          "",
          "```status",
          "state: online",
          "label: API reachable",
          "```",
          "",
          "```video",
          "title: Walkthrough",
          "src: https://example.com/demo.mp4",
          "```",
          "",
          "```gallery",
          "layout: timeline",
          "title: Launch sequence",
          "---",
          "![Start](https://example.com/start.png)",
          "```",
          "",
          "```carousel",
          "title: Screenshots",
          "---",
          "![One](https://example.com/one.png)",
          "![Two](https://example.com/two.png)",
          "```",
        ].join("\n"),
      }),
    );

    expect(html).toContain('data-portable-code-tabs="true"');
    expect(html).toContain('data-portable-status="true"');
    expect(html).toContain('data-portable-video="true"');
    expect(html).toContain('data-portable-gallery="true"');
    expect(html).toContain('data-gallery-layout="timeline"');
    expect(html).toContain('data-gallery-layout="carousel-7"');
  });
  it("renders highlighted code fences in page view markdown", () => {
    const html = renderToStaticMarkup(
      createElement(MarkdownPreview, {
        content: ["```ts", "const answer = 42;", "```"].join("\n"),
      }),
    );

    expect(html).toContain('class="language-ts"');
    expect(html).toContain("hljs-keyword");
    expect(html).toContain("hljs-number");
    expect(html).toContain('data-portable-code-block="true"');
    expect(html).toContain('aria-label="Copy ts code"');
    expect(html).toContain(">Copy</span>");
  });

  it("renders code tabs nested inside a section instead of an empty placeholder", () => {
    const content = [
      SECTION_START_MARKER,
      "",
      "# Examples",
      "",
      "```code-tabs",
      "title: Deploy",
      "tab: Bash | bash",
      "echo ready",
      "```",
      "",
      SECTION_END_MARKER,
    ].join("\n");

    const html = markdownToHtml(content);
    expect(html).toContain('data-code-tabs-preview="true"');
    expect(html).toContain('aria-selected="true"');
    expect(html).toContain("echo ready");
  });

  it("keeps fenced code while removing plain legacy section boundaries", () => {
    const content = [
      SECTION_START_MARKER,
      "",
      "Section 1",
      "",
      SECTION_END_MARKER,
      "",
      SECTION_START_MARKER,
      "",
      "Section 2",
      "",
      '```powershell title="Example PowerShell"',
      "$hostname = 'nj-pc-computer'",
      "```",
      "",
      SECTION_END_MARKER,
    ].join("\n");

    const parts = splitMarkdownDataviewBlocks(content);
    expect(parts).toEqual([{ type: "markdown", content, key: "markdown-0" }]);

    const html = renderToStaticMarkup(createElement(MarkdownPreview, { content }));
    expect(html).not.toContain('data-starlane-section="true"');
    expect(html).toContain("Example PowerShell");
    expect(html).toContain("nj-pc-computer");
    expect(html).not.toContain("&lt;!-- starlane:section");
    expect(html).not.toContain("&lt;!-- /starlane:section");
  });

  it.each(["left", "center", "right", "expand"])(
    "renders %s portable code placement in Read mode",
    (placement) => {
      const content = `\`\`\`typescript title="qa.ts" placement=${placement}\nconst qa = true;\n\`\`\``;
      const parts = splitMarkdownDataviewBlocks(content);
      expect(parts).toContainEqual(
        expect.objectContaining({ type: "code", placement, title: "qa.ts" }),
      );
      const html = renderToStaticMarkup(createElement(MarkdownPreview, { content }));
      expect(html).toContain(`data-block-placement="${placement}"`);
      expect(html).toContain('data-portable-code-block="true"');
    },
  );

  it("renders placed plain-text code without treating placement as a language", () => {
    const content = "``` placement=right\nplain words\n```";
    expect(splitMarkdownDataviewBlocks(content)).toContainEqual(expect.objectContaining({ type: "code", language: "", placement: "right" }));
    const html = renderToStaticMarkup(createElement(MarkdownPreview, { content }));
    expect(html).toContain('data-block-placement="right"');
  });

  it("positions the table scroll wrapper from portable metadata", () => {
    const html = renderMarkdownPreviewHtml(
      "<!-- starlane-table: compact align=right -->\n| Name | Status |\n| --- | --- |\n| Church | Ready |",
    );
    expect(html).toContain('data-block-placement="right"');
    expect(html).toContain('data-table-style="compact"');
  });

  it("contains Page-view tables in a keyboard-accessible horizontal scroller", () => {
    const css = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");
    const wrapped = wrapPreviewTables(
      '<p>Before</p><table data-table-style="directory"><tbody><tr><td>Wide</td></tr></tbody></table>',
    );

    expect(css).toContain(":is(.tiptap-editor, .markdown-preview) .tableWrapper");
    expect(css).toContain("width: max-content");
    expect(css).toContain("margin-inline: auto");
    expect(css).toContain("max-width: 100%");
    expect(css).toContain("overflow-x: auto");
    expect(wrapped).toContain(
      'class="tableWrapper markdown-table-scroll" role="region" aria-label="Scrollable document table" tabindex="0"',
    );
    expect(wrapped).toContain('<table data-table-style="directory">');
    expect(wrapped).toContain("</table></div>");

    const html = renderToStaticMarkup(
      createElement(MarkdownPreview, {
        content: ["| Header | Texas |", "| --- | --- |", "| column | canada |"].join("\n"),
      }),
    );

    expect(html).toContain('aria-label="Scrollable document table"');
  });

  it("gives Page-view link actions the shared overlay lifecycle", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/MarkdownPreview.tsx"),
      "utf8",
    );

    expect(source).toContain("useOverlayFocusLifecycle");
    expect(source).toContain("containerRef: linkMenuRef");
    expect(source).toContain("triggerRef: linkMenuTriggerRef");
    expect(source).toContain("onClick={closeAndRestoreLinkMenu}");
    expect(source).toContain("useMenuKeyboardNavigation");
    expect(source).toContain("onKeyDown={handleLinkMenuKeyDown}");
    expect(source).toContain('role="menu"');
    expect(source).toContain('aria-label="Link actions"');
  });

  it("keeps Dataview group and column labels readable rather than using banned uppercase eyebrows", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/MarkdownPreview.tsx"),
      "utf8",
    );

    expect(source).not.toContain("font-medium uppercase text-text-muted");
    expect(source).not.toContain("bg-surface-raised/60 text-xs uppercase");
  });

  it("owns accessible loading, result, and retryable error states for collections", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/MarkdownPreview.tsx"),
      "utf8",
    );
    const html = renderToStaticMarkup(
      createElement(
        MemoryRouter,
        undefined,
        createElement(MarkdownPreview, {
          content: ["```dataview", "name: Open work", "Status:Open", "```"].join("\n"),
        }),
      ),
    );

    expect(html).toContain('role="region"');
    expect(html).toContain('aria-label="Open work collection"');
    expect(html).toContain('aria-busy="false"');
    expect(html).toContain('role="status"');
    expect(source).toContain('role="alert"');
    expect(source).toContain("Loading collection results");
    expect(source).toContain("motion-safe:animate-pulse");
    expect(source).toContain("errorRetryable ?");
    expect(source).toContain(
      'errorRetryable\n                ? "Unavailable"\n                : "Needs setup"',
    );
    expect(source).toContain("setRequestVersion((version) => version + 1)");
    expect(source).toContain("Retry");
  });

  it("uses semantic focused links for collection result navigation in every view", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/MarkdownPreview.tsx"),
      "utf8",
    );

    expect(source.match(/to=\{getDataviewResultPath\(result\)\}/g)).toHaveLength(3);
    expect(source.match(/aria-label=\{`Open \$\{result\.title\}`\}/g)).toHaveLength(3);
    expect(source.match(/starlane-collection-link focus-ring/g)).toHaveLength(3);
    expect(source).toContain("focus-ring block after:absolute after:inset-0 after:content-['']");
    expect(source).not.toContain("onClick={() => openResult(result)}");
    expect(source).not.toContain("cursor-pointer border-t");
  });

  it("extracts dataview fences between normal markdown sections", () => {
    const parts = splitMarkdownDataviewBlocks(
      ["# Plan", "", "```dataview", "tag:planning AND ownerName:Eagle", "```", "", "Done."].join(
        "\n",
      ),
    );

    expect(parts).toEqual([
      { type: "markdown", content: "# Plan\n", key: "markdown-0" },
      { type: "dataview", query: "tag:planning AND ownerName:Eagle", key: "dataview-1" },
      { type: "markdown", content: "\nDone.", key: "markdown-2" },
    ]);
  });

  it("extracts a collection from the default transparent writing section", () => {
    const content = [
      SECTION_START_MARKER,
      "",
      "# Planning",
      "",
      "```dataview",
      "name: Workspace pages",
      "title:*",
      "```",
      "",
      SECTION_END_MARKER,
    ].join("\n");

    expect(splitMarkdownDataviewBlocks(content)).toEqual([
      { type: "markdown", content: "\n# Planning\n", key: "markdown-0" },
      { type: "dataview", query: "name: Workspace pages\ntitle:*", key: "dataview-1" },
      { type: "markdown", content: "", key: "markdown-2" },
    ]);

    const html = renderToStaticMarkup(
      createElement(MemoryRouter, undefined, createElement(MarkdownPreview, { content })),
    );
    expect(html).toContain("Workspace pages");
    expect(html).not.toContain("<code>name: Workspace pages");
  });

  it("retains authored section boundaries around collection source", () => {
    const content = [
      "<!-- starlane:section layout=single header=Projects -->",
      "",
      "```dataview",
      "name: Workspace pages",
      "title:*",
      "```",
      "",
      SECTION_END_MARKER,
    ].join("\n");

    expect(splitMarkdownDataviewBlocks(content)).toEqual([
      { type: "markdown", content, key: "markdown-0" },
    ]);
  });

  it("supports dv shorthand and leaves unclosed fences as markdown", () => {
    const parts = splitMarkdownDataviewBlocks(
      ["```dv", "title:Project", "```", "```dataview"].join("\n"),
    );

    expect(parts[0]).toMatchObject({ type: "dataview", query: "title:Project" });
    expect(parts[1]).toMatchObject({ type: "markdown", content: "```dataview" });
  });

  it("splits a generated table of contents directive without changing source headings", () => {
    const parts = splitMarkdownDataviewBlocks(["[[toc]]", "", "# Plan", "", "## Next"].join("\n"));

    expect(parts).toEqual([
      { type: "toc", key: "toc-0" },
      { type: "markdown", content: "\n# Plan\n\n## Next", key: "markdown-1" },
    ]);
  });

  it("renders portable callouts as semantic read-only blocks", () => {
    const html = renderToStaticMarkup(
      createElement(MarkdownPreview, {
        content: ["> [!TIP]", "> Keep the source portable."].join("\n"),
      }),
    );

    expect(html).toContain('data-callout="true"');
    expect(html).toContain('data-callout-kind="tip"');
    expect(html).toContain('aria-label="tip callout"');
    expect(html).toContain("Keep the source portable.");
  });

  it("renders portable display math as a safe read-only block", () => {
    const html = renderToStaticMarkup(
      createElement(MarkdownPreview, { content: ["$$", "\\frac{1}{2}", "$$"].join("\n") }),
    );

    expect(html).toContain('data-math-block="true"');
    expect(html).toContain('data-math-rendered="true"');
    expect(html).toContain("katex");
  });

  it("renders a generated keyboard-navigable table of contents in page view", () => {
    const html = renderToStaticMarkup(
      createElement(MarkdownPreview, {
        content: ["[[toc]]", "", "# Plan", "", "## Next"].join("\n"),
      }),
    );

    expect(html).toContain('aria-label="Table of contents"');
    expect(html).toContain("On this page");
    expect(html).toContain('aria-label="Go to Plan"');
    expect(html).toContain('aria-label="Go to Next"');
    expect(html).toContain("<h1>Plan</h1>");
    expect(html).toContain("<h2>Next</h2>");
  });
});

describe("portable content block parsing", () => {
  it("keeps code tabs and semantic status portable", () => {
    expect(parsePortableCodeTabs("title: Setup\ntab: npm | bash\nnpm install")).toEqual({
      title: "Setup",
      tabs: [{ label: "npm", language: "bash", code: "npm install" }],
    });
    expect(parsePortableCodeTabs("title: Empty\ntab: Bash | bash")).toEqual({
      title: "Empty",
      tabs: [{ label: "Bash", language: "bash", code: "" }],
    });
    expect(parsePortableStatusBlock("state: degraded\nlabel: Search is delayed")).toMatchObject({
      state: "degraded",
      label: "Search is delayed",
    });
  });

  it("rejects unsafe media and retains all gallery layout names", () => {
    expect(parsePortableVideoBlock("src: javascript:alert(1)")).toBeNull();
    expect(
      parsePortableGalleryBlock(
        "layout: coverflow\n---\n![Deploy](https://example.com/deploy.png)",
      ),
    ).toMatchObject({
      layout: "coverflow",
      items: [{ alt: "Deploy", src: "https://example.com/deploy.png" }],
    });
  });
});

describe("parseDataviewBlockConfig", () => {
  it("extracts display options without treating query fields as options", () => {
    expect(
      parseDataviewBlockConfig(
        [
          "view: cards",
          "limit: 5",
          "columns: title, excerpt",
          "title:Project AND tag:roadmap",
        ].join("\n"),
      ),
    ).toEqual({
      name: undefined,
      query: "title:Project AND tag:roadmap",
      view: "cards",
      limit: 5,
      columns: ["title", "excerpt"],
      sort: undefined,
      group: undefined,
    });
  });

  it("parses first-pass computed columns", () => {
    expect(
      parseDataviewBlockConfig(
        [
          "columns: title, updated",
          "column: Age = daysSince(updated)",
          "column: Date = date(updated)",
          "column: Month = month(updated)",
          "column: Week = week(updated)",
          "column: Quarter = quarter(updated)",
          "column: Owner = author",
          "column: Title chars = length(title)",
          "column: Excerpt words = words(excerpt)",
          "tag:roadmap",
        ].join("\n"),
      ),
    ).toEqual({
      name: undefined,
      query: "tag:roadmap",
      view: "table",
      limit: 10,
      columns: [
        "title",
        "updated",
        {
          id: "computed:age:daysSince(updated)",
          label: "Age",
          expression: "daysSince(updated)",
        },
        {
          id: "computed:date:date(updated)",
          label: "Date",
          expression: "date(updated)",
        },
        {
          id: "computed:month:month(updated)",
          label: "Month",
          expression: "month(updated)",
        },
        {
          id: "computed:week:week(updated)",
          label: "Week",
          expression: "week(updated)",
        },
        {
          id: "computed:quarter:quarter(updated)",
          label: "Quarter",
          expression: "quarter(updated)",
        },
        {
          id: "computed:owner:author",
          label: "Owner",
          expression: "author",
        },
        {
          id: "computed:title-chars:titleLength",
          label: "Title chars",
          expression: "titleLength",
        },
        {
          id: "computed:excerpt-words:excerptWords",
          label: "Excerpt words",
          expression: "excerptWords",
        },
      ],
      sort: undefined,
      group: undefined,
    });
  });

  it("clamps limits and falls back to table defaults", () => {
    expect(
      parseDataviewBlockConfig(["view: calendar", "limit: 500", "Status:Draft"].join("\n")),
    ).toEqual({
      name: undefined,
      query: "Status:Draft",
      view: "table",
      limit: 50,
      columns: ["title", "space", "updated"],
      sort: undefined,
      group: undefined,
    });
  });

  it("keeps a named collection definition in portable page Markdown", () => {
    const config = parseDataviewBlockConfig(
      ["name: Open work", 'Status = "Open"', "view: table", "columns: title, updated"].join("\n"),
    );

    expect(config).toMatchObject({
      name: "Open work",
      query: 'Status = "Open"',
      view: "table",
      columns: ["title", "updated"],
    });

    const html = renderToStaticMarkup(
      createElement(
        MemoryRouter,
        undefined,
        createElement(MarkdownPreview, {
          content: ["```dataview", "name: Open work", 'Status = "Open"', "```"].join("\n"),
        }),
      ),
    );
    expect(html).toContain("Open work");
    expect(html).toContain("Status = &quot;Open&quot;");
  });

  it("parses sort and group options for display-only result shaping", () => {
    const config = parseDataviewBlockConfig(
      ["sort: updated desc", "group: space", "columns: title, updated", "tag:roadmap"].join("\n"),
    );
    const results = [
      {
        document_id: "a",
        title: "Alpha",
        space_id: "space-1",
        space_name: "Ops",
        excerpt: "",
        updated_at: "2026-07-01T00:00:00.000Z",
        author: "Eagle",
      },
      {
        document_id: "b",
        title: "Beta",
        space_id: "space-1",
        space_name: "Ops",
        excerpt: "",
        updated_at: "2026-07-05T00:00:00.000Z",
        author: "Eagle",
      },
    ];

    expect(config.sort).toEqual({ field: "updated", direction: "desc" });
    expect(config.group).toBe("space");
    const sorted = sortDataviewResults(results, config.sort);
    expect(sorted.map((result) => result.document_id)).toEqual(["b", "a"]);
    expect(results.map((result) => result.document_id)).toEqual(["a", "b"]);
    expect(groupDataviewResults(sorted, config.group)).toEqual([
      { key: "space:Ops", label: "Ops", results: sorted },
    ]);

    const authorConfig = parseDataviewBlockConfig(
      ["sort: author asc", "group: author", "tag:roadmap"].join("\n"),
    );
    const authorSorted = sortDataviewResults(
      [
        { ...results[0], author: "Riley" },
        { ...results[1], author: "Eagle" },
      ],
      authorConfig.sort,
    );

    expect(authorConfig.sort).toEqual({ field: "author", direction: "asc" });
    expect(authorConfig.group).toBe("author");
    expect(authorSorted.map((result) => result.author)).toEqual(["Eagle", "Riley"]);
    expect(groupDataviewResults(authorSorted, authorConfig.group)).toEqual([
      { key: "author:Eagle", label: "Eagle", results: [authorSorted[0]] },
      { key: "author:Riley", label: "Riley", results: [authorSorted[1]] },
    ]);

    const monthConfig = parseDataviewBlockConfig(["group: month", "tag:roadmap"].join("\n"));
    const yearConfig = parseDataviewBlockConfig(["group: year", "tag:roadmap"].join("\n"));
    const dateBucketResults = [
      { ...results[0], updated_at: "2026-07-01T00:00:00.000Z" },
      { ...results[1], updated_at: "2026-08-05T00:00:00.000Z" },
    ];

    expect(monthConfig.group).toBe("month");
    expect(yearConfig.group).toBe("year");
    expect(groupDataviewResults(dateBucketResults, monthConfig.group)).toEqual([
      { key: "month:2026-07", label: "2026-07", results: [dateBucketResults[0]] },
      { key: "month:2026-08", label: "2026-08", results: [dateBucketResults[1]] },
    ]);
    expect(groupDataviewResults(dateBucketResults, yearConfig.group)).toEqual([
      { key: "year:2026", label: "2026", results: dateBucketResults },
    ]);
  });

  it("sorts by computed column labels and expressions", () => {
    const results = [
      {
        document_id: "a",
        title: "Alpha",
        space_id: "space-1",
        space_name: "Ops",
        excerpt: "Short",
        updated_at: "2026-07-05T00:00:00.000Z",
        author: "Eagle",
      },
      {
        document_id: "b",
        title: "Longer Beta",
        space_id: "space-1",
        space_name: "Ops",
        excerpt: "Longer excerpt",
        updated_at: "2026-07-01T00:00:00.000Z",
        author: "Church",
      },
    ];

    const ageConfig = parseDataviewBlockConfig(
      ["column: Age = daysSince(updated)", "sort: Age desc", "tag:roadmap"].join("\n"),
    );
    expect(ageConfig.sort).toEqual({
      field: { type: "computed", expression: "daysSince(updated)" },
      direction: "desc",
    });
    expect(
      sortDataviewResults(results, ageConfig.sort, new Date("2026-07-07T00:00:00.000Z")).map(
        (result) => result.document_id,
      ),
    ).toEqual(["b", "a"]);

    const titleLengthConfig = parseDataviewBlockConfig(
      ["column: Title chars = length(title)", "sort: Title chars asc", "tag:roadmap"].join("\n"),
    );
    expect(titleLengthConfig.sort).toEqual({
      field: { type: "computed", expression: "titleLength" },
      direction: "asc",
    });
    expect(
      sortDataviewResults(results, titleLengthConfig.sort).map((result) => result.document_id),
    ).toEqual(["a", "b"]);

    const excerptWordsConfig = parseDataviewBlockConfig(
      ["column: Excerpt words = words(excerpt)", "sort: Excerpt words desc", "tag:roadmap"].join(
        "\n",
      ),
    );
    expect(excerptWordsConfig.sort).toEqual({
      field: { type: "computed", expression: "excerptWords" },
      direction: "desc",
    });
    expect(
      sortDataviewResults(results, excerptWordsConfig.sort).map((result) => result.document_id),
    ).toEqual(["b", "a"]);

    const expressionConfig = parseDataviewBlockConfig(
      ["sort: month(updated) asc", "tag:roadmap"].join("\n"),
    );
    expect(expressionConfig.sort).toEqual({
      field: { type: "computed", expression: "month(updated)" },
      direction: "asc",
    });

    const weekConfig = parseDataviewBlockConfig(
      ["sort: week(updated) desc", "tag:roadmap"].join("\n"),
    );
    expect(weekConfig.sort).toEqual({
      field: { type: "computed", expression: "week(updated)" },
      direction: "desc",
    });
  });

  it("evaluates computed columns against result metadata", () => {
    const result = {
      document_id: "a",
      title: "Project Alpha",
      space_id: "space-1",
      space_name: "Ops",
      excerpt: "Launch checklist and owner notes",
      updated_at: "2026-07-05T12:00:00.000Z",
      author: "Eagle",
    };

    expect(
      evaluateDataviewComputedColumn(
        result,
        {
          id: "computed:age:daysSince(updated)",
          label: "Age",
          expression: "daysSince(updated)",
        },
        new Date("2026-07-07T00:00:00.000Z"),
      ),
    ).toBe("1 day");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:date:date(updated)",
        label: "Date",
        expression: "date(updated)",
      }),
    ).toBe("2026-07-05");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:weekday:weekday(updated)",
        label: "Weekday",
        expression: "weekday(updated)",
      }),
    ).toBe(
      new Date("2026-07-05T12:00:00.000Z").toLocaleDateString(undefined, { weekday: "short" }),
    );
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:quarter:quarter(updated)",
        label: "Quarter",
        expression: "quarter(updated)",
      }),
    ).toBe("Q3 2026");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:week:week(updated)",
        label: "Week",
        expression: "week(updated)",
      }),
    ).toBe("2026-W27");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:link:link",
        label: "Link",
        expression: "link",
      }),
    ).toBe("/spaces/space-1/docs/a");
    expect(
      getDataviewResultPath({ ...result, space_id: "space/one", document_id: "doc/one" }),
    ).toBe("/spaces/space%2Fone/docs/doc%2Fone");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:author:author",
        label: "Author",
        expression: "author",
      }),
    ).toBe("Eagle");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:space:space",
        label: "Space",
        expression: "space",
      }),
    ).toBe("Ops");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:title:titleLength",
        label: "Title",
        expression: "titleLength",
      }),
    ).toBe("13");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:excerpt:excerptLength",
        label: "Excerpt",
        expression: "excerptLength",
      }),
    ).toBe("32");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:title-words:titleWords",
        label: "Title words",
        expression: "titleWords",
      }),
    ).toBe("2");
    expect(
      evaluateDataviewComputedColumn(result, {
        id: "computed:excerpt-words:excerptWords",
        label: "Excerpt words",
        expression: "excerptWords",
      }),
    ).toBe("5");
  });
});
