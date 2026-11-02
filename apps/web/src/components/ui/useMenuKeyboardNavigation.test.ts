import { describe, expect, it } from "vitest";
import { getNextMenuItemIndex, isMenuItemInNavigationScope } from "./useMenuKeyboardNavigation";

describe("getNextMenuItemIndex", () => {
  it("moves through menu items with wrapping Arrow keys and bounded Home/End", () => {
    expect(getNextMenuItemIndex(0, 3, "ArrowDown")).toBe(1);
    expect(getNextMenuItemIndex(2, 3, "ArrowDown")).toBe(0);
    expect(getNextMenuItemIndex(0, 3, "ArrowUp")).toBe(2);
    expect(getNextMenuItemIndex(2, 3, "Home")).toBe(0);
    expect(getNextMenuItemIndex(0, 3, "End")).toBe(2);
    expect(getNextMenuItemIndex(0, 0, "ArrowDown")).toBe(-1);
  });
});

describe("isMenuItemInNavigationScope", () => {
  it("keeps nested submenu items out of their parent directional list", () => {
    const parentMenu = { id: "parent" } as unknown as Element;
    const childMenu = { id: "child" } as unknown as Element;
    const parentItem = {
      closest: () => parentMenu,
    } as unknown as Element;
    const childItem = {
      closest: () => childMenu,
    } as unknown as Element;

    expect(isMenuItemInNavigationScope(parentMenu, parentItem)).toBe(true);
    expect(isMenuItemInNavigationScope(parentMenu, childItem)).toBe(false);
    expect(isMenuItemInNavigationScope(childMenu, childItem)).toBe(true);
  });
});
