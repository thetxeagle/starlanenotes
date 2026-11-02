import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import {
  BUTTON_DANGER_VARIANT_CLASS,
  BUTTON_RESPONSIVE_HEIGHT_CLASS,
  Button,
  RESPONSIVE_TOUCH_ACTION_CLASS,
  RESPONSIVE_TOUCH_ICON_ACTION_CLASS,
} from "./button";

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

  it("provides responsive touch classes without changing desktop density", () => {
    const html = renderToStaticMarkup(<Button size="sm">Refresh</Button>);

    expect(BUTTON_RESPONSIVE_HEIGHT_CLASS).toBe("min-h-11 sm:min-h-0");
    expect(html).toContain(BUTTON_RESPONSIVE_HEIGHT_CLASS);
    expect(RESPONSIVE_TOUCH_ACTION_CLASS).toContain("min-h-11");
    expect(RESPONSIVE_TOUCH_ACTION_CLASS).toContain("sm:min-h-0");
    expect(RESPONSIVE_TOUCH_ICON_ACTION_CLASS).toContain("min-w-11");
    expect(RESPONSIVE_TOUCH_ICON_ACTION_CLASS).toContain("sm:min-w-0");
  });
});
