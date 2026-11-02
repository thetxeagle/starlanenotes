import { useCallback, type KeyboardEvent as ReactKeyboardEvent, type RefObject } from "react";

export type MenuNavigationKey = "ArrowDown" | "ArrowUp" | "Home" | "End";

const MENU_ITEM_SELECTOR = [
  "button[role='menuitem']:not(:disabled)",
  "button[role='menuitemcheckbox']:not(:disabled)",
  "button[role='menuitemradio']:not(:disabled)",
].join(", ");

export function getNextMenuItemIndex(
  currentIndex: number,
  itemCount: number,
  key: MenuNavigationKey,
): number {
  if (itemCount <= 0) return -1;
  if (key === "Home") return 0;
  if (key === "End") return itemCount - 1;
  if (key === "ArrowUp") return currentIndex <= 0 ? itemCount - 1 : currentIndex - 1;
  return currentIndex >= itemCount - 1 ? 0 : currentIndex + 1;
}

/** Keeps a nested submenu from contributing its actions to its parent menu. */
export function isMenuItemInNavigationScope(menu: Element, item: Element): boolean {
  return item.closest("[role='menu']") === menu;
}

/** Adds standard directional navigation without taking over Tab or Escape. */
export function useMenuKeyboardNavigation(containerRef: RefObject<HTMLElement | null>) {
  return useCallback(
    (event: ReactKeyboardEvent<HTMLElement>) => {
      if (
        event.key !== "ArrowDown" &&
        event.key !== "ArrowUp" &&
        event.key !== "Home" &&
        event.key !== "End"
      ) {
        return;
      }
      const focusedMenu =
        event.target instanceof Element
          ? event.target.closest<HTMLElement>("[role='menu']")
          : null;
      const menu = focusedMenu ?? containerRef.current;
      if (!menu) return;
      const menuItems = Array.from(
        menu.querySelectorAll<HTMLButtonElement>(MENU_ITEM_SELECTOR),
      ).filter((item) => isMenuItemInNavigationScope(menu, item));
      const nextIndex = getNextMenuItemIndex(
        menuItems.indexOf(document.activeElement as HTMLButtonElement),
        menuItems.length,
        event.key,
      );
      if (nextIndex < 0) return;
      event.preventDefault();
      menuItems[nextIndex]?.focus();
    },
    [containerRef],
  );
}
