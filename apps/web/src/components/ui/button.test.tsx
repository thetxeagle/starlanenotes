import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import { BUTTON_DANGER_VARIANT_CLASS, Button } from "./button";

describe("Button", () => {
  it("uses semantic danger tokens for the danger variant", () => {
    expect(BUTTON_DANGER_VARIANT_CLASS).toContain("bg-danger");
    expect(BUTTON_DANGER_VARIANT_CLASS).toContain("hover:bg-danger");
    expect(BUTTON_DANGER_VARIANT_CLASS).not.toMatch(/red-/);

    const html = renderToStaticMarkup(<Button variant="danger">Delete</Button>);

    expect(html).toContain("bg-danger");
    expect(html).toContain("hover:bg-danger");
    expect(html).not.toContain("bg-red");
  });
});
