import { describe, expect, it } from "vitest";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_ACTIVE_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
  CONTROL_CHROME_SURFACE_CLASS,
  CONTROL_CHROME_TOUCH_ACTION_CLASS,
  CONTROL_CHROME_TOOLTIP_CLASS,
} from "./control-chrome";

describe("shared control chrome", () => {
  it("keeps toolbars, tooltips, and button groups on one surface language", () => {
    expect(CONTROL_CHROME_SURFACE_CLASS).toContain("starlane-control-chrome-surface");
    expect(CONTROL_CHROME_SURFACE_CLASS).toContain("shadow-card");
    expect(CONTROL_CHROME_RAIL_CLASS).toContain(CONTROL_CHROME_SURFACE_CLASS);
    expect(CONTROL_CHROME_ACTION_CLASS).toContain("starlane-control-chrome-action");
    expect(CONTROL_CHROME_ACTIVE_ACTION_CLASS).toContain("starlane-control-chrome-active");
    expect(CONTROL_CHROME_ACTIVE_ACTION_CLASS).toContain("text-primary");
    expect(CONTROL_CHROME_TOOLTIP_CLASS).toContain(CONTROL_CHROME_SURFACE_CLASS);
  });

  it("gives icon-only touch chrome a 44px target", () => {
    expect(CONTROL_CHROME_TOUCH_ACTION_CLASS).toContain("h-11");
    expect(CONTROL_CHROME_TOUCH_ACTION_CLASS).toContain("w-11");
    expect(CONTROL_CHROME_TOUCH_ACTION_CLASS).toContain(CONTROL_CHROME_ACTION_CLASS);
  });
});
