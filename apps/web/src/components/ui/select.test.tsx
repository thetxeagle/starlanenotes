import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Select, SELECT_CLASS } from "./select";
import { RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS } from "./form-control";

describe("Select", () => {
  it("uses the shared form-control styling contract", () => {
    expect(SELECT_CLASS).toContain("rounded-input");
    expect(SELECT_CLASS).toContain("border-border");
    expect(SELECT_CLASS).toContain("focus:ring-2");
    expect(SELECT_CLASS).toContain(RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS);
    expect(SELECT_CLASS).not.toContain("bg-transparent");
  });

  it("renders the anchored menu trigger with custom classes preserved", () => {
    const html = renderToStaticMarkup(
      <Select aria-label="Sort" className="text-xs">
        <option value="a">A</option>
      </Select>,
    );

    expect(html).toContain("text-xs");
    expect(html).toContain("rounded-input");
    expect(html).toContain('data-starlane-select-control="true"');
    expect(html).toContain('aria-haspopup="listbox"');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("<svg");
    expect(html).toContain(">A</span>");
    expect(html).not.toContain("<select");
    expect(html).not.toContain("<option");
  });

  it("preserves an associated visible label instead of deriving aria-label from the id", () => {
    const html = renderToStaticMarkup(
      <>
        <label htmlFor="sort-kind">Sort order</label>
        <Select id="sort-kind" value="title">
          <option value="title">Title</option>
        </Select>
      </>,
    );

    expect(html).toContain('<label for="sort-kind">Sort order</label>');
    expect(html).toContain('<button id="sort-kind"');
    expect(html).not.toContain('aria-label="sort-kind"');
    expect(html).not.toContain('aria-label="Select option"');
  });

  it("forwards explicit aria-label and aria-labelledby without conflating them", () => {
    const html = renderToStaticMarkup(
      <>
        <span id="sort-label">Sort order</span>
        <Select id="sort-kind" aria-labelledby="sort-label" value="title">
          <option value="title">Title</option>
        </Select>
        <Select aria-label="Alternative sort order" value="title">
          <option value="title">Title</option>
        </Select>
      </>,
    );

    expect(html).toContain('aria-labelledby="sort-label"');
    expect(html).toContain('aria-label="Alternative sort order"');
    expect(html).not.toContain('aria-label="sort-label"');
  });
});
