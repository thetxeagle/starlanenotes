import { parseTemplateMarker } from "@/lib/templateMarkers";

/** Protect retained CSS even when malformed boundaries make local parsing uncertain. */
function cssSourceSpan(content: string): { start: number; end: number } | "ambiguous" | undefined {
  const candidates = [...content.matchAll(/<!--\s*\/?\s*starlane:custom/gi)];
  if (!candidates.length) return undefined;
  const markers = [...content.matchAll(/<!--\s*(\/?)\s*starlane:custom-css\s*-->/gi)];
  if (candidates.length !== 2 || markers.length !== 2 || markers[0][1] || markers[1][1] !== "/") return "ambiguous";
  return { start: markers[0].index!, end: markers[1].index! + markers[1][0].length };
}

/** Strip only structurally recognized guidance from the published body. */
export function stripTemplateGuidance(content: string): string {
  const protectedSpan = cssSourceSpan(content);
  if (protectedSpan === "ambiguous") return content;
  let offset = 0;
  let fence: { character: string; length: number } | undefined;
  return content
    .split(/(?<=\n)/)
    .filter((line) => {
      const start = offset;
      offset += line.length;
      if (protectedSpan && start < protectedSpan.end && offset > protectedSpan.start) return true;
      const trimmed = line.trim();
      const indentedCode = /^(?: {4}| {0,3}\t)/.test(line);
      const delimiter = trimmed.match(/^(`{3,}|~{3,})(.*)$/);
      if (delimiter && !indentedCode) {
        const character = delimiter[1][0];
        if (!fence) fence = { character, length: delimiter[1].length };
        else if (
          fence.character === character &&
          delimiter[1].length >= fence.length &&
          !delimiter[2].trim()
        )
          fence = undefined;
        return true;
      }
      return Boolean(fence) || indentedCode || parseTemplateMarker(trimmed)?.kind !== "guidance";
    })
    .join("");
}
