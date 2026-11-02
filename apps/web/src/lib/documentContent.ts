export const SECTION_START_MARKER = "<!-- starlane:section -->";
export const SECTION_END_MARKER = "<!-- /starlane:section -->";
export const SECTION_COLUMN_MARKER = "<!-- starlane:column -->";
export const SECTION_COLUMN_MARKER_PREFIX = "<!-- starlane:column";
export const COLUMNS_START_MARKER = "<!-- starlane:columns layout=two-equal -->";
export const COLUMNS_END_MARKER = "<!-- /starlane:columns -->";
export const CUSTOM_CSS_START_MARKER = "<!-- starlane:custom-css -->";
export const CUSTOM_CSS_END_MARKER = "<!-- /starlane:custom-css -->";

const CUSTOM_CSS_BLOCK_RE = /<!--\s*starlane:custom-css\s*-->[\s\S]*?<!--\s*\/starlane:custom-css\s*-->/i;

/** Keep page CSS portable and prevent it from becoming an asset or script loader. */
export function sanitizeCustomCss(value: unknown): string {
  if (typeof value !== "string") return "";
  const css = value.replace(/\0/g, "").trim();
  if (/(@import|url\s*\(|expression\s*\(|javascript\s*:|behavior\s*:|-moz-binding)/i.test(css)) {
    return "";
  }
  return css.slice(0, 20_000);
}

export function readCustomCss(content: string): string {
  const match = content.match(CUSTOM_CSS_BLOCK_RE);
  if (!match) return "";
  return sanitizeCustomCss(
    match[0]
      .replace(/^<!--[\t ]*starlane:custom-css[\t ]*-->\s*/i, "")
      .replace(/\s*<!--[\t ]*\/starlane:custom-css[\t ]*-->$/i, ""),
  );
}

export function replaceCustomCss(content: string, value: string): string {
  const withoutBlock = content.replace(CUSTOM_CSS_BLOCK_RE, "").replace(/\n{3,}/g, "\n\n").trimEnd();
  const css = sanitizeCustomCss(value);
  if (!css) return withoutBlock;
  return `${withoutBlock}\n\n${CUSTOM_CSS_START_MARKER}\n${css}\n${CUSTOM_CSS_END_MARKER}`;
}

export function stripCustomCss(content: string): string {
  return content.replace(CUSTOM_CSS_BLOCK_RE, "").replace(/\n{3,}/g, "\n\n").trim();
}

export const DEFAULT_NEW_DOCUMENT_CONTENT = "";
