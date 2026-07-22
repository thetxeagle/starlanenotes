import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SearchInput } from "./search-input";

describe("SearchInput", () => {
  it("renders a consistent search textbox", () => {
    const html = renderToStaticMarkup(
      <SearchInput aria-label="Search notes" placeholder="Search notes" value="" readOnly />,
    );

    expect(html).toContain('type="search"');
    expect(html).toContain('aria-label="Search notes"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("pl-9");
  });
});
