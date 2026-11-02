import {
  CUSTOM_CSS_END_MARKER,
  CUSTOM_CSS_START_MARKER,
} from "@/lib/documentContent";
import { inspectPageCssSource } from "@/lib/pageCssProvenance";
import {
  resolveTemplateExperience,
  type ResolvedTemplateExperience,
} from "@/lib/templateExperience";
import { templatePresentationAsRecord } from "@/lib/templates";

type TemplateExperienceInput = Parameters<typeof resolveTemplateExperience>[0];

export type PreparedTemplateCreation =
  | { kind: "ready"; resolved: ResolvedTemplateExperience }
  | {
      kind: "held";
      reason: "ambiguous_css" | "conflicting_css" | "generated_css_markers";
      message: string;
    };

const OPEN_MARKER_RE = /^<!--\s*starlane:custom-css\s*-->/i;
const CLOSE_MARKER_RE = /<!--\s*\/\s*starlane:custom-css\s*-->$/i;

/**
 * Resolves a template only after its retained page-CSS source has a single,
 * lossless inheritance decision. Call this before creating folders or pages.
 */
export function prepareTemplateCreation(
  input: TemplateExperienceInput,
): PreparedTemplateCreation {
  const source = input.content ?? input.template?.content ?? "";
  const inspected = inspectPageCssSource(source);
  if (inspected.kind === "ambiguous") {
    return {
      kind: "held",
      reason: "ambiguous_css",
      message: "This template has ambiguous retained page CSS. Repair or export it before creating a page.",
    };
  }

  const properties = input.template?.properties ?? input.properties;
  const metadataCss = templatePresentationAsRecord(properties).custom_css;
  let rawBlock: string | undefined;
  let protectedContent = source;
  let sentinel: string | undefined;

  if (inspected.kind === "single") {
    if (inspected.provenance.kind !== "single") {
      throw new TypeError("Invalid page CSS inspection");
    }
    const retainedBlock = inspected.provenance.rawBlock;
    rawBlock = retainedBlock;
    const bodyCss = readRawCssInner(retainedBlock);
    if (metadataCss !== undefined && metadataCss !== bodyCss) {
      return {
        kind: "held",
        reason: "conflicting_css",
        message: "This template's retained page CSS conflicts with its presentation metadata.",
      };
    }
    sentinel = uniqueSentinel(source);
    const blockStart = source.indexOf(retainedBlock);
    protectedContent =
      source.slice(0, blockStart) + sentinel + source.slice(blockStart + retainedBlock.length);
  } else if (metadataCss !== undefined) {
    // No normalization: an explicitly empty string remains different from absent metadata.
    rawBlock = `${CUSTOM_CSS_START_MARKER}${metadataCss}${CUSTOM_CSS_END_MARKER}`;
    if (inspectPageCssSource(rawBlock).kind !== "single") {
      return {
        kind: "held",
        reason: "ambiguous_css",
        message: "This template has ambiguous retained page CSS. Repair or export it before creating a page.",
      };
    }
  }

  const resolved = resolveTemplateExperience({ ...input, content: protectedContent });
  if (inspectPageCssSource(resolved.content).kind !== "absent") {
    return {
      kind: "held",
      reason: "generated_css_markers",
      message: "Template values produced page-CSS markers, so creation was held for review.",
    };
  }

  if (sentinel && rawBlock) {
    const first = resolved.content.indexOf(sentinel);
    if (first < 0 || resolved.content.indexOf(sentinel, first + sentinel.length) >= 0) {
      return {
        kind: "held",
        reason: "generated_css_markers",
        message: "The protected page-CSS span could not be restored exactly.",
      };
    }
    resolved.content =
      resolved.content.slice(0, first) + rawBlock + resolved.content.slice(first + sentinel.length);
  } else if (rawBlock) {
    const newline = resolved.content.match(/\r\n|\n|\r/)?.[0] ?? "\n";
    resolved.content += (resolved.content.length ? newline + newline : "") + rawBlock;
  }

  return { kind: "ready", resolved };
}

function readRawCssInner(rawBlock: string): string {
  const opening = rawBlock.match(OPEN_MARKER_RE)?.[0] ?? "";
  const closing = rawBlock.match(CLOSE_MARKER_RE)?.[0] ?? "";
  return rawBlock.slice(opening.length, rawBlock.length - closing.length);
}

function uniqueSentinel(source: string): string {
  let suffix = 0;
  let value = "\uE000starlane-retained-css\uE001";
  while (source.includes(value)) value = `\uE000starlane-retained-css-${++suffix}\uE001`;
  return value;
}
