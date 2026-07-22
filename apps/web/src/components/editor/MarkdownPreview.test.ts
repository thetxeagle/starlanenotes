import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import {
  evaluateDataviewComputedColumn,
  applyExcalidrawSidecarScenes,
  getDataviewResultPath,
  getExcalidrawDirectiveFilenames,
  groupDataviewResults,
  MarkdownPreview,
  parseDataviewBlockConfig,
  sortDataviewResults,
  splitMarkdownDataviewBlocks,
} from "./MarkdownPreview";

describe("splitMarkdownDataviewBlocks", () => {
  it("renders highlighted code fences in page view markdown", () => {
    const html = renderToStaticMarkup(
      createElement(MarkdownPreview, {
        content: ["```ts", "const answer = 42;", "```"].join("\n"),
      }),
    );

    expect(html).toContain('class="language-ts"');
    expect(html).toContain("hljs-keyword");
    expect(html).toContain("hljs-number");
  });

  it("renders persisted Excalidraw directives in page view markdown", () => {
    const html = renderToStaticMarkup(
      createElement(MarkdownPreview, {
        content: "![[excalidraw:system-map.excalidraw]]",
      }),
    );

    expect(html).toContain('data-excalidraw-canvas="true"');
    expect(html).toContain('role="button"');
    expect(html).toContain('data-excalidraw-thumbnail="true"');
    expect(html).toContain("system-map.excalidraw");
    expect(html).toContain("aspect-video");
    expect(html).not.toContain("Open drawing</button>");
  });

  it("renders an intentional empty Excalidraw preview state in page view", () => {
    const html = renderToStaticMarkup(
      createElement(MarkdownPreview, {
        content: "![[excalidraw:empty-map.excalidraw]]",
      }),
    );

    expect(html).toContain('data-excalidraw-empty-preview="true"');
    expect(html).toContain("No thumbnail yet. Open drawing to edit.");
    expect(html).toContain("bg-surface-raised");
  });

  it("renders Excalidraw scene thumbnails in page view markdown", () => {
    const scene = {
      elements: [
        {
          id: "text-1",
          type: "text",
          x: 12,
          y: 24,
          width: 220,
          height: 48,
          text: "Launch map",
          strokeColor: "#c9d1d9",
          fontSize: 24,
        },
      ],
      appState: {},
      files: {},
    };
    const content = [
      "![[excalidraw:launch-map.excalidraw]]",
      "",
      "```excalidraw-json",
      JSON.stringify(scene),
      "```",
    ].join("\n");
    const html = renderToStaticMarkup(createElement(MarkdownPreview, { content }));

    expect(html).toContain('data-excalidraw-preview="true"');
    expect(html).toContain("<text");
    expect(html).toContain("Launch map");
  });

  it("hydrates Excalidraw sidecar scenes for page-view thumbnails", () => {
    const scene = JSON.stringify({
      elements: [{ id: "box-1", type: "rectangle", x: 0, y: 0, width: 80, height: 40 }],
      appState: {},
      files: {},
    });
    const hydrated = applyExcalidrawSidecarScenes(
      "![[excalidraw:launch-map.excalidraw]]",
      new Map([["launch-map.excalidraw", scene]]),
    );

    expect(getExcalidrawDirectiveFilenames(hydrated)).toEqual(["launch-map.excalidraw"]);
    expect(hydrated).toContain("```excalidraw-json");
    expect(hydrated).toContain('"box-1"');
  });

  it("does not duplicate embedded Excalidraw scene blocks", () => {
    const content = [
      "![[excalidraw:launch-map.excalidraw]]",
      "",
      "```excalidraw-json",
      '{"elements":[]}',
      "```",
    ].join("\n");

    expect(
      applyExcalidrawSidecarScenes(
        content,
        new Map([["launch-map.excalidraw", '{"elements":[{"id":"sidecar"}]}']]),
      ),
    ).toBe(content);
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

  it("supports dv shorthand and leaves unclosed fences as markdown", () => {
    const parts = splitMarkdownDataviewBlocks(["```dv", "title:Project", "```", "```dataview"].join("\n"));

    expect(parts[0]).toMatchObject({ type: "dataview", query: "title:Project" });
    expect(parts[1]).toMatchObject({ type: "markdown", content: "```dataview" });
  });
});

describe("parseDataviewBlockConfig", () => {
  it("extracts display options without treating query fields as options", () => {
    expect(
      parseDataviewBlockConfig(
        ["view: cards", "limit: 5", "columns: title, excerpt", "title:Project AND tag:roadmap"].join(
          "\n",
        ),
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
    expect(parseDataviewBlockConfig(["view: calendar", "limit: 500", "Status:Draft"].join("\n"))).toEqual({
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
    expect(html).toContain('Status = &quot;Open&quot;');
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
    expect(sorted.map((result) => result.document_id)).toEqual([
      "b",
      "a",
    ]);
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
    expect(sortDataviewResults(results, titleLengthConfig.sort).map((result) => result.document_id)).toEqual([
      "a",
      "b",
    ]);

    const excerptWordsConfig = parseDataviewBlockConfig(
      ["column: Excerpt words = words(excerpt)", "sort: Excerpt words desc", "tag:roadmap"].join(
        "\n",
      ),
    );
    expect(excerptWordsConfig.sort).toEqual({
      field: { type: "computed", expression: "excerptWords" },
      direction: "desc",
    });
    expect(sortDataviewResults(results, excerptWordsConfig.sort).map((result) => result.document_id)).toEqual([
      "b",
      "a",
    ]);

    const expressionConfig = parseDataviewBlockConfig(["sort: month(updated) asc", "tag:roadmap"].join("\n"));
    expect(expressionConfig.sort).toEqual({
      field: { type: "computed", expression: "month(updated)" },
      direction: "asc",
    });

    const weekConfig = parseDataviewBlockConfig(["sort: week(updated) desc", "tag:roadmap"].join("\n"));
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
    ).toBe(new Date("2026-07-05T12:00:00.000Z").toLocaleDateString(undefined, { weekday: "short" }));
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
    expect(getDataviewResultPath({ ...result, space_id: "space/one", document_id: "doc/one" })).toBe(
      "/spaces/space%2Fone/docs/doc%2Fone",
    );
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
