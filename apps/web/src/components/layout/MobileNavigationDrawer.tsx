import { useRef, type ReactNode } from "react";
import { X } from "lucide-react";
import { useOverlayFocusLifecycle } from "@/components/ui/useOverlayFocusLifecycle";

interface MobileNavigationDrawerProps {
  open: boolean;
  suspended?: boolean;
  onClose: () => void;
  children: ReactNode;
}

export const MOBILE_NAVIGATION_DRAWER_PANEL_CLASS =
  "fixed bottom-0 left-0 top-0 z-50 w-[min(19rem,86vw)] border-r border-border bg-surface shadow-card";

/**
 * Shared touch-safe owner for the workspace sidebar on mobile web and native shells.
 * It remains a navigation drawer rather than a centered action dialog.
 */
export function MobileNavigationDrawer({
  open,
  suspended = false,
  onClose,
  children,
}: MobileNavigationDrawerProps) {
  const drawerRef = useRef<HTMLElement | null>(null);
  const closeAndRestoreFocus = useOverlayFocusLifecycle({
    open: open && !suspended,
    onClose,
    containerRef: drawerRef,
  });

  if (!open) return null;

  return (
    <>
      {!suspended && (
        <button
          type="button"
          className="fixed inset-0 z-40 bg-black/40"
          aria-label="Close navigation"
          onClick={closeAndRestoreFocus}
        />
      )}
      <aside
        ref={drawerRef}
        className={`${MOBILE_NAVIGATION_DRAWER_PANEL_CLASS} ${suspended ? "invisible" : ""}`}
        role="dialog"
        aria-hidden={suspended || undefined}
        aria-modal={suspended ? undefined : "true"}
        aria-labelledby="mobile-navigation-title"
        tabIndex={-1}
      >
        <div className="flex h-full min-h-0 flex-col pt-[max(0.5rem,env(safe-area-inset-top))]">
          <div className="flex h-11 shrink-0 items-center justify-between border-b border-border px-3">
            <h2 id="mobile-navigation-title" className="text-sm font-medium text-text">
              Navigation
            </h2>
            <button
              type="button"
              onClick={closeAndRestoreFocus}
              className="flex h-11 w-11 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:bg-surface-raised hover:text-text"
              aria-label="Close navigation"
            >
              <X size={18} />
            </button>
          </div>
          <div className="min-h-0 flex-1">{children}</div>
        </div>
      </aside>
    </>
  );
}
