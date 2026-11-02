import { useRef, type ReactNode } from "react";
import { useMenuKeyboardNavigation } from "./useMenuKeyboardNavigation";
import { useOverlayFocusLifecycle } from "./useOverlayFocusLifecycle";

interface MobileActionSheetProps {
  open: boolean;
  title: string;
  labelledBy: string;
  onClose: () => void;
  children: ReactNode;
}

export const MOBILE_ACTION_SHEET_CLASS =
  "starlane-menu-surface fixed inset-x-3 bottom-[calc(env(safe-area-inset-bottom)+4.2rem)] z-50 max-h-[min(30rem,calc(100dvh-6rem))] overflow-y-auto";

/** Shared touch-safe action menu for long-press and contextual mobile actions. */
export function MobileActionSheet({
  open,
  title,
  labelledBy,
  onClose,
  children,
}: MobileActionSheetProps) {
  const actionSheetRef = useRef<HTMLElement | null>(null);
  const closeAndRestoreFocus = useOverlayFocusLifecycle({
    open,
    onClose,
    containerRef: actionSheetRef,
  });
  const handleMenuKeyDown = useMenuKeyboardNavigation(actionSheetRef);

  if (!open) return null;

  return (
    <>
      <button
        type="button"
        className="fixed inset-0 z-40 bg-black/40"
        aria-label={`Close ${title} actions`}
        onClick={closeAndRestoreFocus}
      />
      <section
        ref={actionSheetRef}
        className={MOBILE_ACTION_SHEET_CLASS}
        role="menu"
        aria-labelledby={labelledBy}
        tabIndex={-1}
        onKeyDown={handleMenuKeyDown}
      >
        <p id={labelledBy} className="px-2 py-1 text-xs text-text-muted">
          {title}
        </p>
        {children}
        <button
          type="button"
          role="menuitem"
          onClick={closeAndRestoreFocus}
          className="mt-1 w-full rounded-button border border-border px-2 py-2 text-left text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
        >
          Cancel
        </button>
      </section>
    </>
  );
}
