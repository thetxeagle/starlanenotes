import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  collectDocumentContentsHeadings,
  DocumentContents,
  getCurrentDocumentHeadingIndex,
} from "./DocumentContents";

describe("DocumentContents", () => {
  it("collects document h1-h6 headings and disambiguates duplicate labels", () => {
    const makeHeading = (tagName: string, textContent: string, inNav = false) => ({
      tagName,
      textContent,
      id: "",
      closest: () => inNav ? ({ tagName: "NAV" } as Element) : null,
    }) as unknown as HTMLElement;
    const root = {
      querySelectorAll: () => [
        makeHeading("H1", "Page title"),
        makeHeading("H2", "Operations"),
        makeHeading("H3", "Recovery"),
        makeHeading("H2", "Operations"),
        makeHeading("H2", "Contents panel heading", true),
        makeHeading("H4", "Appendix"),
        makeHeading("H5", "Nested notes"),
        makeHeading("H6", "Implementation detail"),
        makeHeading("H7", "Outside HTML heading range"),
      ].filter((heading) => /^H[1-6]$/.test(heading.tagName)),
    } as unknown as HTMLElement;

    expect(collectDocumentContentsHeadings(root).map(({ id, title, level }) => ({ id, title, level }))).toEqual([
      { id: "page-title", title: "Page title", level: 1 },
      { id: "operations", title: "Operations", level: 2 },
      { id: "recovery", title: "Recovery", level: 3 },
      { id: "operations-2", title: "Operations", level: 2 },
      { id: "appendix", title: "Appendix", level: 4 },
      { id: "nested-notes", title: "Nested notes", level: 5 },
      { id: "implementation-detail", title: "Implementation detail", level: 6 },
    ]);
  });

  it("keeps a closed rail out of the rendered tree", () => {
    expect(
      renderToStaticMarkup(
        createElement(DocumentContents, {
          containerRef: { current: {} as HTMLElement },
          contentKey: "page-a",
          open: false,
          onClose: () => undefined,
        }),
      ),
    ).toBe("");
  });

  it("marks the last heading that has reached the reading threshold as current", () => {
    const headings = [120, 340, 560].map((top, index) => ({
      id: `section-${index + 1}`,
      title: `Section ${index + 1}`,
      level: 2 as const,
      element: { getBoundingClientRect: () => ({ top }) } as unknown as HTMLElement,
    }));

    expect(getCurrentDocumentHeadingIndex(headings, 350)).toBe(1);
    expect(getCurrentDocumentHeadingIndex(headings, 600)).toBe(2);
  });
});
