import { readCustomCss } from "./documentContent";

/** Retained source evidence, separate from renderable CSS and save authority. */
export type PageCssProvenance =
  | { schemaVersion: 1; kind: "absent" }
  | { schemaVersion: 1; kind: "single"; rawBlock: string }
  | { schemaVersion: 1; kind: "ambiguous"; originalSource: string };

/** Source identity shown by page details; this confers no write or access authority. */
export interface RetainedPageCssSource {
  spaceId: string;
  documentId: string;
  draftId?: string;
  provenance: PageCssProvenance;
}

export interface PageCssInspection {
  kind: PageCssProvenance["kind"];
  provenance: PageCssProvenance;
  renderCss: string;
  bodySource: string;
}

export type PageCssRebuild =
  | { kind: "ready"; content: string }
  | {
      kind: "held";
      reason: "ambiguous_original" | "markers_in_edited_body";
      originalSource?: string;
      editedBody: string;
    };

type MarkerScan =
  | { kind: "absent" }
  | { kind: "single"; start: number; end: number }
  | { kind: "ambiguous" };

function scanMarkers(source: string): MarkerScan {
  const markers = [...source.matchAll(/<!--\s*(\/?)\s*starlane:custom-css\s*-->/gi)];
  // Include incomplete and malformed marker prefixes: guessing their boundary
  // would silently discard source during ordinary body serialization.
  const prefixes = [...source.matchAll(/<!--\s*\/?\s*starlane:custom/gi)];
  if (markers.length === 0 && prefixes.length === 0) return { kind: "absent" };
  if (markers.length !== 2 || prefixes.length !== 2 || markers[0][1] || !markers[1][1]) {
    return { kind: "ambiguous" };
  }
  return {
    kind: "single",
    start: markers[0].index!,
    end: markers[1].index! + markers[1][0].length,
  };
}

/** No trimming, NUL removal, sanitizing or truncation touches retained source. */
export function inspectPageCssSource(originalSource: string): PageCssInspection {
  const scan = scanMarkers(originalSource);
  const renderCss = readCustomCss(originalSource);
  if (scan.kind === "absent") {
    return {
      kind: "absent",
      provenance: { schemaVersion: 1, kind: "absent" },
      renderCss,
      bodySource: originalSource,
    };
  }
  if (scan.kind === "ambiguous") {
    return {
      kind: "ambiguous",
      provenance: { schemaVersion: 1, kind: "ambiguous", originalSource },
      renderCss,
      bodySource: originalSource,
    };
  }
  return {
    kind: "single",
    provenance: {
      schemaVersion: 1,
      kind: "single",
      rawBlock: originalSource.slice(scan.start, scan.end),
    },
    renderCss,
    bodySource: originalSource.slice(0, scan.start) + originalSource.slice(scan.end),
  };
}

function hasOnlyKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const actual = Object.keys(value);
  return (
    actual.length === keys.length &&
    keys.every((key) => Object.prototype.hasOwnProperty.call(value, key))
  );
}

/** Validate persisted provenance before it can supply opaque bytes to a save. */
export function validatePageCssProvenance(value: unknown): value is PageCssProvenance {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  if (record.schemaVersion !== 1) return false;
  if (record.kind === "absent") return hasOnlyKeys(record, ["schemaVersion", "kind"]);
  if (record.kind === "single") {
    if (
      !hasOnlyKeys(record, ["schemaVersion", "kind", "rawBlock"]) ||
      typeof record.rawBlock !== "string"
    )
      return false;
    const scan = scanMarkers(record.rawBlock);
    return scan.kind === "single" && scan.start === 0 && scan.end === record.rawBlock.length;
  }
  return (
    record.kind === "ambiguous" &&
    hasOnlyKeys(record, ["schemaVersion", "kind", "originalSource"]) &&
    typeof record.originalSource === "string" &&
    scanMarkers(record.originalSource).kind === "ambiguous"
  );
}

/** Character count for the existing UTF-16 recovery-storage budget. */
export function retainedPageCssTextLength(provenance: PageCssProvenance): number {
  if (provenance.kind === "single") return provenance.rawBlock.length;
  if (provenance.kind === "ambiguous") return provenance.originalSource.length;
  return 0;
}

/** Ordinary body saves preserve the opaque block; CSS authoring is separate intent. */
export function rebuildPageCssSource(
  editedBody: string,
  provenance: PageCssProvenance,
): PageCssRebuild {
  if (!validatePageCssProvenance(provenance)) throw new TypeError("Invalid page CSS provenance");
  if (provenance.kind === "ambiguous") {
    return {
      kind: "held",
      reason: "ambiguous_original",
      originalSource: provenance.originalSource,
      editedBody,
    };
  }
  if (scanMarkers(editedBody).kind !== "absent") {
    return { kind: "held", reason: "markers_in_edited_body", editedBody };
  }
  if (provenance.kind === "absent") return { kind: "ready", content: editedBody };
  const newline = provenance.rawBlock.match(/\r\n|\n|\r/)?.[0] ?? "\n";
  return {
    kind: "ready",
    content: editedBody + (editedBody.length ? newline + newline : "") + provenance.rawBlock,
  };
}
