export const TOGGLE_THEME_EVENT = "starlane:toggle-theme";

export function dispatchThemeToggle(): void {
  if (typeof document === "undefined") return;
  document.dispatchEvent(new Event(TOGGLE_THEME_EVENT));
}
