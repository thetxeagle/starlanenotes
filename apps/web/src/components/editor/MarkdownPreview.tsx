import {releaseBrowserAttachmentImage} from "@/lib/browserAttachmentImage";
import { saveBrowserPinnedAttachment } from "@/lib/browserAttachmentSave";
import { isTauri as isNativeAttachmentShell, captureOfflineServerContext as captureAttachmentContext, getApiBase as attachmentApiBase, api as attachmentApi } from "@/lib/api";
import {verifyOfflineReplayContext as verifyAttachmentContext} from "@/lib/offlineReplayOrigin";
import {
  WIKI_DOCUMENT_HREF_PREFIX,
  markdownToHtml,
  parseWikiDocumentHref,
  parseCodeFenceInfo,
  editorLowlight,
  highlightedCodeToHtml,
  type TemplateMarkerRenderMode,
} from "@/components/editor/Editor";
import type { SlashTemplateField } from "@/components/editor/SlashCommands";
import { registerEditorSyntax } from "@/components/editor/editorSyntax";
import {
  COLUMNS_END_MARKER,
  parseColumnsStartMarker,
  parseSectionStartMarker,
  parseSectionStartOptions,
  SECTION_END_MARKER,
} from "@/components/editor/EditorSection";
import {
  CalloutHeader,
  parseCalloutDirective,
  type CalloutKind,
} from "@/components/editor/Callout";
import { MathBlockDisplay, parseMathBlockDirective } from "@/components/editor/MathBlock";
import {
  isTableOfContentsDirective,
  TableOfContentsList,
  type TableOfContentsEntry,
} from "@/components/editor/TableOfContents";
import {
  ChildPagesBlock,
  getDirectChildPages,
  isChildPagesDirective,
  parseChildPagesDirective,
  renderChildPagesHtml,
  type ChildPagesVariant,
} from "@/components/editor/ChildPages";
import {
  dataClient,
  type AttachmentRecord,
  type LocalAssetContext,
  type SearchResultRecord,
} from "@/lib/dataClient";
import { api } from "@/lib/api";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { parseDocumentHeadings } from "@/lib/documentOutline";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import {
  parsePortableCodeTabs,
  parsePortableGalleryBlock,
  parsePortableStatusBlock,
  parsePortableVideoBlock,
  PortableCodeTabs,
  PortableCodeBlock,
  PortableGallery,
  PortableStatusBlock,
  PortableVideoPlayer,
} from "@/components/editor/PortableBlocks";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { useOverlayFocusLifecycle } from "@/components/ui/useOverlayFocusLifecycle";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";
import { Button } from "@/components/ui/button";
import { EditorCommentDialog } from "@/components/editor/EditorCommentDialog";
import { triggerMediaDownload } from "@/components/editor/EditorMediaActionMenu";
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  ExternalLink,
  MessageSquare,
  RefreshCw,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import {
  emitDocumentCommentsChanged,
  listDocumentComments,
  subscribeToDocumentComments,
  type DocumentComment,
} from "@/lib/documentComments";

// Preview rendering can be loaded without the editor route. Register the same
// supported grammars here so standalone/page-view previews retain highlighting.
registerEditorSyntax(editorLowlight);

interface MarkdownPreviewProps {
  content: string;
  assetContext?: LocalAssetContext;
  inlineCommentsVisible?: boolean;
  spaceId?: string;
  docId?: string;
  onInlineCommentOpen?: (commentId: string) => void;
  onCommentCreated?: () => void;
  onDocumentNavigate?: (documentId: string) => void;
  selectionCommentsEnabled?: boolean;
  templateMarkerMode?: Extract<TemplateMarkerRenderMode, "preview" | "omit">;
  templateFields?: SlashTemplateField[];
}

interface PreviewLinkMenu {
  href: string;
  position: { top: number; left: number };
}

interface PreviewCommentSelection {
  selectionText: string;
  position: { top: number; left: number };
}

export function replaceMarkdownAttachmentReferences(
  markdown: string,
  attachmentUrls: ReadonlyMap<string, string>,
): string {
  return markdown.replace(
    /(!\[[^\]\n]*\]\()starlane-attachment:\/\/([^\s)]+)/gi,
    (match, prefix, attachmentId) => {
      const url = attachmentUrls.get(String(attachmentId));
      return url ? `${prefix}${url}` : match;
    },
  );
}

export function getPreviewSelectionCommentPosition(
  rect: Pick<DOMRect, "top" | "bottom" | "left" | "right" | "width">,
  viewportWidth: number,
  viewportHeight: number,
): { top: number; left: number } {
  const size = 36;
  const topAbove = rect.top - size - 8;
  const top = topAbove >= 8 ? topAbove : Math.min(rect.bottom + 8, viewportHeight - size - 8);
  const left = rect.left + rect.width / 2 - size / 2;
  return {
    top: Math.max(8, Math.min(top, viewportHeight - size - 8)),
    left: Math.max(8, Math.min(left, viewportWidth - size - 8)),
  };
}

export function wrapPreviewTables(html: string): string {
  return html.replace(
    /<table\b([^>]*)>([\s\S]*?)<\/table>/gi,
    (_match, attributes: string, contents: string) =>
      `<div data-block-placement="${attributes.match(/data-table-placement="(left|center|right)"/)?.[1] ?? "center"}" class="tableWrapper markdown-table-scroll" role="region" aria-label="Scrollable document table" tabindex="0"><table${attributes}>${contents}</table></div>`,
  );
}

export function renderMarkdownPreviewHtml(
  markdown: string,
  templateMarkerMode: Extract<TemplateMarkerRenderMode, "preview" | "omit"> = "omit",
  templateFields: readonly SlashTemplateField[] = [],
): string {
  return wrapPreviewTables(markdownToHtml(markdown, false, templateFields, templateMarkerMode)).replace(
    /<input\b(?=[^>]*\btype="checkbox")([^>]*)>/gi,
    (input, attributes: string) =>
      /\sdisabled(?:\s|=|$)/i.test(attributes) ? input : `<input${attributes} disabled>`,
  );
}

type MarkdownPreviewPart =
  | { type: "markdown"; content: string; key: string }
  | { type: "dataview"; query: string; key: string }
  | { type: "callout"; kind: CalloutKind; content: string; key: string }
  | { type: "math"; expression: string; key: string }
  | { type: "toc"; key: string }
  | { type: "child-pages"; variant: ChildPagesVariant; key: string }
  | { type: "code-tabs"; source: string; key: string }
  | { type: "status"; source: string; key: string }
  | { type: "video"; source: string; key: string }
  | { type: "gallery"; source: string; key: string }
  | { type: "carousel"; source: string; key: string }
  | {
      type: "code";
      language: string;
      title?: string;
      placement?: import("./blockPlacement").BlockPlacement;
      code: string;
      key: string;
    };

