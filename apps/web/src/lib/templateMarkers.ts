/**
 * Portable, inert Markdown comments used by template-only editor nodes.
 * The reserved prefix is deliberately versioned so older clients can preserve
 * directives they do not understand without treating them as authored prose.
 */

const PREFIX = "starlane-template:";
const MARKER_LINE = /^<!--[ \t]*starlane-template:([^:]+):([^:]+):([A-Za-z0-9_-]+)[ \t]*-->$/;
const RESERVED_MARKER_COMMENT = /^<!--[ \t]*starlane-template:/;

export type TemplateMarkerPayload =
  | { kind: "guidance"; id: string; text: string }
  | { kind: "field"; id: string; field_id: string };

export type ParsedTemplateMarker =
  | ({ version: number } & TemplateMarkerPayload)
  | { kind: "opaque"; raw: string };

function encodePayload(value: Record<string, string>): string {
  const bytes = new TextEncoder().encode(JSON.stringify(value));
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function decodePayload(value: string): unknown {
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = base64 + "=".repeat((4 - (base64.length % 4)) % 4);
  const binary = atob(padded);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return JSON.parse(new TextDecoder().decode(bytes)) as unknown;
}

function validId(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 160;
}

/** Serialize a supported v1 guidance or named-field insertion point. */
export function serializeTemplateMarker(marker: TemplateMarkerPayload): string {
  const payload: Record<string, string> =
    marker.kind === "guidance"
      ? { id: marker.id, text: marker.text }
      : { id: marker.id, field_id: marker.field_id };
  return `<!-- ${PREFIX}v1:${marker.kind}:${encodePayload(payload)} -->`;
}

/**
 * Parse a reserved marker comment. Unsupported or malformed reserved comments
 * are returned as opaque markers so editor round-trips preserve them verbatim.
 */
export function parseTemplateMarker(line: string): ParsedTemplateMarker | null {
  const normalized = line.trim();
  if (!RESERVED_MARKER_COMMENT.test(normalized)) return null;

  const match = normalized.match(MARKER_LINE);
  if (!match || match[1] !== "v1" || (match[2] !== "guidance" && match[2] !== "field")) {
    return { kind: "opaque", raw: normalized };
  }

  try {
    const payload = decodePayload(match[3]);
    if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
      return { kind: "opaque", raw: normalized };
    }
    const value = payload as Record<string, unknown>;
    if (!validId(value.id)) return { kind: "opaque", raw: normalized };
    if (match[2] === "guidance" && typeof value.text === "string") {
      return { kind: "guidance", version: 1, id: value.id, text: value.text };
    }
    if (match[2] === "field" && validId(value.field_id)) {
      return { kind: "field", version: 1, id: value.id, field_id: value.field_id };
    }
  } catch {
    // The source remains a reserved opaque comment; never reinterpret it.
  }
  return { kind: "opaque", raw: normalized };
}

/** HTML attribute escaping for the editor's intermediate DOM representation. */
export function escapeTemplateMarkerAttribute(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
