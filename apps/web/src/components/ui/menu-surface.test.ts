import { describe, expect, it } from "vitest";
import {
  CONTEXT_MENU_ITEM_HEIGHT_CLASS,
  CONTEXT_MENU_VIEWPORT_CONTAINMENT_CLASS,
} from "./context-menu";
import { POPOVER_VIEWPORT_CONTAINMENT_CLASS } from "./popover";

describe("shared menu surfaces", () => {
  it("contains popover and context-menu overflow within Radix's available viewport", () => {
    expect(POPOVER_VIEWPORT_CONTAINMENT_CLASS).toContain(
      "max-h-[var(--radix-popover-content-available-height)]",
    );
    expect(CONTEXT_MENU_VIEWPORT_CONTAINMENT_CLASS).toContain(
      "max-h-[var(--radix-context-menu-content-available-height)]",
    );
    expect(POPOVER_VIEWPORT_CONTAINMENT_CLASS).toContain("overflow-y-auto");
    expect(CONTEXT_MENU_VIEWPORT_CONTAINMENT_CLASS).toContain("overscroll-contain");
  });

  it("keeps context-menu rows touch-safe on phones without changing desktop density", () => {
    expect(CONTEXT_MENU_ITEM_HEIGHT_CLASS).toBe("min-h-11 sm:min-h-8");
  });
});
