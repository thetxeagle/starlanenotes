/**
 * A field value remains readable as ordinary inline text in Markdown readers
 * that render HTML, while the Starlane editor can recover its text boundaries
 * without parsing field answers as Markdown structure.
 */

export function escapeTemplateValueHtmlText(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** Decode only the entities emitted by escapeTemplateValueHtmlText. */
export function decodeTemplateValueHtmlText(value: string): string {
  return value
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, "&");
}

export function serializeTemplateValue(value: string): string {
  return `<span data-starlane-template-value="v1" style="white-space:pre-wrap">${escapeTemplateValueHtmlText(value)}</span>`;
}

export function encodeTemplateValue(value: string): string {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

export function decodeTemplateValue(value: string): string {
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = base64 + "=".repeat((4 - (base64.length % 4)) % 4);
  const binary = atob(padded);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}