export function splitMarkdownDataviewBlocks(content: string): MarkdownPreviewPart[] {
  const parts: MarkdownPreviewPart[] = [];
  const lines = unwrapPlainSectionsContainingDataview(content).split("\n");
  const markdownBuffer: string[] = [];
  let partIndex = 0;
  let inGenericFence = false;
  let inMathFence = false;
  const containerEnds: string[] = [];

  const flushMarkdown = () => {
    if (markdownBuffer.length === 0) return;
    parts.push({
      type: "markdown",
      content: markdownBuffer.join("\n"),
      key: `markdown-${partIndex}`,
    });
    partIndex += 1;
    markdownBuffer.length = 0;
  };

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const trimmed = line.trim();
    const startsSection = !inGenericFence && !inMathFence && parseSectionStartMarker(trimmed);
    const startsColumns = !inGenericFence && !inMathFence && parseColumnsStartMarker(trimmed);
    if (startsSection || startsColumns) {
      containerEnds.push(startsColumns ? COLUMNS_END_MARKER : SECTION_END_MARKER);
      markdownBuffer.push(line);
      continue;
    }
    if (
      !inGenericFence &&
      !inMathFence &&
      containerEnds.length > 0 &&
      trimmed === containerEnds[containerEnds.length - 1]
    ) {
      containerEnds.pop();
      markdownBuffer.push(line);
      continue;
    }
    if (containerEnds.length > 0) {
      if (/^```/.test(trimmed)) inGenericFence = !inGenericFence;
      if (!inGenericFence && /^\$\$\s*$/.test(trimmed)) inMathFence = !inMathFence;
      markdownBuffer.push(line);
      continue;
    }
    if (!inGenericFence && !inMathFence && /^\$\$\s*$/.test(line.trim())) {
      let closingIndex = -1;
      for (let next = index + 1; next < lines.length; next += 1) {
        if (/^\$\$\s*$/.test(lines[next].trim())) {
          closingIndex = next;
          break;
        }
      }
      if (closingIndex >= 0) {
        const expression = parseMathBlockDirective(lines.slice(index, closingIndex + 1).join("\n"));
        if (expression != null) {
          flushMarkdown();
          parts.push({ type: "math", expression, key: `math-${partIndex}` });
          partIndex += 1;
          index = closingIndex;
          continue;
        }
      }
    }
    if (
      !inGenericFence &&
      !inMathFence &&
      /^>\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$/i.test(line.trim())
    ) {
      const calloutLines = [line];
      let closingIndex = index;
      for (let next = index + 1; next < lines.length && /^>\s?/.test(lines[next]); next += 1) {
        calloutLines.push(lines[next]);
        closingIndex = next;
      }
      const callout = parseCalloutDirective(calloutLines.join("\n"));
      if (callout) {
        flushMarkdown();
        parts.push({
          type: "callout",
          kind: callout.kind,
          content: callout.content,
          key: `callout-${partIndex}`,
        });
        partIndex += 1;
        index = closingIndex;
        continue;
      }
    }
    if (!inGenericFence && !inMathFence && isTableOfContentsDirective(line)) {
      flushMarkdown();
      parts.push({ type: "toc", key: `toc-${partIndex}` });
      partIndex += 1;
      continue;
    }
    if (!inGenericFence && !inMathFence && isChildPagesDirective(line)) {
      flushMarkdown();
      parts.push({
        type: "child-pages",
        variant: parseChildPagesDirective(line) || "list",
        key: `child-pages-${partIndex}`,
      });
      partIndex += 1;
      continue;
    }
    if (
      !inGenericFence &&
      !inMathFence &&
      /^```(?!(?:dataview|dv|code-tabs|status|video|gallery|carousel)\s*$)([\w-]*(?:\s*title=(?:"[^"]*"|'[^']*'))?(?:\s*placement=(?:left|center|right|expand))?)\s*$/i.test(
        line.trim(),
      )
    ) {
      const { language, title, placement } = parseCodeFenceInfo(line.trim().slice(3));
      const codeLines: string[] = [];
      let closingIndex = -1;
      for (let next = index + 1; next < lines.length; next += 1) {
        if (lines[next].trim() === "```") {
          closingIndex = next;
          break;
        }
        codeLines.push(lines[next]);
      }
      if (closingIndex === -1) {
        markdownBuffer.push(...lines.slice(index));
        break;
      }
      flushMarkdown();
      parts.push({
        type: "code",
        language,
        title,
        placement,
        code: codeLines.join("\n"),
        key: `code-${partIndex}`,
      });
      partIndex += 1;
      index = closingIndex;
      continue;
    }
    if (!inGenericFence && !inMathFence && /^```(?:dataview|dv)\s*$/i.test(line.trim())) {
      const queryLines: string[] = [];
      let closingIndex = -1;
      for (let next = index + 1; next < lines.length; next += 1) {
        if (lines[next].trim() === "```") {
          closingIndex = next;
          break;
        }
        queryLines.push(lines[next]);
      }

      if (closingIndex === -1) {
        markdownBuffer.push(...lines.slice(index));
        break;
      }

      flushMarkdown();
      parts.push({
        type: "dataview",
        query: queryLines.join("\n").trim(),
        key: `dataview-${partIndex}`,
      });
      partIndex += 1;
      index = closingIndex;
      continue;
    }
    if (
      !inGenericFence &&
      !inMathFence &&
      /^```(?:code-tabs|status|video|gallery|carousel)\s*$/i.test(line.trim())
    ) {
      const kind = line.trim().slice(3).toLowerCase() as
        | "code-tabs"
        | "status"
        | "video"
        | "gallery"
        | "carousel";
      const sourceLines: string[] = [];
      let closingIndex = -1;
      for (let next = index + 1; next < lines.length; next += 1) {
        if (lines[next].trim() === "```") {
          closingIndex = next;
          break;
        }
        sourceLines.push(lines[next]);
      }
      if (closingIndex === -1) {
        markdownBuffer.push(...lines.slice(index));
        break;
      }
      flushMarkdown();
      parts.push({
        type: kind,
        source: sourceLines.join("\n").trim(),
        key: `${kind}-${partIndex}`,
      });
      partIndex += 1;
      index = closingIndex;
      continue;
    }
    if (/^```/.test(line.trim())) inGenericFence = !inGenericFence;
    if (!inGenericFence && /^\$\$\s*$/.test(line.trim())) inMathFence = !inMathFence;
    markdownBuffer.push(line);
  }

  flushMarkdown();
  return parts;
}

function unwrapPlainSectionsContainingDataview(content: string): string {
  const lines = content.split("\n");
  const removableBoundaries = new Set<number>();
  const sections: Array<{ startIndex: number; isPlainSingle: boolean }> = [];
  let inFence = false;
  let inMathBlock = false;

  lines.forEach((line, index) => {
    const trimmed = line.trim();
    if (/^```/.test(trimmed)) {
      inFence = !inFence;
      return;
    }
    if (!inFence && /^\$\$\s*$/.test(trimmed)) {
      inMathBlock = !inMathBlock;
      return;
    }
    if (inFence || inMathBlock) return;

    const options = parseSectionStartOptions(trimmed);
    if (options) {
      const { style } = options;
      sections.push({
        startIndex: index,
        isPlainSingle:
          options.layout === "single" &&
          !style.borderColor &&
          !style.backgroundColor &&
          !style.contentBackgroundColor &&
          !style.headerBackgroundColor &&
          !style.headerText,
      });
      return;
    }

    if (trimmed !== SECTION_END_MARKER || sections.length === 0) return;
    const section = sections.pop();
    if (!section?.isPlainSingle) return;
    const containsDataview = lines
      .slice(section.startIndex + 1, index)
      .some((candidate) => /^```(?:dataview|dv)\s*$/i.test(candidate.trim()));
    if (containsDataview) {
      removableBoundaries.add(section.startIndex);
      removableBoundaries.add(index);
    }
  });

  return lines.filter((_line, index) => !removableBoundaries.has(index)).join("\n");
}

function TableOfContentsPreview({ content }: { content: string }) {
  const entries: TableOfContentsEntry[] = parseDocumentHeadings(content);

  return (
    <TableOfContentsList
      entries={entries}
      onSelect={(entry) => {
        const root = document.querySelector(".markdown-preview");
        if (!root) return;
        const entryIndex = entries.findIndex((candidate) => candidate.id === entry.id);
        const headings = Array.from(root.querySelectorAll<HTMLElement>("h1, h2, h3, h4, h5, h6"));
        const target = headings[entryIndex];
        if (!target) return;
        target.tabIndex = -1;
        target.scrollIntoView({ block: "start" });
        target.focus({ preventScroll: true });
      }}
    />
  );
}

function CalloutPreview({ kind, content }: { kind: CalloutKind; content: string }) {
  return (
    <aside
      data-callout="true"
      data-callout-kind={kind}
      role="note"
      aria-label={`${kind} callout`}
      className="rounded-card border-border bg-surface my-4 border p-4"
    >
      <CalloutHeader kind={kind} />
      <p className="text-text mt-3 whitespace-pre-wrap text-sm leading-6">{content}</p>
    </aside>
  );
}

function MathBlockPreview({ expression }: { expression: string }) {
  return (
    <div
      data-math-block="true"
      aria-label="Math block"
      className="rounded-card border-border bg-surface my-4 border px-4 py-2"
    >
      <MathBlockDisplay expression={expression} />
    </div>
  );
}

type DataviewBlockView = "table" | "list" | "cards";
type DataviewBaseColumn = "title" | "space" | "updated" | "excerpt";
type DataviewComputedExpression =
  | "daysSince(updated)"
  | "date(updated)"
  | "weekday(updated)"
  | "year(updated)"
  | "week(updated)"
  | "quarter(updated)"
  | "month(updated)"
  | "author"
  | "space"
  | "titleLength"
  | "excerptLength"
  | "titleWords"
  | "excerptWords"
  | "link";
type DataviewColumn =
  | DataviewBaseColumn
  | {
      id: string;
      label: string;
      expression: DataviewComputedExpression;
    };
type DataviewBaseSortField = "title" | "space" | "updated" | "author";
type DataviewSortField =
  | DataviewBaseSortField
  | {
      type: "computed";
      expression: DataviewComputedExpression;
    };
type DataviewSortDirection = "asc" | "desc";
type DataviewGroupField = "space" | "updated" | "month" | "year" | "author";

interface DataviewBlockConfig {
  name?: string;
  query: string;
  view: DataviewBlockView;
  limit: number;
  columns: DataviewColumn[];
  sort?: {
    field: DataviewSortField;
    direction: DataviewSortDirection;
  };
  group?: DataviewGroupField;
}

const DEFAULT_DATAVIEW_COLUMNS: DataviewBaseColumn[] = ["title", "space", "updated"];

export function getDataviewResultPath(result: SearchResultRecord): string {
  return buildDocumentRoutePath(result.space_id, result.document_id);
}

export function parseDataviewBlockConfig(source: string): DataviewBlockConfig {
  const queryLines: string[] = [];
  let name: string | undefined;
  let view: DataviewBlockView = "table";
  let limit = 10;
  let columns: DataviewColumn[] = DEFAULT_DATAVIEW_COLUMNS;
  let rawSort: string | undefined;
  let group: DataviewGroupField | undefined;

  for (const line of source.split("\n")) {
    const trimmed = line.trim();
    const option = trimmed.match(/^(name|view|limit|columns|column|sort|group)\s*:\s*(.+)$/i);
    if (!option) {
      queryLines.push(line);
      continue;
    }

    const key = option[1].toLowerCase();
    const value = option[2].trim();
    if (key === "name") {
      name = value.slice(0, 80) || undefined;
      continue;
    }
    if (key === "view") {
      const normalized = value.toLowerCase();
      if (normalized === "table" || normalized === "list" || normalized === "cards") {
        view = normalized;
      }
      continue;
    }
    if (key === "limit") {
      const parsed = Number.parseInt(value, 10);
      if (Number.isFinite(parsed)) {
        limit = Math.max(1, Math.min(parsed, 50));
      }
      continue;
    }
    if (key === "columns") {
      const parsed = value
        .split(",")
        .map((item) => item.trim().toLowerCase())
        .filter(
          (item): item is DataviewBaseColumn =>
            item === "title" || item === "space" || item === "updated" || item === "excerpt",
        );
      if (parsed.length > 0) columns = parsed;
      continue;
    }
    if (key === "column") {
      const computed = parseDataviewComputedColumn(value);
      if (computed) columns = [...columns, computed];
      continue;
    }
    if (key === "sort") {
      rawSort = value;
      continue;
    }
    if (key === "group") {
      const normalized = value.toLowerCase();
      if (
        normalized === "space" ||
        normalized === "updated" ||
        normalized === "month" ||
        normalized === "year" ||
        normalized === "author"
      ) {
        group = normalized;
      }
    }
  }

  return {
    name,
    query: queryLines.join("\n").trim(),
    view,
    limit,
    columns,
    sort: rawSort ? parseDataviewSort(rawSort, columns) : undefined,
    group,
  };
}

function parseDataviewComputedColumn(
  value: string,
): Extract<DataviewColumn, { id: string }> | undefined {
  const match = value.match(/^(.+?)\s*=\s*(.+)$/);
  if (!match) return undefined;
  const label = match[1].trim().slice(0, 40);
  const expression = normalizeDataviewComputedExpression(match[2]);
  if (!label || !expression) return undefined;
  const id = `computed:${label.toLowerCase().replace(/[^a-z0-9]+/g, "-")}:${expression}`;
  return { id, label, expression };
}

function normalizeDataviewComputedExpression(
  value: string,
): DataviewComputedExpression | undefined {
  const normalized = value.trim().replace(/\s+/g, "").toLowerCase();
  if (normalized === "dayssince(updated)" || normalized === "age") return "daysSince(updated)";
  if (normalized === "date(updated)" || normalized === "updated") return "date(updated)";
  if (normalized === "weekday(updated)") return "weekday(updated)";
  if (normalized === "year(updated)") return "year(updated)";
  if (normalized === "week(updated)" || normalized === "isoweek(updated)") return "week(updated)";
  if (normalized === "quarter(updated)") return "quarter(updated)";
  if (normalized === "month(updated)") return "month(updated)";
  if (normalized === "author") return "author";
  if (normalized === "space") return "space";
  if (normalized === "titlelength" || normalized === "length(title)") return "titleLength";
  if (normalized === "excerptlength" || normalized === "length(excerpt)") return "excerptLength";
  if (
    normalized === "titlewords" ||
    normalized === "words(title)" ||
    normalized === "wordcount(title)"
  ) {
    return "titleWords";
  }
  if (
    normalized === "excerptwords" ||
    normalized === "words(excerpt)" ||
    normalized === "wordcount(excerpt)"
  ) {
    return "excerptWords";
  }
  if (normalized === "link" || normalized === "url") return "link";
  return undefined;
}

function parseDataviewSort(value: string, columns: DataviewColumn[]): DataviewBlockConfig["sort"] {
  const parts = value.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return undefined;
  const last = parts[parts.length - 1]?.toLowerCase();
  const hasDirection = last === "asc" || last === "desc";
  const rawField = (hasDirection ? parts.slice(0, -1) : parts).join(" ");
  if (!rawField) return undefined;
  const normalizedField = rawField.toLowerCase();
  const field = normalizedField === "name" ? "title" : normalizedField;
  if (field !== "title" && field !== "space" && field !== "updated" && field !== "author") {
    const computed = resolveDataviewComputedSort(rawField, columns);
    if (!computed) return undefined;
    const direction = hasDirection && last === "desc" ? "desc" : "asc";
    return { field: { type: "computed", expression: computed.expression }, direction };
  }
  const direction = hasDirection && last === "desc" ? "desc" : "asc";
  return { field, direction };
}

function resolveDataviewComputedSort(
  rawField: string,
  columns: DataviewColumn[],
): Extract<DataviewColumn, { id: string }> | undefined {
  const directExpression = normalizeDataviewComputedExpression(rawField);
  if (directExpression) {
    return {
      id: `computed:sort:${directExpression}`,
      label: rawField,
      expression: directExpression,
    };
  }

  const normalizedField = rawField.trim().toLowerCase();
  return columns.find(
    (column): column is Extract<DataviewColumn, { id: string }> =>
      typeof column !== "string" &&
      (column.label.toLowerCase() === normalizedField ||
        column.id.toLowerCase() === normalizedField ||
        column.expression.toLowerCase() === normalizedField),
  );
}

function dataviewColumnKey(column: DataviewColumn): string {
  return typeof column === "string" ? column : column.id;
}

export function sortDataviewResults(
  results: SearchResultRecord[],
  sort: DataviewBlockConfig["sort"],
  now = new Date(),
): SearchResultRecord[] {
  if (!sort) return results;
  const direction = sort.direction === "desc" ? -1 : 1;
  return [...results].sort((a, b) => {
    if (typeof sort.field !== "string") {
      const aValue = evaluateDataviewComputedSortValue(a, sort.field.expression, now);
      const bValue = evaluateDataviewComputedSortValue(b, sort.field.expression, now);
      return (
        compareDataviewSortValues(aValue, bValue) * direction || a.title.localeCompare(b.title)
      );
    }
    if (sort.field === "updated") {
      const aTime = new Date(a.updated_at).getTime();
      const bTime = new Date(b.updated_at).getTime();
      const comparison = (Number.isNaN(aTime) ? 0 : aTime) - (Number.isNaN(bTime) ? 0 : bTime);
      return comparison * direction || a.title.localeCompare(b.title);
    }
    const aValue =
      sort.field === "space" ? a.space_name : sort.field === "author" ? a.author : a.title;
    const bValue =
      sort.field === "space" ? b.space_name : sort.field === "author" ? b.author : b.title;
    return aValue.localeCompare(bValue) * direction || a.title.localeCompare(b.title);
  });
}

function compareDataviewSortValues(a: string | number, b: string | number): number {
  if (typeof a === "number" && typeof b === "number") return a - b;
  return String(a).localeCompare(String(b));
}

export function groupDataviewResults(
  results: SearchResultRecord[],
  group: DataviewBlockConfig["group"],
): Array<{ key: string; label: string; results: SearchResultRecord[] }> {
  if (!group) return [{ key: "all", label: "", results }];
  const groups = new Map<string, { key: string; label: string; results: SearchResultRecord[] }>();
  results.forEach((result) => {
    const label =
      group === "space"
        ? result.space_name || "Unknown space"
        : group === "author"
          ? result.author || "Unknown author"
          : normalizeDataviewGroupDate(result.updated_at, group);
    const key = `${group}:${label}`;
    const current = groups.get(key) ?? { key, label, results: [] };
    current.results.push(result);
    groups.set(key, current);
  });
  return [...groups.values()];
}

function normalizeDataviewGroupDate(value: string, group: "updated" | "month" | "year"): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Unknown date";
  if (group === "year") return date.toISOString().slice(0, 4);
  if (group === "month") return date.toISOString().slice(0, 7);
  return date.toISOString().slice(0, 10);
}

function getPreviewLinkPosition(rect: DOMRect): { top: number; left: number } {
  const width = 320;
  const height = 40;
  const viewportWidth = typeof window === "undefined" ? 1024 : window.innerWidth;
  const viewportHeight = typeof window === "undefined" ? 768 : window.innerHeight;
  const top = Math.max(8, Math.min(rect.top - height - 8, viewportHeight - height - 8));
  const left = Math.max(
    8,
    Math.min(rect.left + rect.width / 2 - width / 2, viewportWidth - width - 8),
  );
  return { top, left };
}

function formatDataviewDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

export function evaluateDataviewComputedColumn(
  result: SearchResultRecord,
  column: Extract<DataviewColumn, { id: string }>,
  now = new Date(),
): string {
  const updated = new Date(result.updated_at);
  switch (column.expression) {
    case "daysSince(updated)": {
      if (Number.isNaN(updated.getTime())) return "";
      const difference = Math.max(0, now.getTime() - updated.getTime());
      const days = Math.floor(difference / 86_400_000);
      if (days === 0) return "Today";
      if (days === 1) return "1 day";
      return `${days} days`;
    }
    case "date(updated)":
      if (Number.isNaN(updated.getTime())) return "";
      return updated.toISOString().slice(0, 10);
    case "weekday(updated)":
      if (Number.isNaN(updated.getTime())) return "";
      return updated.toLocaleDateString(undefined, { weekday: "short" });
    case "year(updated)":
      if (Number.isNaN(updated.getTime())) return "";
      return String(updated.getFullYear());
    case "week(updated)": {
      if (Number.isNaN(updated.getTime())) return "";
      const { year, week } = getUtcIsoWeek(updated);
      return `${year}-W${String(week).padStart(2, "0")}`;
    }
    case "quarter(updated)": {
      if (Number.isNaN(updated.getTime())) return "";
      const quarter = Math.floor(updated.getUTCMonth() / 3) + 1;
      return `Q${quarter} ${updated.getUTCFullYear()}`;
    }
    case "month(updated)":
      if (Number.isNaN(updated.getTime())) return "";
      return updated.toISOString().slice(0, 7);
    case "author":
      return result.author || "";
    case "space":
      return result.space_name || "";
    case "titleLength":
      return String(result.title.length);
    case "excerptLength":
      return String(result.excerpt.length);
    case "titleWords":
      return String(countDataviewWords(result.title));
    case "excerptWords":
      return String(countDataviewWords(result.excerpt));
    case "link":
      return getDataviewResultPath(result);
  }
}

function countDataviewWords(value: string): number {
  return value.trim().split(/\s+/).filter(Boolean).length;
}

function evaluateDataviewComputedSortValue(
  result: SearchResultRecord,
  expression: DataviewComputedExpression,
  now: Date,
): string | number {
  const updated = new Date(result.updated_at);
  switch (expression) {
    case "daysSince(updated)":
      if (Number.isNaN(updated.getTime())) return 0;
      return Math.max(0, Math.floor((now.getTime() - updated.getTime()) / 86_400_000));
    case "date(updated)":
      return Number.isNaN(updated.getTime()) ? 0 : updated.getTime();
    case "weekday(updated)":
      return Number.isNaN(updated.getTime())
        ? ""
        : updated.toLocaleDateString(undefined, { weekday: "short" });
    case "year(updated)":
      return Number.isNaN(updated.getTime()) ? 0 : updated.getUTCFullYear();
    case "week(updated)": {
      if (Number.isNaN(updated.getTime())) return 0;
      const { year, week } = getUtcIsoWeek(updated);
      return year * 100 + week;
    }
    case "quarter(updated)":
      return Number.isNaN(updated.getTime())
        ? 0
        : updated.getUTCFullYear() * 4 + Math.floor(updated.getUTCMonth() / 3);
    case "month(updated)":
      return Number.isNaN(updated.getTime())
        ? 0
        : updated.getUTCFullYear() * 12 + updated.getUTCMonth();
    case "author":
      return result.author || "";
    case "space":
      return result.space_name || "";
    case "titleLength":
      return result.title.length;
    case "excerptLength":
      return result.excerpt.length;
    case "titleWords":
      return countDataviewWords(result.title);
    case "excerptWords":
      return countDataviewWords(result.excerpt);
    case "link":
      return getDataviewResultPath(result);
  }
}

function getUtcIsoWeek(value: Date): { year: number; week: number } {
  const date = new Date(Date.UTC(value.getUTCFullYear(), value.getUTCMonth(), value.getUTCDate()));
  const day = date.getUTCDay() || 7;
  date.setUTCDate(date.getUTCDate() + 4 - day);
  const yearStart = new Date(Date.UTC(date.getUTCFullYear(), 0, 1));
  const week = Math.ceil(((date.getTime() - yearStart.getTime()) / 86_400_000 + 1) / 7);
  return { year: date.getUTCFullYear(), week };
}

function DataviewBlock({ query }: { query: string }) {
  const config = parseDataviewBlockConfig(query);
  const [results, setResults] = useState<SearchResultRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [errorRetryable, setErrorRetryable] = useState(false);
  const [requestVersion, setRequestVersion] = useState(0);
  const [interactiveSort, setInteractiveSort] = useState<DataviewBlockConfig["sort"] | undefined>();

  useEffect(() => {
    let canceled = false;
    const trimmed = config.query.trim();
    if (trimmed.length < 2) {
      setResults([]);
      setLoading(false);
      setError("Add a Dataview query to show matching pages.");
      setErrorRetryable(false);
      return;
    }

    setLoading(true);
    setError("");
    setErrorRetryable(false);
    dataClient
      .queryDataview(trimmed, config.limit)
      .then((records) => {
        if (canceled) return;
        setResults(records);
      })
      .catch(() => {
        if (canceled) return;
        setResults([]);
        setError("Dataview query is not available for this storage mode yet.");
        setErrorRetryable(true);
      })
      .finally(() => {
        if (!canceled) setLoading(false);
      });

    return () => {
      canceled = true;
    };
  }, [config.limit, config.query, requestVersion]);

  const activeSort = interactiveSort ?? config.sort;
  const displayResults = sortDataviewResults(results, activeSort);
  const groupedResults = groupDataviewResults(displayResults, config.group);

  const renderColumnHeader = (column: DataviewColumn) => {
    if (typeof column !== "string") return column.label;
    switch (column) {
      case "title":
        return "Page";
      case "space":
        return "Space";
      case "updated":
        return "Updated";
      case "excerpt":
        return "Excerpt";
    }
  };

  const renderColumn = (result: SearchResultRecord, column: DataviewColumn) => {
    if (typeof column !== "string") {
      const value = evaluateDataviewComputedColumn(result, column);
      return <span className="text-text-muted whitespace-nowrap">{value}</span>;
    }
    switch (column) {
      case "title":
        return <span className="text-text font-medium">{result.title}</span>;
      case "space":
        return <span className="text-text-muted">{result.space_name}</span>;
      case "updated":
        return (
          <span className="text-text-muted whitespace-nowrap">
            {formatDataviewDate(result.updated_at)}
          </span>
        );
      case "excerpt":
        return <span className="text-text-muted text-xs leading-5">{result.excerpt}</span>;
    }
  };
  const toggleColumnSort = (column: DataviewColumn) => {
    const field: DataviewSortField =
      typeof column === "string"
        ? column === "excerpt"
          ? "title"
          : column
        : { type: "computed", expression: column.expression };
    const sameField = activeSort && JSON.stringify(activeSort.field) === JSON.stringify(field);
    setInteractiveSort({
      field,
      direction: sameField && activeSort?.direction === "asc" ? "desc" : "asc",
    });
  };
  const getColumnSort = (column: DataviewColumn): DataviewSortDirection | undefined => {
    const field: DataviewSortField =
      typeof column === "string"
        ? column === "excerpt"
          ? "title"
          : column
        : { type: "computed", expression: column.expression };
    return activeSort && JSON.stringify(activeSort.field) === JSON.stringify(field)
      ? activeSort.direction
      : undefined;
  };

  return (
    <div
      className="border-border bg-surface rounded-card my-4 overflow-hidden border"
      role="region"
      aria-label={`${config.name || "Dataview"} collection`}
      aria-busy={loading}
    >
      <div className="border-border bg-surface-raised flex items-center justify-between gap-3 border-b px-3 py-2">
        <div>
          <p className="text-text text-sm font-medium">{config.name || "Dataview"}</p>
          <p className="text-text-muted mt-0.5 max-w-full truncate font-mono text-xs">
            {config.query || "Add a property or page query"}
          </p>
        </div>
        <span
          className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1"
          role="status"
          aria-live="polite"
        >
          {loading
            ? "Running"
            : error
              ? errorRetryable
                ? "Unavailable"
                : "Needs setup"
              : `${results.length} results`}
        </span>
      </div>

      {error ? (
        <div className="flex items-center justify-between gap-3 px-3 py-3" role="alert">
          <p className="text-text-muted flex min-w-0 items-start gap-2 text-sm">
            <AlertCircle aria-hidden="true" className="text-danger mt-0.5 shrink-0" size={16} />
            <span>{error}</span>
          </p>
          {errorRetryable ? (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              className="shrink-0"
              onClick={() => setRequestVersion((version) => version + 1)}
            >
              <RefreshCw aria-hidden="true" size={14} />
              Retry
            </Button>
          ) : null}
        </div>
      ) : loading ? (
        <div className="space-y-2 px-3 py-3" aria-label="Loading collection results">
          <span className="sr-only">Running collection query.</span>
          {["70%", "58%", "64%"].map((width) => (
            <div
              key={width}
              className="border-border bg-surface-raised h-8 rounded border motion-safe:animate-pulse"
              style={{ width }}
              aria-hidden="true"
            />
          ))}
        </div>
      ) : results.length === 0 ? (
        <p className="text-text-muted px-3 py-3 text-sm">No matching pages.</p>
      ) : config.view === "list" ? (
        <div className="divide-border divide-y">
          {groupedResults.map((group) => (
            <section key={group.key}>
              {config.group ? (
                <h4 className="bg-surface-raised/50 text-text-muted px-3 py-2 text-xs font-medium">
                  {group.label}
                </h4>
              ) : null}
              {group.results.map((result) => (
                <Link
                  key={`${result.space_id}-${result.document_id}`}
                  to={getDataviewResultPath(result)}
                  aria-label={`Open ${result.title}`}
                  className="starlane-collection-link focus-ring hover:bg-surface-raised/70 block w-full px-3 py-3 text-left transition-colors duration-100"
                >
                  <span className="text-text block text-sm font-medium">{result.title}</span>
                  <span className="text-text-muted mt-1 block text-xs">
                    {result.space_name} · {formatDataviewDate(result.updated_at)}
                  </span>
                  {result.excerpt ? (
                    <span className="text-text-muted mt-2 block text-xs leading-5">
                      {result.excerpt}
                    </span>
                  ) : null}
                </Link>
              ))}
            </section>
          ))}
        </div>
      ) : config.view === "cards" ? (
        <div className="space-y-3 p-3">
          {groupedResults.map((group) => (
            <section key={group.key}>
              {config.group ? (
                <h4 className="text-text-muted mb-2 text-xs font-medium">{group.label}</h4>
              ) : null}
              <div className="grid gap-2 sm:grid-cols-2">
                {group.results.map((result) => (
                  <Link
                    key={`${result.space_id}-${result.document_id}`}
                    to={getDataviewResultPath(result)}
                    aria-label={`Open ${result.title}`}
                    className="starlane-collection-link focus-ring border-border hover:bg-surface-raised/70 rounded-card border p-3 text-left transition-colors duration-100"
                  >
                    <span className="text-text block text-sm font-medium">{result.title}</span>
                    <span className="text-text-muted mt-1 block text-xs">
                      {result.space_name} · {formatDataviewDate(result.updated_at)}
                    </span>
                    {result.excerpt ? (
                      <span className="text-text-muted mt-2 block text-xs leading-5">
                        {result.excerpt}
                      </span>
                    ) : null}
                  </Link>
                ))}
              </div>
            </section>
          ))}
        </div>
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full text-left text-sm">
            <thead className="text-text-muted bg-surface-raised text-xs">
              <tr>
                {config.columns.map((column) => {
                  const columnSort = getColumnSort(column);
                  return (
                    <th
                      key={dataviewColumnKey(column)}
                      className="px-3 py-2 font-medium"
                      aria-sort={
                        columnSort === "asc"
                          ? "ascending"
                          : columnSort === "desc"
                            ? "descending"
                            : "none"
                      }
                    >
                      <button
                        type="button"
                        className="hover:text-text flex items-center gap-1 text-left"
                        onClick={() => toggleColumnSort(column)}
                        aria-label={`Sort by ${renderColumnHeader(column)}`}
                      >
                        {renderColumnHeader(column)}
                        {columnSort === "asc" ? (
                          <ArrowUp size={13} />
                        ) : columnSort === "desc" ? (
                          <ArrowDown size={13} />
                        ) : (
                          <ArrowUpDown size={13} />
                        )}
                      </button>
                    </th>
                  );
                })}
              </tr>
            </thead>
            <tbody>
              {groupedResults.flatMap((group) => [
                config.group ? (
                  <tr
                    key={`${group.key}-heading`}
                    className="border-border bg-surface-raised/40 border-t"
                  >
                    <td
                      colSpan={config.columns.length}
                      className="text-text-muted px-3 py-2 text-xs font-medium"
                    >
                      {group.label}
                    </td>
                  </tr>
                ) : null,
                ...group.results.map((result) => (
                  <tr
                    key={`${result.space_id}-${result.document_id}`}
                    className="border-border hover:bg-surface-raised/70 relative border-t transition-colors duration-100"
                  >
                    {config.columns.map((column, columnIndex) => {
                      const content = renderColumn(result, column);
                      return (
                        <td key={dataviewColumnKey(column)} className="px-3 py-2 align-top">
                          {columnIndex === 0 ? (
                            <Link
                              to={getDataviewResultPath(result)}
                              aria-label={`Open ${result.title}`}
                              className="starlane-collection-link focus-ring block after:absolute after:inset-0 after:content-['']"
                            >
                              {content}
                            </Link>
                          ) : (
                            content
                          )}
                        </td>
                      );
                    })}
                  </tr>
                )),
              ])}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

export function applyPreviewInlineCommentHighlights(
  root: HTMLElement,
  comments: DocumentComment[],
): () => void {
  const marks: HTMLElement[] = [];
  for (const comment of comments) {
    const selection = comment.selection_text?.trim();
    if (comment.resolved || !selection) continue;

    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let current = walker.nextNode();
    while (current) {
      const textNode = current as Text;
      const parent = textNode.parentElement;
      const index = textNode.data.indexOf(selection);
      if (
        index >= 0 &&
        parent &&
        !parent.closest("[data-inline-comment-id], pre, code, textarea, input")
      ) {
        const selectedNode = textNode.splitText(index);
        selectedNode.splitText(selection.length);
        const mark = document.createElement("mark");
        mark.className = "starlane-inline-comment";
        mark.dataset.inlineCommentId = comment.id;
        parent.insertBefore(mark, selectedNode);
        mark.append(selectedNode);
        marks.push(mark);
        break;
      }
      current = walker.nextNode();
    }
  }

  return () => {
    for (const mark of marks) {
      if (!mark.parentNode) continue;
      mark.replaceWith(document.createTextNode(mark.textContent || ""));
    }
    root.normalize();
  };
}

export function MarkdownPreview({
  content,
  assetContext = {},
  inlineCommentsVisible = false,
  spaceId,
  docId,
  onInlineCommentOpen,
  onCommentCreated,
  onDocumentNavigate,
  selectionCommentsEnabled = false,
  templateMarkerMode = "omit",
  templateFields = [],
}: MarkdownPreviewProps) {
  const previewRef = useRef<HTMLElement | null>(null);
  const commentsAvailable = Boolean(spaceId && docId && selectionCommentsEnabled);
  const [documentComments, setDocumentComments] = useState<DocumentComment[]>([]);
  const [commentSelection, setCommentSelection] = useState<PreviewCommentSelection | null>(null);
  const [commentDialog, setCommentDialog] = useState<{ selectionText: string } | null>(null);
  const [commentContent, setCommentContent] = useState("");
  const [commentError, setCommentError] = useState("");
  const [commentSaving, setCommentSaving] = useState(false);
  const assetSpaceId = assetContext.space_id;
  const assetDocumentId = assetContext.document_id;
  // selection-owned link preview: this virtual anchor follows the clicked inline link
  // while preserving the read-only document selection.
  const [linkMenu, setLinkMenu] = useState<PreviewLinkMenu | null>(null);
  const linkMenuTriggerRef = useRef<HTMLAnchorElement | null>(null);
  const linkMenuRef = useRef<HTMLDivElement | null>(null);
  const handleLinkMenuKeyDown = useMenuKeyboardNavigation(linkMenuRef);
  const [attachmentUrls, setAttachmentUrls] = useState<Map<string, string>>(() => new Map());
  const [attachmentRecords, setAttachmentRecords] = useState<Map<string, AttachmentRecord>>(
    () => new Map(),
  );
  const [attachmentDownloadError, setAttachmentDownloadError] = useState("");
  const hydratedContent = replaceMarkdownAttachmentReferences(content, attachmentUrls);
  const parts = splitMarkdownDataviewBlocks(hydratedContent);

  useEffect(() => {
    if (!assetSpaceId || !assetDocumentId || getCurrentLaunchMode() === "local") {
      setAttachmentUrls(new Map());
      setAttachmentRecords(new Map());
      return;
    }

    const ids = Array.from(
      new Set(
        Array.from(content.matchAll(/starlane-attachment:\/\/([^\s)]+)/gi)).map(
          (match) => match[1],
        ),
      ),
    );
    if (ids.length === 0) {
      setAttachmentUrls(new Map());
      setAttachmentRecords(new Map());
      return;
    }

    const imageIds = new Set(
      Array.from(content.matchAll(/!\[[^\]\n]*\]\(starlane-attachment:\/\/([^\s)]+)/gi)).map(
        (match) => match[1],
      ),
    );

    let cancelled = false;
    const ownedImageUrls:string[]=[];
    const load = async () => {
      const attachments = await dataClient
        .listDocumentAttachments(assetSpaceId, assetDocumentId)
        .catch(() => []);
      const byId = new Map(attachments.map((attachment) => [attachment.id, attachment]));
      if (!cancelled) setAttachmentRecords(byId);
      const resolved = await Promise.all(
        Array.from(imageIds).map(async (id) => {
          const url = await dataClient.previewPinnedDocumentAttachmentVersionImage(assetSpaceId,assetDocumentId,id).catch(() => null);
          if(url?.startsWith("blob:")){if(cancelled){releaseBrowserAttachmentImage(url);return null;}ownedImageUrls.push(url);}
          return url ? ([id, url] as [string, string]) : null;
        }),
      );
      if (!cancelled)
        setAttachmentUrls(
          new Map(resolved.filter((entry): entry is [string, string] => Boolean(entry))),
        );
    };
    void load();
    return () => {
      cancelled = true;
      ownedImageUrls.forEach(releaseBrowserAttachmentImage);
    };
  }, [assetDocumentId, assetSpaceId, content]);

  const downloadPreviewAttachment = async (button: HTMLButtonElement) => {
    const card = button.closest<HTMLElement>("[data-type='file-attachment']");
    const attachmentId = card?.dataset.attachmentId || "";
    const name = card?.dataset.name || "attachment";
    const originalAction = button.querySelector<HTMLElement>(".file-attachment-card__action");
    setAttachmentDownloadError("");
    button.disabled = true;
    if (originalAction) originalAction.textContent = "Downloading…";
    try {
      if(isNativeAttachmentShell()) {
        const selectedContext=captureAttachmentContext();
        if(!attachmentId||!assetSpaceId||!assetDocumentId)throw new Error("Exact attachment version and current page required for native Save file.");
        await dataClient.savePinnedDocumentAttachmentVersion(assetSpaceId,assetDocumentId,attachmentId);
        verifyAttachmentContext(selectedContext,attachmentApiBase(),attachmentApi.getToken());
        return;
      }
      if (attachmentId && assetSpaceId && assetDocumentId) {
        await saveBrowserPinnedAttachment({space_id:assetSpaceId,document_id:assetDocumentId},attachmentId,name);
        return;
      }
      let url = card?.dataset.url || "";
      if (!url && attachmentId && assetSpaceId && assetDocumentId) {
        let attachment = attachmentRecords.get(attachmentId);
        if (!attachment) {
          const attachments = await dataClient.listDocumentAttachments(
            assetSpaceId,
            assetDocumentId,
          );
          attachment = attachments.find((candidate) => candidate.id === attachmentId);
        }
        if (!attachment) throw new Error("This attachment is no longer available.");
        url = await dataClient.downloadDocumentAttachmentToVault(
          assetSpaceId,
          assetDocumentId,
          attachment,
        );
      }
      if (!url) throw new Error("This attachment does not have downloadable data.");
      await triggerMediaDownload(url, name);
    } catch (error) {
      setAttachmentDownloadError(
        error instanceof Error ? error.message : "Could not download this attachment.",
      );
    } finally {
      button.disabled = false;
      if (originalAction) originalAction.textContent = "Download";
    }
  };

  useEffect(() => {
    if (!spaceId || !docId) {
      setDocumentComments([]);
      return;
    }

    let cancelled = false;
    const load = async () => {
      try {
        const comments = await listDocumentComments(spaceId, docId);
        if (!cancelled) setDocumentComments(comments);
      } catch {
        if (!cancelled) setDocumentComments([]);
      }
    };
    void load();
    const unsubscribe = subscribeToDocumentComments(spaceId, docId, () => void load());
    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, [docId, spaceId]);

  useEffect(() => {
    const root = previewRef.current;
    if (!root || !inlineCommentsVisible) return;
    return applyPreviewInlineCommentHighlights(root, documentComments);
  }, [content, documentComments, inlineCommentsVisible]);

  // Code Tabs nested inside enforced sections are emitted by markdownToHtml
  // as static HTML so the section grid can stay a single DOM tree. Rehydrate
  // their tab strip here so those tabs behave exactly like top-level tabs.
  useEffect(() => {
    const root = previewRef.current;
    if (!root) return;
    const handleCodeTabClick = (event: MouseEvent) => {
      const target = event.target;
      if (!(target instanceof HTMLElement)) return;
      const tab = target.closest<HTMLElement>(
        '[data-code-tabs-preview="true"] .code-tabs-preview__tab',
      );
      if (!tab) return;
      const block = tab.closest<HTMLElement>('[data-code-tabs-block="true"]');
      const code = block?.querySelector<HTMLElement>(".code-tabs-preview__code code");
      if (!block || !code) return;
      let tabs: Array<{ label?: string; language?: string; code?: string }> = [];
      try {
        tabs = JSON.parse(block.dataset.codeTabsItems || "[]");
      } catch {
        return;
      }
      const tabIndex = Array.from(tab.parentElement?.children || []).indexOf(tab);
      const active = tabs[tabIndex];
      if (!active) return;
      event.preventDefault();
      block.querySelectorAll<HTMLElement>(".code-tabs-preview__tab").forEach((item, index) => {
        item.classList.toggle("is-active", index === tabIndex);
        item.setAttribute("aria-selected", String(index === tabIndex));
      });
      code.className = active.language ? `language-${active.language}` : "language-text";
      code.innerHTML = highlightedCodeToHtml(active.code || "", active.language || "");
    };
    root.addEventListener("click", handleCodeTabClick);
    return () => root.removeEventListener("click", handleCodeTabClick);
  }, [hydratedContent]);

  useEffect(() => {
    const root = previewRef.current;
    if (!root || !spaceId || !docId) return;
    const blocks = Array.from(
      root.querySelectorAll<HTMLElement>("[data-child-pages-hydrate='true']"),
    );
    if (blocks.length === 0) return;
    let cancelled = false;
    void dataClient
      .listDocuments(spaceId, { include_nested: true })
      .then((documents) => {
        if (cancelled) return;
        const children = getDirectChildPages(documents, docId);
        blocks.forEach((block) => {
          const variant =
            block.dataset.childPagesVariant === "collapsible" ? "collapsible" : "list";
          block.innerHTML = renderChildPagesHtml(children, spaceId, variant);
        });
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [docId, hydratedContent, spaceId]);

  useEffect(() => {
    if (!commentsAvailable) {
      setCommentSelection(null);
      return;
    }

    const updateCommentSelection = () => {
      const root = previewRef.current;
      const selection = window.getSelection();
      if (
        !root ||
        !selection ||
        selection.rangeCount === 0 ||
        selection.isCollapsed ||
        !selection.anchorNode ||
        !selection.focusNode ||
        !root.contains(selection.anchorNode) ||
        !root.contains(selection.focusNode)
      ) {
        setCommentSelection(null);
        return;
      }

      const selectionText = selection.toString().trim().slice(0, 2_000);
      if (!selectionText) {
        setCommentSelection(null);
        return;
      }

      const rect = selection.getRangeAt(0).getBoundingClientRect();
      setCommentSelection({
        selectionText,
        position: getPreviewSelectionCommentPosition(rect, window.innerWidth, window.innerHeight),
      });
    };

    document.addEventListener("selectionchange", updateCommentSelection);
    return () => document.removeEventListener("selectionchange", updateCommentSelection);
  }, [commentsAvailable]);

  const openLinkMenu = (anchor: HTMLAnchorElement) => {
    linkMenuTriggerRef.current = anchor;
    setLinkMenu({
      href: anchor.href || anchor.getAttribute("href") || "",
      position: getPreviewLinkPosition(anchor.getBoundingClientRect()),
    });
  };
  const closeLinkMenu = () => setLinkMenu(null);
  const closeAndRestoreLinkMenu = useOverlayFocusLifecycle({
    open: Boolean(linkMenu),
    onClose: closeLinkMenu,
    containerRef: linkMenuRef,
    triggerRef: linkMenuTriggerRef,
  });

  const openSelectionCommentDialog = () => {
    if (!commentSelection) return;
    setCommentContent("");
    setCommentError("");
    setCommentDialog({ selectionText: commentSelection.selectionText });
    setCommentSelection(null);
  };

  const closeSelectionCommentDialog = () => {
    if (commentSaving) return;
    setCommentDialog(null);
    setCommentContent("");
    setCommentError("");
  };

  const submitSelectionComment = async () => {
    if (!spaceId || !docId || !commentDialog || !commentContent.trim() || commentSaving) {
      return;
    }

    setCommentSaving(true);
    setCommentError("");
    try {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/comments`, {
        method: "POST",
        body: {
          content: commentContent.trim(),
          selection_text: commentDialog.selectionText,
        },
      });
      setCommentDialog(null);
      setCommentContent("");
      emitDocumentCommentsChanged(spaceId, docId);
      onCommentCreated?.();
    } catch {
      setCommentError("Unable to add your comment. Please try again.");
    } finally {
      setCommentSaving(false);
    }
  };

  return (
    <section
      ref={previewRef}
      className="markdown-preview min-h-[300px] px-0 py-1"
      aria-label="Markdown preview"
      onClick={(event) => {
        const target = event.target;
        if (!(target instanceof Element)) return;
        const inlineComment = target.closest<HTMLElement>("[data-inline-comment-id]");
        if (inlineComment) {
          onInlineCommentOpen?.(inlineComment.dataset.inlineCommentId || "");
          return;
        }
        const attachmentButton = target.closest<HTMLButtonElement>(
          "[data-file-attachment-download='true']",
        );
        if (attachmentButton) {
          event.preventDefault();
          void downloadPreviewAttachment(attachmentButton);
          return;
        }
        const anchor = target.closest("a[href]");
        if (!(anchor instanceof HTMLAnchorElement)) return;
        const wikiDocumentId = parseWikiDocumentHref(anchor.getAttribute("href") || "");
        if (wikiDocumentId) {
          event.preventDefault();
          setLinkMenu(null);
          onDocumentNavigate?.(wikiDocumentId);
          return;
        }
        event.preventDefault();
        openLinkMenu(anchor);
      }}
      onKeyDown={(event) => {
        if (event.key !== " ") return;
        const target = event.target;
        if (!(target instanceof Element)) return;
        const anchor = target.closest<HTMLAnchorElement>(
          `a[href^="${WIKI_DOCUMENT_HREF_PREFIX}"]`,
        );
        const wikiDocumentId = anchor
          ? parseWikiDocumentHref(anchor.getAttribute("href") || "")
          : null;
        if (!wikiDocumentId) return;
        event.preventDefault();
        onDocumentNavigate?.(wikiDocumentId);
      }}
    >
      {commentSelection && commentsAvailable && (
        <div
          className={`${CONTROL_CHROME_RAIL_CLASS} shadow-card fixed z-[70]`}
          style={commentSelection.position}
          role="toolbar"
          aria-label="Page selection comment action"
          data-starlane-preview-selection-comment="true"
        >
          <button
            type="button"
            className={`${CONTROL_CHROME_ACTION_CLASS} flex h-9 w-9 items-center justify-center`}
            onPointerDown={(event) => event.preventDefault()}
            onClick={openSelectionCommentDialog}
            aria-label="Comment on selected text"
            title="Comment on selected text"
          >
            <MessageSquare size={14} />
          </button>
        </div>
      )}
      <div className="tiptap-editor">
        {parts.map((part) =>
          part.type === "markdown" ? (
            <div
              key={part.key}
              dangerouslySetInnerHTML={{
                __html: renderMarkdownPreviewHtml(part.content, templateMarkerMode, templateFields),
              }}
            />
          ) : part.type === "dataview" ? (
            <DataviewBlock key={part.key} query={part.query} />
          ) : part.type === "callout" ? (
            <CalloutPreview key={part.key} kind={part.kind} content={part.content} />
          ) : part.type === "math" ? (
            <MathBlockPreview key={part.key} expression={part.expression} />
          ) : part.type === "child-pages" ? (
            <ChildPagesBlock
              key={part.key}
              spaceId={spaceId}
              docId={docId}
              variant={part.variant}
            />
          ) : part.type === "code-tabs" ? (
            <PortableCodeTabs key={part.key} {...parsePortableCodeTabs(part.source)} />
          ) : part.type === "status" ? (
            <PortableStatusBlock key={part.key} config={parsePortableStatusBlock(part.source)} />
          ) : part.type === "video" ? (
            (() => {
              const config = parsePortableVideoBlock(part.source);
              return config ? (
                <PortableVideoPlayer key={part.key} config={config} />
              ) : (
                <p key={part.key} className="text-text-muted my-4 text-sm">
                  Video source is missing or unsupported.
                </p>
              );
            })()
          ) : part.type === "gallery" ? (
            <PortableGallery key={part.key} config={parsePortableGalleryBlock(part.source)} />
          ) : part.type === "carousel" ? (
            <PortableGallery
              key={part.key}
              config={{ ...parsePortableGalleryBlock(part.source), layout: "carousel-7" }}
            />
          ) : part.type === "code" ? (
            <PortableCodeBlock
              key={part.key}
              code={part.code}
              language={part.language}
              title={part.title}
              placement={part.placement}
            />
          ) : (
            <TableOfContentsPreview key={part.key} content={hydratedContent} />
          ),
        )}
      </div>
      {attachmentDownloadError ? (
        <p role="alert" className="text-danger mt-2 text-sm">
          {attachmentDownloadError}
        </p>
      ) : null}
      {linkMenu && (
        <>
          <button
            type="button"
            aria-label="Close link menu"
            className="fixed inset-0 z-[65] cursor-default bg-transparent"
            onClick={closeAndRestoreLinkMenu}
          />
          <div
            ref={linkMenuRef}
            className="starlane-menu-surface fixed z-[70] flex max-w-[min(20rem,calc(100vw-1rem))] items-center gap-1 p-1"
            style={{ top: linkMenu.position.top, left: linkMenu.position.left }}
            role="menu"
            aria-label="Link actions"
            tabIndex={-1}
            onKeyDown={handleLinkMenuKeyDown}
          >
            <span className="text-text-muted max-w-56 truncate px-2 text-xs">{linkMenu.href}</span>
            <button
              type="button"
              role="menuitem"
              title="Open link"
              aria-label="Open link"
              onClick={() => {
                window.open(linkMenu.href, "_blank", "noopener,noreferrer");
                setLinkMenu(null);
              }}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
            >
              <ExternalLink size={14} />
            </button>
            <button
              type="button"
              role="menuitem"
              title="Close"
              aria-label="Close"
              onClick={closeAndRestoreLinkMenu}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
            >
              <X size={14} />
            </button>
          </div>
        </>
      )}
      {commentDialog && (
        <EditorCommentDialog
          content={commentContent}
          error={commentError}
          saving={commentSaving}
          target={commentDialog}
          onCancel={closeSelectionCommentDialog}
          onChange={setCommentContent}
          onSubmit={() => void submitSelectionComment()}
        />
      )}
    </section>
  );
}
