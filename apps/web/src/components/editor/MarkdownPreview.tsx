import { markdownToHtml } from "@/components/editor/Editor";
import { dataClient, type LocalAssetContext, type SearchResultRecord } from "@/lib/dataClient";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { ExternalLink, X } from "lucide-react";
import { lazy, Suspense, useEffect, useState, type ComponentType } from "react";
import { useNavigate } from "react-router-dom";

interface MarkdownPreviewProps {
  content: string;
  assetContext?: LocalAssetContext;
}

interface PreviewLinkMenu {
  href: string;
  position: { top: number; left: number };
}

interface PreviewExcalidrawScene {
  filename: string;
  scene: {
    elements: readonly unknown[];
    appState?: Record<string, unknown>;
    files?: Record<string, unknown>;
  };
}

interface ExcalidrawComponentProps {
  initialData?: unknown;
  viewModeEnabled?: boolean;
}

const LazyPreviewExcalidraw = lazy(
  async (): Promise<{ default: ComponentType<ExcalidrawComponentProps> }> => {
    const module = await import("@excalidraw/excalidraw");
    return { default: module.Excalidraw as ComponentType<ExcalidrawComponentProps> };
  },
);

type MarkdownPreviewPart =
  | { type: "markdown"; content: string; key: string }
  | { type: "dataview"; query: string; key: string };

