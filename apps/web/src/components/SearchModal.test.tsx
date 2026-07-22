import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import {
  SearchModal,
  canSaveSearchViewName,
  getDefaultSavedSearchViewName,
  getSearchResultDocumentPath,
  getStarredSearchResultDocumentPath,
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
  });

  it("uses Starlane save-view naming helpers instead of browser prompts", () => {
    expect(getDefaultSavedSearchViewName("tag:work folder:project", false)).toBe(
      "tag:work folder:project",
    );
    expect(getDefaultSavedSearchViewName("   ", true)).toBe("Starred documents");
    expect(getDefaultSavedSearchViewName("   ", false)).toBe("Saved search");
    expect(getDefaultSavedSearchViewName("x".repeat(80), false)).toHaveLength(48);
    expect(canSaveSearchViewName("Roadmap")).toBe(true);
    expect(canSaveSearchViewName("   ")).toBe(false);
  });

  it("uses the shared search textbox for the primary query field", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SearchModal open onClose={() => {}} />
      </MemoryRouter>,
    );

    expect(html).toContain('type="search"');
    expect(html).toContain('aria-label="Search documents"');
    expect(html).toContain("Search documents...");
    expect(html).toContain("pl-9");
    expect(html).toContain("rounded-input");
  });

  it("keeps structured filter fields in the full dialog presentation", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SearchModal open onClose={() => {}} />
      </MemoryRouter>,
    );

    expect(html).toContain('aria-label="Search tag filter"');
    expect(html).toContain('aria-label="Search property filter"');
    expect(html).toContain('aria-label="Search property value filter"');
    expect(html.match(/rounded-input/g)?.length ?? 0).toBeGreaterThanOrEqual(4);
  });

  it("can render as a web search dropdown surface", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SearchModal open onClose={() => {}} presentation="popover" />
      </MemoryRouter>,
    );

    expect(html).toContain('aria-modal="false"');
    expect(html).toContain("starlane-menu-surface");
    expect(html).toContain("top-full");
    expect(html).not.toContain('aria-label="Search tag filter"');
    expect(html).not.toContain('aria-label="Search property filter"');
    expect(html).not.toContain('aria-label="Search property value filter"');
  });

  it("uses one SNQL field in the full-screen mobile search surface", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <SearchModal open onClose={() => {}} presentation="fullscreen" />
      </MemoryRouter>,
    );

    expect(html).toContain('aria-modal="true"');
    expect(html).toContain("fixed inset-0");
    expect(html).toContain("pt-[max(0.5rem,env(safe-area-inset-top))]");
    expect(html).toContain('Search SNQL: &quot;notes&quot; tag:work folder:project has:attachment');
    expect(html).toContain("Close search");
    expect(html).not.toContain('aria-label="Search tag filter"');
    expect(html).not.toContain('aria-label="Search property filter"');
    expect(html).not.toContain('aria-label="Search property value filter"');
    expect(html).not.toContain("max-h-[calc(100dvh-1.5rem)]");
  });
});
