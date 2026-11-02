import { useCallback, type KeyboardEvent as ReactKeyboardEvent, type RefObject } from "react";
import { getNextMenuItemIndex, type MenuNavigationKey } from "./useMenuKeyboardNavigation";

const LISTBOX_OPTION_SELECTOR = "[role='option']:not([aria-disabled='true'])";

export function useListboxKeyboardNavigation(containerRef: RefObject<HTMLElement | null>) {
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
      const listbox = containerRef.current;
      if (!listbox) return;
      const options = Array.from(listbox.querySelectorAll<HTMLElement>(LISTBOX_OPTION_SELECTOR));
      const nextIndex = getNextMenuItemIndex(
        options.indexOf(document.activeElement as HTMLElement),
        options.length,
        event.key as MenuNavigationKey,
      );
      if (nextIndex < 0) return;
      event.preventDefault();
      options[nextIndex]?.focus();
    },
    [containerRef],
  );
}
