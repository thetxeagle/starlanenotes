import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS } from "./form-control";
import { Input, INPUT_CLASS } from "./input";

describe("Input", () => {
  it("uses the shared responsive form-control contract", () => {
    const html = renderToStaticMarkup(<Input aria-label="Name" />);

    expect(INPUT_CLASS).toContain(RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS);
    expect(html).toContain("min-h-11");
    expect(html).toContain("sm:min-h-0");
    expect(html).toContain("focus:ring-2");
  });
});
