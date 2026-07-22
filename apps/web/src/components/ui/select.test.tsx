import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Select, SELECT_CLASS } from "./select";

describe("Select", () => {
  it("uses the shared form-control styling contract", () => {
    expect(SELECT_CLASS).toContain("rounded-input");
    expect(SELECT_CLASS).toContain("border-border");
    expect(SELECT_CLASS).toContain("focus:ring-2");
    expect(SELECT_CLASS).not.toContain("bg-transparent");
  });

  it("renders native select options with custom classes preserved", () => {
    const html = renderToStaticMarkup(
      <Select aria-label="Sort" className="text-xs">
        <option value="a">A</option>
      </Select>,
    );

    expect(html).toContain("text-xs");
    expect(html).toContain("rounded-input");
    expect(html).toContain("<option");
  });
});
