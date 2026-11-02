import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import {
  SearchModal,
  SearchFailureState,
  applySearchQuerySuggestion,
  canSaveSearchViewName,
  formatSearchSpaceNames,
  getActiveSearchToken,
  getActiveSearchSpaceId,
  getSearchModalPresetFromEvent,
  getSearchBuilderFilterOptions,
  getSearchBuilderOperatorOptions,
  getSearchBuilderValueSuggestions,
  getDefaultSavedSearchViewName,
  getSearchQuerySuggestions,
  getSearchResultOptionId,
  getSearchResultDocumentPath,
  getStarredSearchResultDocumentPath,
  isCurrentWorkspaceSearchRequest,
  resolveSearchSpaceFilter,
  SearchSaveViewPanel,
  composeVisualSearchQuery,
  buildVisualMultiValueFilter,
  splitVisualSearchQuery,
} from "./SearchModal";

vi.mock("@/lib/savedSearches", () => ({
  deleteSavedSearchView: vi.fn(() => []),
  readSavedSearchViews: vi.fn(() => []),
  saveSearchView: vi.fn(() => []),
}));

vi.mock("@/lib/starredDocuments", () => ({
  filterStarredDocuments: vi.fn(() => []),
  readStarredDocuments: vi.fn(() => []),
  toggleStarredDocumentSynced: vi.fn(),
}));

