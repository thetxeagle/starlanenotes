/**
 * Shared visual contract for compact action chrome.
 *
 * Toolbars, contextual toolbars, tooltips, and button groups may change their
 * actions or layout, but not their surface, border, active, or hover language.
 */
export const CONTROL_CHROME_SURFACE_CLASS =
  "starlane-control-chrome-surface rounded-button border text-text shadow-card";

export const CONTROL_CHROME_RAIL_CLASS = `${CONTROL_CHROME_SURFACE_CLASS} flex items-center gap-1 p-1`;

export const CONTROL_CHROME_ACTION_CLASS =
  "starlane-control-chrome-action rounded-button text-text-muted transition-colors duration-100";

/** Icon-only control chrome used on touch-first toolbar surfaces. */
export const CONTROL_CHROME_TOUCH_ACTION_CLASS = `${CONTROL_CHROME_ACTION_CLASS} flex h-11 w-11 shrink-0 items-center justify-center`;

export const CONTROL_CHROME_ACTIVE_ACTION_CLASS =
  "starlane-control-chrome-active text-primary";

export const CONTROL_CHROME_TOOLTIP_CLASS =
  `${CONTROL_CHROME_SURFACE_CLASS} px-2 py-1 text-xs`;
