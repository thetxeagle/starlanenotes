import { useCallback, useEffect, useMemo, useRef, type RefObject } from "react";

const FOCUSABLE_OVERLAY_SELECTOR = [
  "button:not([disabled])",
  "[href]",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(", ");

type FocusableTrigger = HTMLElement | SVGElement;

// React mounts child effects before parents: DOM nesting, not registration order,
// decides ownership for a nested modal. Registration order settles sibling overlays.
const overlayOwners: Array<RefObject<HTMLElement | null>> = [];
export function ownsOverlayKeyboard(
  owner: RefObject<HTMLElement | null>,
  owners = overlayOwners,
): boolean {
  const container = owner.current;
  if (!container?.isConnected) return false;
  const index = owners.indexOf(owner);
  return !owners.some((other, otherIndex) => {
    const surface = other.current;
    if (other === owner || !surface?.isConnected) return false;
    if (container.contains(surface)) return true;
    if (surface.contains(container)) return false;
    return otherIndex > index;
  });
}

function getOverlayFocusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_OVERLAY_SELECTOR)).filter(
    (element) => element.getAttribute("aria-hidden") !== "true",
  );
}

interface OverlayFocusLifecycleOptions {
  open: boolean;
  onClose: () => void;
  containerRef: RefObject<HTMLElement | null>;
  triggerRef?: RefObject<FocusableTrigger | null>;
  closeOnEscape?: boolean;
  /** Some contextual popovers must not steal the editor caret when they open. */
  focusOnOpen?: boolean;
}

/**
 * Gives the shared mobile sheet/dialog owners the same keyboard lifecycle:
 * capture the opener, move focus into the surface, contain Tab, then restore
 * the opener after dismissal.
 */
export function useOverlayFocusLifecycle({
  open,
  onClose,
  containerRef,
  triggerRef,
  closeOnEscape = true,
  focusOnOpen = true,
}: OverlayFocusLifecycleOptions): () => void {
  // Capture before the child commit's autoFocus can replace the opener.
  const triggerAtOpen = useMemo(() => {
    if (!open || typeof document === "undefined") return null;
    const active = document.activeElement;
    return active instanceof HTMLElement || active instanceof SVGElement ? active : null;
  }, [open]);
  const capturedTriggerRef = useRef<FocusableTrigger | null>(null);
  const onCloseRef = useRef(onClose);
  // Changing nested Escape ownership must not re-register or refocus the modal.
  const closeOnEscapeRef = useRef(closeOnEscape);
  closeOnEscapeRef.current = closeOnEscape;

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  const restoreCapturedTriggerFocus = useCallback(() => {
    window.requestAnimationFrame(() => {
      const trigger = capturedTriggerRef.current;
      if (trigger?.isConnected && !overlayOwners.some((owner) =>
        ownsOverlayKeyboard(owner) && !owner.current?.contains(trigger),
      )) trigger.focus();
    });
  }, []);

  const closeAndRestoreFocus = useCallback(() => {
    if (ownsOverlayKeyboard(containerRef)) onCloseRef.current();
  }, [containerRef]);

  useEffect(() => {
    if (!open) return;
    capturedTriggerRef.current =
      triggerRef?.current ??
      triggerAtOpen;

    return restoreCapturedTriggerFocus;
  }, [open, restoreCapturedTriggerFocus, triggerRef, triggerAtOpen]);

  useEffect(() => {
    if (!open) return;

    overlayOwners.push(containerRef);
    const focusSurface = () => {
      if (!ownsOverlayKeyboard(containerRef)) return;
      const container = containerRef.current;
      if (!container) return;
      const [firstFocusable] = getOverlayFocusableElements(container);
      (firstFocusable ?? container).focus();
    };

    const focusFrame = focusOnOpen ? window.requestAnimationFrame(focusSurface) : null;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (!ownsOverlayKeyboard(containerRef)) return;
      if (event.key === "Escape" && closeOnEscapeRef.current) {
        event.preventDefault();
        event.stopPropagation();
        closeAndRestoreFocus();
        return;
      }
      if (event.key !== "Tab") return;

      const container = containerRef.current;
      if (!container) return;
      const focusable = getOverlayFocusableElements(container);
      if (focusable.length === 0) {
        event.preventDefault();
        container.focus();
        return;
      }

      const firstFocusable = focusable[0];
      const lastFocusable = focusable[focusable.length - 1];
      const activeElement = document.activeElement;
      if (
        event.shiftKey &&
        (activeElement === firstFocusable || !container.contains(activeElement))
      ) {
        event.preventDefault();
        lastFocusable.focus();
      } else if (
        !event.shiftKey &&
        (activeElement === lastFocusable || !container.contains(activeElement))
      ) {
        event.preventDefault();
        firstFocusable.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      const ownerIndex = overlayOwners.indexOf(containerRef);
      if (ownerIndex >= 0) overlayOwners.splice(ownerIndex, 1);
      if (focusFrame !== null) window.cancelAnimationFrame(focusFrame);
      document.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [closeAndRestoreFocus, containerRef, focusOnOpen, open, triggerRef]);

  return closeAndRestoreFocus;
}