export function splitMarkdownDataviewBlocks(content: string): MarkdownPreviewPart[] {
  const parts: MarkdownPreviewPart[] = [];
  const lines = content.split("\n");
  const markdownBuffer: string[] = [];
  let partIndex = 0;

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
    if (/^```(?:dataview|dv)\s*$/i.test(line.trim())) {
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
    markdownBuffer.push(line);
  }

  flushMarkdown();
  return parts;
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
        .filter((item): item is DataviewBaseColumn =>
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

function parseDataviewComputedColumn(value: string): Extract<DataviewColumn, { id: string }> | undefined {
  const match = value.match(/^(.+?)\s*=\s*(.+)$/);
  if (!match) return undefined;
  const label = match[1].trim().slice(0, 40);
  const expression = normalizeDataviewComputedExpression(match[2]);
  if (!label || !expression) return undefined;
  const id = `computed:${label.toLowerCase().replace(/[^a-z0-9]+/g, "-")}:${expression}`;
  return { id, label, expression };
}

function normalizeDataviewComputedExpression(value: string): DataviewComputedExpression | undefined {
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
  if (normalized === "titlewords" || normalized === "words(title)" || normalized === "wordcount(title)") {
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
    return { id: `computed:sort:${directExpression}`, label: rawField, expression: directExpression };
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
      return compareDataviewSortValues(aValue, bValue) * direction || a.title.localeCompare(b.title);
    }
    if (sort.field === "updated") {
      const aTime = new Date(a.updated_at).getTime();
      const bTime = new Date(b.updated_at).getTime();
      const comparison =
        (Number.isNaN(aTime) ? 0 : aTime) - (Number.isNaN(bTime) ? 0 : bTime);
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

function readPreviewExcalidrawScene(value: string | null): PreviewExcalidrawScene["scene"] {
  if (!value) return { elements: [], appState: {}, files: {} };
  try {
    return normalizePreviewExcalidrawScene(JSON.parse(decodeURIComponent(value)));
  } catch {
    return { elements: [], appState: {}, files: {} };
  }
}

function parsePreviewExcalidrawSceneJson(value: string): PreviewExcalidrawScene["scene"] {
  try {
    return normalizePreviewExcalidrawScene(JSON.parse(value));
  } catch {
    return { elements: [], appState: {}, files: {} };
  }
}

function normalizePreviewExcalidrawScene(value: unknown): PreviewExcalidrawScene["scene"] {
  const parsed = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  return {
    elements: Array.isArray(parsed.elements) ? parsed.elements : [],
    appState:
      parsed.appState && typeof parsed.appState === "object"
        ? (parsed.appState as Record<string, unknown>)
        : {},
    files:
      parsed.files && typeof parsed.files === "object"
        ? (parsed.files as Record<string, unknown>)
        : {},
  };
}

function extractExcalidrawDirectiveFilename(line: string): string | null {
  const match = line.match(/!\[\[excalidraw:([^\]]+)\]\]/i);
  return match?.[1]?.trim() || null;
}

export function getExcalidrawDirectiveFilenames(content: string): string[] {
  const filenames = new Set<string>();
  for (const line of content.split("\n")) {
    const filename = extractExcalidrawDirectiveFilename(line);
    if (filename) filenames.add(filename);
  }
  return [...filenames];
}

export function applyExcalidrawSidecarScenes(
  content: string,
  scenes: ReadonlyMap<string, string>,
): string {
  if (scenes.size === 0) return content;
  const lines = content.split("\n");
  const next: string[] = [];

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    next.push(line);
    const filename = extractExcalidrawDirectiveFilename(line);
    if (!filename) continue;

    let lookahead = index + 1;
    while (lookahead < lines.length && lines[lookahead].trim() === "") {
      lookahead += 1;
    }
    if (/^```excalidraw-json\s*$/i.test(lines[lookahead]?.trim() ?? "")) continue;

    const scene = scenes.get(filename);
    if (!scene) continue;
    next.push("", "```excalidraw-json", scene, "```");
  }

  return next.join("\n");
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
      return Number.isNaN(updated.getTime()) ? "" : updated.toLocaleDateString(undefined, { weekday: "short" });
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
  const navigate = useNavigate();
  const config = parseDataviewBlockConfig(query);
  const [results, setResults] = useState<SearchResultRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let canceled = false;
    const trimmed = config.query.trim();
    if (trimmed.length < 2) {
      setResults([]);
      setLoading(false);
      setError("Add a Dataview query to show matching pages.");
      return;
    }

    setLoading(true);
    setError("");
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
      })
      .finally(() => {
        if (!canceled) setLoading(false);
      });

    return () => {
      canceled = true;
    };
  }, [config.limit, config.query]);

  const openResult = (result: SearchResultRecord) => {
    navigate(getDataviewResultPath(result));
  };
  const displayResults = sortDataviewResults(results, config.sort);
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
        return <span className="text-text-muted whitespace-nowrap">{formatDataviewDate(result.updated_at)}</span>;
      case "excerpt":
        return <span className="text-text-muted text-xs leading-5">{result.excerpt}</span>;
    }
  };

  return (
    <div className="border-border bg-surface rounded-card my-4 overflow-hidden border">
      <div className="border-border bg-surface-raised flex items-center justify-between gap-3 border-b px-3 py-2">
        <div>
          <p className="text-text text-sm font-medium">{config.name || "Dataview"}</p>
          <p className="text-text-muted mt-0.5 max-w-full truncate font-mono text-xs">
            {config.query || "Add a property or page query"}
          </p>
        </div>
        <span className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1">
          {loading ? "Running" : `${results.length} results`}
        </span>
      </div>

      {error ? (
        <p className="text-text-muted px-3 py-3 text-sm">{error}</p>
      ) : loading ? (
        <p className="text-text-muted px-3 py-3 text-sm">Running query...</p>
      ) : results.length === 0 ? (
        <p className="text-text-muted px-3 py-3 text-sm">No matching pages.</p>
      ) : config.view === "list" ? (
        <div className="divide-border divide-y">
          {groupedResults.map((group) => (
            <section key={group.key}>
              {config.group ? (
                <h4 className="bg-surface-raised/50 px-3 py-2 text-xs font-medium uppercase text-text-muted">
                  {group.label}
                </h4>
              ) : null}
              {group.results.map((result) => (
                <button
                  key={`${result.space_id}-${result.document_id}`}
                  type="button"
                  onClick={() => openResult(result)}
                  className="hover:bg-surface-raised/70 block w-full px-3 py-3 text-left transition-colors duration-100"
                >
                  <span className="text-text block text-sm font-medium">{result.title}</span>
                  <span className="text-text-muted mt-1 block text-xs">
                    {result.space_name} · {formatDataviewDate(result.updated_at)}
                  </span>
                  {result.excerpt ? (
                    <span className="text-text-muted mt-2 block text-xs leading-5">{result.excerpt}</span>
                  ) : null}
                </button>
              ))}
            </section>
          ))}
        </div>
      ) : config.view === "cards" ? (
        <div className="space-y-3 p-3">
          {groupedResults.map((group) => (
            <section key={group.key}>
              {config.group ? (
                <h4 className="mb-2 text-xs font-medium uppercase text-text-muted">{group.label}</h4>
              ) : null}
              <div className="grid gap-2 sm:grid-cols-2">
                {group.results.map((result) => (
                  <button
                    key={`${result.space_id}-${result.document_id}`}
                    type="button"
                    onClick={() => openResult(result)}
                    className="border-border hover:bg-surface-raised/70 rounded-card border p-3 text-left transition-colors duration-100"
                  >
                    <span className="text-text block text-sm font-medium">{result.title}</span>
                    <span className="text-text-muted mt-1 block text-xs">
                      {result.space_name} · {formatDataviewDate(result.updated_at)}
                    </span>
                    {result.excerpt ? (
                      <span className="text-text-muted mt-2 block text-xs leading-5">{result.excerpt}</span>
                    ) : null}
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
      ) : (
        <div className="overflow-x-auto">
          <table className="min-w-full text-left text-sm">
            <thead className="text-text-muted bg-surface-raised/60 text-xs uppercase">
              <tr>
                {config.columns.map((column) => (
                  <th key={dataviewColumnKey(column)} className="px-3 py-2 font-medium">
                    {renderColumnHeader(column)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {groupedResults.flatMap((group) => [
                config.group ? (
                  <tr key={`${group.key}-heading`} className="border-border border-t bg-surface-raised/40">
                    <td colSpan={config.columns.length} className="px-3 py-2 text-xs font-medium uppercase text-text-muted">
                      {group.label}
                    </td>
                  </tr>
                ) : null,
                ...group.results.map((result) => (
                  <tr
                    key={`${result.space_id}-${result.document_id}`}
                    className="border-border hover:bg-surface-raised/70 cursor-pointer border-t transition-colors duration-100"
                    onClick={() => openResult(result)}
                  >
                    {config.columns.map((column) => (
                      <td key={dataviewColumnKey(column)} className="px-3 py-2 align-top">
                        {renderColumn(result, column)}
                      </td>
                    ))}
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

export function MarkdownPreview({ content, assetContext = {} }: MarkdownPreviewProps) {
  const assetSpaceId = assetContext.space_id;
  const assetDocumentId = assetContext.document_id;
  const [linkMenu, setLinkMenu] = useState<PreviewLinkMenu | null>(null);
  const [excalidrawPreview, setExcalidrawPreview] = useState<PreviewExcalidrawScene | null>(null);
  const [excalidrawStatus, setExcalidrawStatus] = useState<string | null>(null);
  const [sidecarScenes, setSidecarScenes] = useState<Map<string, string>>(() => new Map());
  const hydratedContent = applyExcalidrawSidecarScenes(content, sidecarScenes);
  const parts = splitMarkdownDataviewBlocks(hydratedContent);

  useEffect(() => {
    if (!assetDocumentId) {
      setSidecarScenes(new Map());
      return;
    }

    const filenames = getExcalidrawDirectiveFilenames(content);
    if (filenames.length === 0) {
      setSidecarScenes(new Map());
      return;
    }

    let cancelled = false;
    setSidecarScenes(new Map());
    Promise.all(
      filenames.map(async (filename) => {
        try {
          const scene = await dataClient.readDocumentTextAsset(filename, {
            space_id: assetSpaceId,
            document_id: assetDocumentId,
          });
          return scene ? ([filename, scene] as const) : null;
        } catch {
          return null;
        }
      }),
    ).then((entries) => {
      if (cancelled) return;
      setSidecarScenes(
        new Map(entries.filter((entry): entry is readonly [string, string] => Boolean(entry))),
      );
    });

    return () => {
      cancelled = true;
    };
  }, [assetDocumentId, assetSpaceId, content]);

  const openLinkMenu = (anchor: HTMLAnchorElement) => {
    setLinkMenu({
      href: anchor.href || anchor.getAttribute("href") || "",
      position: getPreviewLinkPosition(anchor.getBoundingClientRect()),
    });
  };

  const openExcalidrawPreview = async (canvasElement: HTMLElement) => {
    const filename = canvasElement.getAttribute("filename") || "untitled.excalidraw";
    let scene = readPreviewExcalidrawScene(canvasElement.getAttribute("data-scene"));
    setExcalidrawStatus(null);
    if (assetContext.document_id) {
      try {
        const storedScene = await dataClient.readDocumentTextAsset(filename, assetContext);
        if (storedScene) scene = parsePreviewExcalidrawSceneJson(storedScene);
      } catch {
        setExcalidrawStatus("Could not read the drawing file. Showing the embedded fallback.");
      }
    }
    setExcalidrawPreview({
      filename,
      scene,
    });
  };

  return (
    <section
      className="markdown-preview min-h-[300px] px-0 py-1"
      aria-label="Markdown preview"
      onClick={(event) => {
        const target = event.target;
        if (!(target instanceof Element)) return;
        const excalidrawCanvas = target.closest("[data-excalidraw-canvas]");
        if (excalidrawCanvas instanceof HTMLElement) {
          event.preventDefault();
          void openExcalidrawPreview(excalidrawCanvas);
          return;
        }
        const anchor = target.closest("a[href]");
        if (!(anchor instanceof HTMLAnchorElement)) return;
        event.preventDefault();
        openLinkMenu(anchor);
      }}
      onKeyDown={(event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        const target = event.target;
        if (!(target instanceof Element)) return;
        const excalidrawCanvas = target.closest("[data-excalidraw-canvas]");
        if (excalidrawCanvas instanceof HTMLElement) {
          event.preventDefault();
          void openExcalidrawPreview(excalidrawCanvas);
        }
      }}
    >
      <div className="tiptap-editor">
        {parts.map((part) =>
          part.type === "markdown" ? (
            <div key={part.key} dangerouslySetInnerHTML={{ __html: markdownToHtml(part.content) }} />
          ) : (
            <DataviewBlock key={part.key} query={part.query} />
          ),
        )}
      </div>
      {excalidrawPreview && (
        <div
          className="fixed inset-0 z-[95] flex items-center justify-center bg-black/40 p-4"
          role="dialog"
          aria-modal="true"
          aria-label="View Excalidraw canvas"
        >
          <div className="flex h-[min(52rem,calc(100dvh-2rem))] w-[min(96rem,calc(100vw-2rem))] flex-col overflow-hidden rounded-card border border-border bg-surface shadow-card">
            <div className="flex shrink-0 items-center justify-between gap-3 border-b border-border px-3 py-2">
              <div className="min-w-0">
                <h2 className="text-sm font-medium text-text">Excalidraw canvas</h2>
                <p className="truncate text-xs text-text-muted">{excalidrawPreview.filename}</p>
              </div>
              <button
                type="button"
                onClick={() => setExcalidrawPreview(null)}
                className="rounded-button border border-border px-3 py-1.5 text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
              >
                Close
              </button>
            </div>
            {excalidrawStatus && (
              <div className="border-b border-border px-3 py-2 text-xs text-text-muted">
                {excalidrawStatus}
              </div>
            )}
            <div className="min-h-0 flex-1 bg-bg">
              <Suspense
                fallback={
                  <div className="flex h-full items-center justify-center text-sm text-text-muted">
                    Loading drawing…
                  </div>
                }
              >
                <LazyPreviewExcalidraw
                  initialData={excalidrawPreview.scene}
                  viewModeEnabled
                />
              </Suspense>
            </div>
          </div>
        </div>
      )}
      {linkMenu && (
        <>
          <button
            type="button"
            aria-label="Close link menu"
            className="fixed inset-0 z-[65] cursor-default bg-transparent"
            onClick={() => setLinkMenu(null)}
          />
          <div
            className="starlane-menu-surface fixed z-[70] flex max-w-[min(20rem,calc(100vw-1rem))] items-center gap-1 p-1"
            style={{ top: linkMenu.position.top, left: linkMenu.position.left }}
            aria-label="Link actions"
          >
            <span className="text-text-muted max-w-56 truncate px-2 text-xs">{linkMenu.href}</span>
            <button
              type="button"
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
              title="Close"
              aria-label="Close"
              onClick={() => setLinkMenu(null)}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
            >
              <X size={14} />
            </button>
          </div>
        </>
      )}
    </section>
  );
}