describe("SearchModal", () => {
  it("renders an accessible retry state for failed searches", () => {
    const html = renderToStaticMarkup(<SearchFailureState onRetry={() => undefined} />);

    expect(html).toContain('role="alert"');
    expect(html).toContain("Search couldn&#x27;t be completed.");
    expect(html).toContain("Retry search");
  });

  it("distinguishes Dataview and document request failures from empty results", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/SearchModal.tsx"), "utf8");

    expect(source).toMatch(
      /queryDataview\(dataviewQuery, 50\)[\s\S]*?catch \{\s*if \(!canPublish\(\)\) return;\s*setResults\(\[\]\);\s*setSearchError\(true\);/,
    );
    expect(source).toMatch(
      /searchDocuments\(parsed\.query, 50, effectiveFilters\)[\s\S]*?catch \{\s*if \(!canPublish\(\)\) return;\s*setResults\(\[\]\);\s*setSearchError\(true\);/,
    );
    expect(source).toContain("setSearchError(false);");
    expect(source).toContain("<SearchFailureState onRetry={() => doSearch(query, filters)} />");
    expect(source).toContain('"Search failed"');
  });

  it("prevents superseded search and suggestion requests from publishing stale state", () => {
    let currentRequestId = 1;

    expect(isCurrentWorkspaceSearchRequest(1, currentRequestId)).toBe(true);
    currentRequestId += 1;
    expect(isCurrentWorkspaceSearchRequest(1, currentRequestId)).toBe(false);
    expect(isCurrentWorkspaceSearchRequest(2, currentRequestId)).toBe(true);

    const source = readFileSync(resolve(process.cwd(), "src/components/SearchModal.tsx"), "utf8");
    expect(source).toContain("const requestId = ++searchRequestIdRef.current;");
    expect(source).toContain("const requestId = ++suggestionRequestIdRef.current;");
    expect(source).toContain("if (!canPublish()) return;");
    expect(source).toContain("if (canPublish()) setLoading(false);");
  });

  it("builds encoded document paths for search result navigation", () => {
    expect(
      getSearchResultDocumentPath({
        document_id: "doc/one",
        title: "Doc",
        space_id: "space/one",
        space_name: "Space",
        excerpt: "",
        updated_at: "2026-07-11T00:00:00.000Z",
        author: "System",
      }),
    ).toBe("/spaces/space%2Fone/docs/doc%2Fone");

    expect(
      getStarredSearchResultDocumentPath({
        id: "doc/one",
        title: "Doc",
        spaceId: "space/one",
        starred_at: "2026-07-11T00:00:00.000Z",
      }),
    ).toBe("/spaces/space%2Fone/docs/doc%2Fone");

    expect(getSearchResultOptionId("doc/one")).toBe("search-result-doc%2Fone");
    expect(getSearchResultOptionId("space/one:doc/one", true)).toBe(
      "search-starred-result-space%2Fone%3Adoc%2Fone",
    );
  });

  it("uses Starlane save-view naming helpers instead of browser prompts", () => {
    expect(getDefaultSavedSearchViewName("tag:work folder:project", false)).toBe(
      "tag:work folder:project",
    );
    expect(getDefaultSavedSearchViewName("   ", true)).toBe("Favorite pages");
    expect(getDefaultSavedSearchViewName("   ", false)).toBe("Saved search");
    expect(getDefaultSavedSearchViewName("x".repeat(80), false)).toHaveLength(48);
    expect(canSaveSearchViewName("Roadmap")).toBe(true);
    expect(canSaveSearchViewName("   ")).toBe(false);
  });

  it("preserves sidebar and property-search filters in a shared event preset", () => {
    expect(getSearchModalPresetFromEvent({ tag: "project", spaceId: "space/one" }, 42)).toEqual({
      nonce: 42,
      filters: {
        tag: "project",
        space_id: "space/one",
      },
    });
    expect(getSearchModalPresetFromEvent(undefined, 7)).toEqual({
      nonce: 7,
      filters: {
        tag: undefined,
        space_id: undefined,
      },
    });
  });

  it("derives the active space from workspace routes without broadening the search", () => {
    expect(getActiveSearchSpaceId("/spaces/space%2Fone/docs/doc-1")).toBe("space/one");
    expect(getActiveSearchSpaceId("/profile")).toBeUndefined();
  });

  it("keeps save-view entry as a named form region inside search, not a nested dialog", () => {
    const html = renderToStaticMarkup(
      <SearchSaveViewPanel
        name="Roadmap"
        onNameChange={() => undefined}
        onCancel={() => undefined}
        onSave={() => undefined}
      />,
    );

    expect(html).toContain("<section");
    expect(html).toContain('aria-labelledby="save-search-view-title"');
    expect(html).toContain('aria-label="Saved search view name"');
    expect(html).not.toContain('role="dialog"');
    expect(html).toContain('type="submit"');
  });

  it("uses the shared search textbox for the primary query field", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SearchModal open onClose={() => {}} />
      </MemoryRouter>,
    );

    expect(html).toContain('type="search"');
    expect(html).toContain('aria-label="Search pages"');
    expect(html).toContain('role="combobox"');
    expect(html).toContain('aria-controls="search-query-suggestions"');
    expect(html).toContain("Search pages by title or content");
    expect(html).toContain('aria-label="Choose active search space"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-0");
    expect(html).toContain("focus:ring-offset-0");
  });

  it("keeps common filters visible and tucks operators behind Advanced search", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SearchModal open onClose={() => {}} />
      </MemoryRouter>,
    );

    expect(html).toContain("Find pages by their words, then narrow the results with filters.");
    expect(html).toContain('aria-label="Search query builder"');
    expect(html).toContain("Add a filter");
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain('aria-controls="advanced-search-controls"');
    expect(html).toContain('id="advanced-search-controls" hidden=""');
    expect(html).toContain("Advanced search");
    expect(html.indexOf("Add a filter")).toBeLessThan(html.indexOf("Advanced search"));
    expect(html.indexOf('id="advanced-search-controls" hidden=""')).toBeLessThan(
      html.indexOf("Add an operator"),
    );
    expect(html).toContain("min-h-[min(30rem,calc(100dvh-2rem))]");
    expect(html).toContain("max-h-[min(48rem,calc(100dvh-2rem))]");
    expect(html).not.toContain('aria-label="Search tag filter"');
    expect(html).not.toContain('aria-label="Search property filter"');
    expect(html).not.toContain('aria-label="Search property value filter"');
    expect(html).toContain('aria-label="Close search"');
  });

  it("keeps Space in its dedicated selector instead of duplicating it as a filter", () => {
    expect(getSearchBuilderFilterOptions().map((option) => option.label)).toEqual([
      "Tag",
      "Property",
      "Folder",
      "User",
      "Created date",
      "Updated date",
      "Page state",
      "Has",
      "Title",
      "Favorites",
    ]);
  });

  it("uses typed free-text filter values instead of repeating an empty operator suggestion", () => {
    expect(getSearchBuilderValueSuggestions("title:", "Launch")).toEqual([]);
    expect(getSearchBuilderValueSuggestions("tag:", "Planning")).toEqual([]);
    expect(
      getSearchBuilderValueSuggestions("tag:", "road", {
        tags: [{ id: "tag-1", name: "Roadmap", color: "blue" }],
      })[0],
    ).toMatchObject({ insertion: "tag:Roadmap" });
  });

  it("keeps desktop Search on the shared dialog lifecycle", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/SearchModal.tsx"), "utf8");

    expect(source).toContain('presentation?: "dialog" | "fullscreen"');
    expect(source).toContain('<DialogShell labelledBy="search-modal-title" maxWidth="settings"');
    expect(source).not.toContain('presentation === "popover"');
    expect(source).not.toContain("PopoverAnchor");
    expect(source).not.toContain("popoverInputRef");
  });

  it("renders exactly one editable Search composer", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SearchModal open onClose={() => {}} />
      </MemoryRouter>,
    );

    expect(html.match(/aria-label="Search pages"/g)).toHaveLength(1);
    expect(html.match(/type="search"/g)).toHaveLength(1);
    expect(html).toContain('id="search-results-title"');
    expect(html).not.toContain("Search Starlane Notes with SNQL");
  });

  it("keeps full-screen mobile search touch-safe while preserving starred search scope", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SearchModal open onClose={() => {}} presentation="fullscreen" />
      </MemoryRouter>,
    );

    expect(html).toContain('aria-modal="true"');
    expect(html).toContain("fixed inset-0");
    expect(html).toContain("z-[60]");
    expect(html).toContain("pt-[max(0.5rem,env(safe-area-inset-top))]");
    expect(html).toContain("Search pages by title or content");
    expect(html).toContain("Close search");
    expect(html).toContain("size-10");
    expect(html).toContain("Find pages by their words, then narrow the results with filters.");
    expect(html).toContain("Advanced search");
    expect(html).toContain('id="advanced-search-controls" hidden=""');
    expect(html).toContain("min-h-48");
    expect(html).toContain("min-h-12");
    expect(html).not.toContain('aria-label="Search tag filter"');
    expect(html).not.toContain('aria-label="Search property filter"');
    expect(html).not.toContain('aria-label="Search property value filter"');
    expect(html).not.toContain("max-h-[calc(100dvh-1.5rem)]");
  });

  it("opens Advanced search for an active advanced query preset", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SearchModal
          open
          onClose={() => {}}
          preset={{ nonce: 4, query: "dv:tag:Planning OR roadmap" }}
        />
      </MemoryRouter>,
    );

    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain('id="advanced-search-controls"');
    expect(html).toContain("Add an operator");
    expect(html).toContain("Dataview query with");
  });

  it("offers supported query operators and replaces only the active token", () => {
    expect(getActiveSearchToken("roadmap spa")).toEqual({ value: "spa", start: 8 });
    expect(getSearchQuerySuggestions("spa").map((item) => item.label)).toContain("Space");
    expect(
      applySearchQuerySuggestion("roadmap spa", {
        insertion: "space:",
        complete: false,
      }),
    ).toBe("roadmap space:");
    expect(
      applySearchQuerySuggestion("roadmap is:sta", {
        insertion: "is:starred",
        complete: true,
      }),
    ).toBe("roadmap is:starred ");
  });

  it("exposes every supported logical operator", () => {
    expect(getSearchBuilderOperatorOptions()).toEqual([
      { value: "AND", label: "AND", detail: "all conditions" },
      { value: "OR", label: "OR", detail: "any condition" },
      { value: "NOT", label: "NOT", detail: "exclude the next condition" },
    ]);
  });

  it("offers contextual spaces, states, dates, tags, and properties", () => {
    const data = {
      spaces: [
        {
          id: "space-1",
          name: "Harley's Space",
          slug: "harleys-space",
          visibility: "internal",
        },
      ],
      tags: [{ id: "tag-1", name: "Roadmap", color: "blue" }],
      properties: [{ id: "property-1", name: "Status", property_type: "select", sort_order: 0 }],
      now: new Date("2026-07-25T12:00:00.000Z"),
    };

    expect(getSearchQuerySuggestions("space", data)[0]).toMatchObject({
      label: "Harley's Space",
      insertion: 'space:"Harley\'s Space"',
    });
    expect(getSearchQuerySuggestions("tag:road", data)[0]).toMatchObject({
      label: "Roadmap",
      insertion: "tag:Roadmap",
    });
    expect(getSearchQuerySuggestions("property:sta", data)[0]).toMatchObject({
      label: "Status",
      insertion: "property:Status",
    });
    expect(getSearchQuerySuggestions("is:sta", data)[0]).toMatchObject({
      label: "starred",
      insertion: "is:starred",
    });
    expect(getSearchQuerySuggestions("updated:", data)[0]).toMatchObject({
      label: "Today",
      insertion: "updated:2026-07-25",
    });
  });

  it("offers fuzzy folder and user values from real workspace metadata", () => {
    const data = {
      folders: [
        { id: "folder-1", name: "Docker Containers", spaceId: "space-1", spaceName: "Lab" },
      ],
      users: [{ id: "user-1", display_name: "Harley Frank", email: "harley@example.com" }],
    };

    expect(getSearchQuerySuggestions("folder:docker", data)[0]).toMatchObject({
      label: "Docker Containers",
      insertion: 'folder:"Docker Containers"',
    });
    expect(getSearchQuerySuggestions("user:har", data)[0]).toMatchObject({
      label: "Harley Frank",
      insertion: 'user:"Harley Frank"',
    });
  });

  it("splits SNQL filters and boolean operators into editable visual blocks", () => {
    const visual = splitVisualSearchQuery(
      'release tag:"Docker Containers" AND folder:Software user:"Harley Frank"',
    );

    expect(visual.text).toBe("release");
    expect(visual.tokens.map((token) => token.label)).toEqual([
      "tag: Docker Containers",
      "AND",
      "folder: Software",
      "user: Harley Frank",
    ]);
    expect(composeVisualSearchQuery(visual.tokens, visual.text)).toBe(
      'tag:"Docker Containers" AND folder:Software user:"Harley Frank" release',
    );
  });

  it("keeps multi-value filters readable and promotes boolean groups to advanced SNQL", () => {
    const group = buildVisualMultiValueFilter("tag:", ["tag:Planning", 'tag:"Release Notes"']);
    const query = composeVisualSearchQuery([group], "roadmap");
    const visual = splitVisualSearchQuery(query);

    expect(query).toBe('dv:(tag:Planning OR tag:"Release Notes") roadmap');
    expect(visual.advanced).toBe(true);
    expect(visual.tokens).toMatchObject([
      {
        kind: "group",
        raw: '(tag:Planning OR tag:"Release Notes")',
        label: "tag: Planning, Release Notes",
      },
    ]);
    expect(composeVisualSearchQuery(visual.tokens, visual.text, visual.advanced)).toBe(query);
  });

  it("promotes OR and NOT blocks to advanced SNQL without changing AND-only searches", () => {
    const conditionToken = splitVisualSearchQuery("tag:Planning").tokens[0];
    const andToken = splitVisualSearchQuery("AND").tokens[0];
    const orToken = splitVisualSearchQuery("OR").tokens[0];
    const notToken = splitVisualSearchQuery("NOT").tokens[0];

    expect(composeVisualSearchQuery([conditionToken, andToken], "roadmap")).toBe(
      "tag:Planning AND roadmap",
    );
    expect(composeVisualSearchQuery([conditionToken, orToken], "roadmap")).toBe(
      "dv:tag:Planning OR roadmap",
    );
    expect(composeVisualSearchQuery([notToken], "draft")).toBe("dv:NOT draft");
  });

  it("shows readable space names while resolving them to stable search IDs", () => {
    const spaces = [
      {
        id: "774be8ac-73ce-479f-8cfd-20f0af606f03",
        name: "Harley Frank's Space",
        slug: "harley-franks-space",
        visibility: "private",
      },
    ];

    expect(formatSearchSpaceNames("space:774be8ac-73ce-479f-8cfd-20f0af606f03", spaces)).toBe(
      'space:"Harley Frank\'s Space"',
    );
    expect(
      resolveSearchSpaceFilter({ space_id: "Harley Frank's Space", tag: "Planning" }, spaces),
    ).toEqual({
      space_id: "774be8ac-73ce-479f-8cfd-20f0af606f03",
      tag: "Planning",
    });
    expect(resolveSearchSpaceFilter({ space_id: "harley-franks-space" }, spaces)).toEqual({
      space_id: "774be8ac-73ce-479f-8cfd-20f0af606f03",
    });
  });

  it("advertises the existing advanced boolean and fuzzy query mode", () => {
    expect(getSearchQuerySuggestions("dv")[0]).toMatchObject({
      id: "operator-dataview",
      label: "Advanced query",
      detail: "Use AND, OR, NOT, =, is not, *, or fuzzy ~",
      insertion: "dv:",
      complete: false,
    });
  });
});
