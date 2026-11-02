export type WorkspaceShortcutAction =
  | "search"
  | "command"
  | "new-document"
  | "new-space"
  | "navigation"
  | "toggle-theme";

/**
 * Dialogs and menus claim keys through their shared focus lifecycle. Global
 * workspace shortcuts must leave those already-owned surfaces alone instead
 * of changing navigation underneath an active overlay.
 */
export function shouldHandleWorkspaceShortcutEvent(
  event: Pick<KeyboardEvent, "defaultPrevented">,
  overlayOpen = false,
): boolean {
  return !event.defaultPrevented && !overlayOpen;
}

/** Shared modal/menu boundary for desktop, responsive web, and native shells. */
export function isWorkspaceOverlayOpen(
  root: Pick<Document, "querySelector">,
): boolean {
  return Boolean(root.querySelector("[role='dialog'], [role='menu']"));
}

/** Keep document writing and form entry from invoking shell-level shortcuts. */
export function isWorkspaceTextEntryTarget(
  target: EventTarget | Pick<HTMLElement, "tagName" | "isContentEditable"> | null,
): boolean {
  if (typeof HTMLElement !== "undefined" && target instanceof HTMLElement) {
    return target.isContentEditable || Boolean(target.closest("input, textarea, select, [contenteditable='true']"));
  }
  if (!target || typeof target !== "object" || !("tagName" in target)) return false;
  const entryTarget = target as Pick<HTMLElement, "tagName" | "isContentEditable">;
  return entryTarget.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(entryTarget.tagName);
}

export function getWorkspaceShortcutAction(
  event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "shiftKey">,
): WorkspaceShortcutAction | null {
  if (!event.metaKey && !event.ctrlKey) return null;
  const key = event.key.toLowerCase();
  if (event.shiftKey) {
    if (key === "d") return "toggle-theme";
    return key === "n" ? "new-space" : null;
  }
  if (key === "k") return "search";
  if (key === "p") return "command";
  if (key === "n") return "new-document";
  return event.key === "\\" ? "navigation" : null;
}
