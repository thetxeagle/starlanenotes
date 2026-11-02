import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  BookmarkPlus,
  CalendarDays,
  Check,
  ChevronDown,
  Clock3,
  Eraser,
  FileText,
  FilePenLine,
  Filter,
  Folder,
  Heading,
  Layers3,
  LockKeyhole,
  Paperclip,
  Plus,
  Search as SearchIcon,
  Star,
  Tag,
  Trash2,
  UserRound,
  X,
  type LucideIcon,
} from "lucide-react";
import {
  dataClient,
  type FolderRecord,
  type PropertySchemaRecord,
  type SearchFilters,
  type SearchResultRecord,
  type SpaceRecord,
  type TagRecord,
  type UserDirectoryRecord,
} from "@/lib/dataClient";
import {
  deleteSavedSearchView,
  readSavedSearchViews,
  saveSearchView,
  type SavedSearchView,
} from "@/lib/savedSearches";
import { formatSearchQueryInput, parseSearchQueryInput } from "@/lib/searchQuery";
import {
  filterStarredDocuments,
  readStarredDocuments,
  type StarredDocumentRecord,
} from "@/lib/starredDocuments";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";

interface SearchModalProps {
  open: boolean;
  onClose: () => void;
  preset?: SearchModalPreset;
  presentation?: "dialog" | "fullscreen";
}

export interface SearchModalPreset {
  nonce: number;
  query?: string;
  filters?: SearchFilters;
}

export interface WorkspaceSearchEventDetail {
  tag?: string;
  spaceId?: string;
}

/** Keep sidebar and property-search handoffs identical across every shell. */
export function getSearchModalPresetFromEvent(
  detail: WorkspaceSearchEventDetail | undefined,
  nonce = Date.now(),
): SearchModalPreset {
  return {
    nonce,
    filters: {
      tag: detail?.tag,
      space_id: detail?.spaceId,
    },
  };
}

export function getActiveSearchSpaceId(pathname: string): string | undefined {
  const match = /^\/spaces\/([^/]+)/.exec(pathname);
  return match ? decodeURIComponent(match[1]) : undefined;
}

export function getSearchResultDocumentPath(result: SearchResultRecord): string {
  const path = buildDocumentRoutePath(result.space_id, result.document_id);
  return result.is_draft && result.draft_id
    ? `${path}?draft=${encodeURIComponent(result.draft_id)}`
    : path;
}

export function getStarredSearchResultDocumentPath(result: StarredDocumentRecord): string {
  return buildDocumentRoutePath(result.spaceId, result.id);
}

export function getSearchResultOptionId(resultId: string, starred = false): string {
  return `search-${starred ? "starred-" : ""}result-${encodeURIComponent(resultId)}`;
}

export function isCurrentWorkspaceSearchRequest(
  requestId: number,
  currentRequestId: number,
): boolean {
  return requestId === currentRequestId;
}

export function getDefaultSavedSearchViewName(query: string, starredOnly: boolean): string {
  const trimmed = query.trim();
  if (trimmed) return trimmed.slice(0, 48);
  return starredOnly ? "Favorite pages" : "Saved search";
}

export function canSaveSearchViewName(name: string): boolean {
  return name.trim().length > 0;
}

export interface SearchQuerySuggestion {
  id: string;
  label: string;
  detail: string;
  insertion: string;
  complete: boolean;
  icon: LucideIcon;
}

export interface SearchSuggestionData {
  spaces?: SpaceRecord[];
  folders?: Array<FolderRecord & { spaceId: string; spaceName: string }>;
  tags?: TagRecord[];
  properties?: PropertySchemaRecord[];
  users?: UserDirectoryRecord[];
  now?: Date;
}

const QUERY_OPERATORS: SearchQuerySuggestion[] = [
  {
    id: "operator-dataview",
    label: "Advanced query",
    detail: "Use AND, OR, NOT, =, is not, *, or fuzzy ~",
    insertion: "dv:",
    complete: false,
    icon: Filter,
  },
  {
    id: "operator-space",
    label: "Space",
    detail: "Limit results to a workspace",
    insertion: "space:",
    complete: false,
    icon: Layers3,
  },
  {
    id: "operator-tag",
    label: "Tag",
    detail: "Find pages carrying a tag",
    insertion: "tag:",
    complete: false,
    icon: Tag,
  },
  {
    id: "operator-property",
    label: "Property",
    detail: "Match a property name or name=value",
    insertion: "property:",
    complete: false,
    icon: Filter,
  },
  {
    id: "operator-created",
    label: "Created date",
    detail: "Match an exact creation date",
    insertion: "created:",
    complete: false,
    icon: CalendarDays,
  },
  {
    id: "operator-updated",
    label: "Updated date",
    detail: "Match an exact modified date",
    insertion: "updated:",
    complete: false,
    icon: CalendarDays,
  },
  {
    id: "operator-starred",
    label: "Favorites",
    detail: "Show only favorite pages",
    insertion: "is:starred",
    complete: true,
    icon: Star,
  },
  {
    id: "operator-state",
    label: "Page state",
    detail: "Filter locked or unlocked pages",
    insertion: "is:",
    complete: false,
    icon: LockKeyhole,
  },
  {
    id: "operator-has",
    label: "Has",
    detail: "Require an attachment, cover, workflow, tag, or property",
    insertion: "has:",
    complete: false,
    icon: Paperclip,
  },
  {
    id: "operator-folder",
    label: "Folder",
    detail: "Match a folder name or identifier",
    insertion: "folder:",
    complete: false,
    icon: Folder,
  },
  {
    id: "operator-user",
    label: "User",
    detail: "Match the person who last updated a page",
    insertion: "user:",
    complete: false,
    icon: UserRound,
  },
  {
    id: "operator-title",
    label: "Title",
    detail: "Search page titles",
    insertion: "title:",
    complete: false,
    icon: Heading,
  },
];

const SEARCH_BUILDER_FILTER_ORDER = [
  "operator-tag",
  "operator-property",
  "operator-folder",
  "operator-user",
  "operator-created",
  "operator-updated",
  "operator-state",
  "operator-has",
  "operator-title",
  "operator-starred",
];

export function getSearchBuilderFilterOptions(): SearchQuerySuggestion[] {
  return QUERY_OPERATORS.filter(
    (option) => option.id !== "operator-dataview" && option.id !== "operator-space",
  ).sort(
    (left, right) =>
      SEARCH_BUILDER_FILTER_ORDER.indexOf(left.id) - SEARCH_BUILDER_FILTER_ORDER.indexOf(right.id),
  );
}

export function getSearchBuilderValueSuggestions(
  prefix: string,
  search: string,
  data: SearchSuggestionData = {},
): SearchQuerySuggestion[] {
  if (!prefix) return [];
  const normalizedPrefix = prefix.toLowerCase();
  return getSearchQuerySuggestions(`${prefix}${search}`, data).filter(
    (suggestion) =>
      suggestion.insertion.toLowerCase().startsWith(normalizedPrefix) &&
      (suggestion.insertion !== prefix || suggestion.complete),
  );
}

export function getActiveSearchToken(query: string): { value: string; start: number } {
  const match = /(?:^|\s)(\S*)$/.exec(query);
  return {
    value: match?.[1] ?? "",
    start: match ? query.length - (match[1]?.length ?? 0) : query.length,
  };
}

export function applySearchQuerySuggestion(
  query: string,
  suggestion: Pick<SearchQuerySuggestion, "insertion" | "complete">,
): string {
  const active = getActiveSearchToken(query);
  const prefix = query.slice(0, active.start);
  return `${prefix}${suggestion.insertion}${suggestion.complete ? " " : ""}`;
}

export function getSearchQuerySuggestions(
  query: string,
  data: SearchSuggestionData = {},
): SearchQuerySuggestion[] {
  const { value } = getActiveSearchToken(query);
  if (!value && query.trim()) return [];
  const lower = value.toLowerCase();
  const suffix = (prefix: string) => value.slice(prefix.length).toLowerCase();
  const matches = (candidate: string, needle: string) =>
    !needle || candidate.toLowerCase().includes(needle);

  if (lower === "space" || lower.startsWith("space:")) {
    const needle = lower === "space" ? "" : suffix("space:");
    const spaceSuggestions = (data.spaces ?? [])
      .filter((space) => matches(space.name, needle) || matches(space.slug, needle))
      .slice(0, 8)
      .map((space) => ({
        id: `space-${space.id}`,
        label: space.name,
        detail: `Space · ${space.visibility ?? "available"}`,
        insertion: `space:${formatSuggestionValue(space.name)}`,
        complete: true,
        icon: Layers3,
      }));
    if (spaceSuggestions.length > 0) return spaceSuggestions;
  }

  if (lower === "tag" || lower.startsWith("tag:")) {
    const needle = lower === "tag" ? "" : suffix("tag:");
    const tagSuggestions = uniqueByName(data.tags ?? [])
      .filter((tag) => matches(tag.name, needle))
      .slice(0, 8)
      .map((tag) => ({
        id: `tag-${tag.id}`,
        label: tag.name,
        detail: "Tag",
        insertion: `tag:${formatSuggestionValue(tag.name)}`,
        complete: true,
        icon: Tag,
      }));
    if (tagSuggestions.length > 0) return tagSuggestions;
  }

  if (lower === "property" || lower.startsWith("property:")) {
    const needle = lower === "property" ? "" : suffix("property:");
    const propertySuggestions = uniqueByName(data.properties ?? [])
      .filter((property) => matches(property.name, needle))
      .slice(0, 8)
      .map((property) => ({
        id: `property-${property.id}`,
        label: property.name,
        detail: `${property.property_type} property · add =value to narrow`,
        insertion: `property:${formatSuggestionValue(property.name)}`,
        complete: true,
        icon: Filter,
      }));
    if (propertySuggestions.length > 0) return propertySuggestions;
  }

  if (lower === "folder" || lower.startsWith("folder:")) {
    const needle = lower === "folder" ? "" : suffix("folder:");
    const folderSuggestions = uniqueByName(data.folders ?? [])
      .filter((folder) => matches(folder.name, needle) || matches(folder.spaceName, needle))
      .slice(0, 8)
      .map((folder) => ({
        id: `folder-${folder.spaceId}-${folder.id}`,
        label: folder.name,
        detail: `Folder · ${folder.spaceName}`,
        insertion: `folder:${formatSuggestionValue(folder.name)}`,
        complete: true,
        icon: Folder,
      }));
    if (folderSuggestions.length > 0) return folderSuggestions;
  }

  if (lower === "user" || lower.startsWith("user:")) {
    const needle = lower === "user" ? "" : suffix("user:");
    const userSuggestions = (data.users ?? [])
      .filter((user) => matches(user.display_name, needle) || matches(user.email, needle))
      .slice(0, 8)
      .map((user) => ({
        id: `user-${user.id}`,
        label: user.display_name,
        detail: user.email,
        insertion: `user:${formatSuggestionValue(user.display_name)}`,
        complete: true,
        icon: UserRound,
      }));
    if (userSuggestions.length > 0) return userSuggestions;
  }

  if (lower.startsWith("is:")) {
    const needle = suffix("is:");
    return [
      ["starred", "Favorite pages", Star],
      ["draft", "Private drafts", FilePenLine],
      ["locked", "Locked pages", LockKeyhole],
      ["unlocked", "Unlocked pages", LockKeyhole],
    ]
      .filter(([value]) => matches(String(value), needle))
      .map(([state, detail, icon]) => ({
        id: `state-${state}`,
        label: String(state),
        detail: String(detail),
        insertion: `is:${state}`,
        complete: true,
        icon: icon as SearchQuerySuggestion["icon"],
      }));
  }

  if (lower.startsWith("has:")) {
    const needle = suffix("has:");
    return ["attachment", "cover", "workflow", "tag", "property"]
      .filter((value) => matches(value, needle))
      .map((value) => ({
        id: `has-${value}`,
        label: value,
        detail: `Documents with ${value}`,
        insertion: `has:${value}`,
        complete: true,
        icon: value === "attachment" ? Paperclip : Filter,
      }));
  }

  if (lower.startsWith("created:") || lower.startsWith("updated:")) {
    const prefix = lower.startsWith("created:") ? "created:" : "updated:";
    const needle = suffix(prefix);
    return getDateSuggestions(prefix, data.now ?? new Date()).filter((item) =>
      matches(item.insertion.slice(prefix.length), needle),
    );
  }

  const operatorNeedle = lower.replace(/:.*$/, "");
  return QUERY_OPERATORS.filter((operator) => {
    const searchable = `${operator.label} ${operator.detail} ${operator.insertion}`;
    return matches(searchable, operatorNeedle);
  }).slice(0, 10);
}

export function resolveSearchSpaceFilter(
  filters: SearchFilters,
  spaces: SpaceRecord[] = [],
): SearchFilters {
  const requestedSpace = filters.space_id?.trim();
  if (!requestedSpace) return filters;
  const normalized = requestedSpace.toLowerCase();
  const matchingSpace = spaces.find(
    (space) =>
      space.id.toLowerCase() === normalized ||
      space.name.trim().toLowerCase() === normalized ||
      space.slug.trim().toLowerCase() === normalized,
  );
  return matchingSpace ? { ...filters, space_id: matchingSpace.id } : filters;
}

export function formatSearchSpaceNames(query: string, spaces: SpaceRecord[] = []): string {
  const visual = splitVisualSearchQuery(query);
  const spaceToken = visual.tokens.find((token) => token.raw.toLowerCase().startsWith("space:"));
  const requestedSpace = spaceToken?.raw
    .slice(spaceToken.raw.indexOf(":") + 1)
    .replace(/^"|"$/g, "")
    .trim();
  if (!requestedSpace) return query;
  const matchingSpace = spaces.find(
    (space) => space.id.toLowerCase() === requestedSpace.toLowerCase(),
  );
  if (!matchingSpace) return query;
  return composeVisualSearchQuery(
    visual.tokens.map((token) =>
      token === spaceToken
        ? {
            ...token,
            raw: `space:${formatSuggestionValue(matchingSpace.name)}`,
            label: `space: ${matchingSpace.name}`,
          }
        : token,
    ),
    visual.text,
    visual.advanced,
  );
}

interface SearchSaveViewPanelProps {
  name: string;
  onNameChange: (name: string) => void;
  onCancel: () => void;
  onSave: () => void;
}

export function SearchSaveViewPanel({
  name,
  onNameChange,
  onCancel,
  onSave,
}: SearchSaveViewPanelProps) {
  return (
    <section
      aria-labelledby="save-search-view-title"
      className="border-border bg-surface-raised border-b px-3 py-3"
    >
      <form
        className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]"
        onSubmit={(event) => {
          event.preventDefault();
          if (canSaveSearchViewName(name)) onSave();
        }}
      >
        <div className="min-w-0">
          <h2 id="save-search-view-title" className="text-text text-xs font-medium">
            Save search view
          </h2>
          <Input
            id="save-search-view-name"
            value={name}
            onChange={(event) => onNameChange(event.target.value)}
            className="bg-surface mt-1 h-8 text-sm"
            placeholder="View name"
            aria-label="Saved search view name"
            autoComplete="off"
          />
        </div>
        <div className="flex items-end justify-end gap-2">
          <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSaveSearchViewName(name)}>
            Save
          </Button>
        </div>
      </form>
    </section>
  );
}

export function SearchFailureState({ onRetry }: { onRetry: () => void }) {
  return (
    <div
      role="alert"
      className="flex min-h-48 flex-col items-center justify-center gap-3 px-6 text-center"
    >
      <p className="text-text text-sm font-medium">Search couldn&apos;t be completed.</p>
      <p className="text-text-muted text-xs">Check your connection, then try again.</p>
      <Button type="button" variant="secondary" size="sm" onClick={onRetry}>
        Retry search
      </Button>
    </div>
  );
}

function SearchSuggestionPanel({
  suggestions,
  selectedIndex,
  onSelect,
}: {
  suggestions: SearchQuerySuggestion[];
  selectedIndex: number;
  onSelect: (suggestion: SearchQuerySuggestion) => void;
}) {
  if (suggestions.length === 0) return null;

  return (
    <section aria-labelledby="search-query-builder-title" className="border-border border-b p-1.5">
      <div className="flex items-center justify-between gap-3 px-2 py-1">
        <div>
          <h2 id="search-query-builder-title" className="text-text text-xs font-medium">
            Build your search
          </h2>
          <p className="text-text-muted text-[11px]">
            Choose a filter, then keep typing. Quotes preserve multi-word values.
          </p>
        </div>
        <span className="text-text-muted hidden text-[10px] sm:block">
          <kbd className="border-border rounded border px-1 font-mono">tab</kbd> complete
        </span>
      </div>
      <div
        id="search-query-suggestions"
        role="listbox"
        aria-label="Search query suggestions"
        className="grid max-h-80 gap-0.5 overflow-y-auto"
      >
        {suggestions.map((suggestion, index) => {
          const Icon = suggestion.icon;
          return (
            <button
              key={suggestion.id}
              type="button"
              id={`search-suggestion-${suggestion.id}`}
              role="option"
              aria-selected={index === selectedIndex}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => onSelect(suggestion)}
              className={`rounded-button flex min-h-11 w-full items-center gap-3 px-2.5 py-2 text-left transition-colors duration-100 ${
                index === selectedIndex
                  ? "bg-surface-raised text-text"
                  : "text-text-muted hover:bg-surface-raised hover:text-text"
              }`}
            >
              <span className="border-border bg-surface rounded-input flex size-8 shrink-0 items-center justify-center border">
                <Icon size={15} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium">{suggestion.label}</span>
                <span className="text-text-muted block truncate text-xs">{suggestion.detail}</span>
              </span>
              <code className="border-border bg-surface text-text-muted hidden max-w-48 truncate rounded border px-1.5 py-0.5 text-[11px] sm:block">
                {suggestion.insertion}
              </code>
            </button>
          );
        })}
      </div>
    </section>
  );
}

export interface VisualSearchToken {
  id: string;
  raw: string;
  kind: "filter" | "operator" | "group";
  label: string;
}

export interface SearchBuilderOperatorOption {
  value: "AND" | "OR" | "NOT";
  label: string;
  detail: string;
}

const SEARCH_BUILDER_OPERATOR_OPTIONS: SearchBuilderOperatorOption[] = [
  { value: "AND", label: "AND", detail: "all conditions" },
  { value: "OR", label: "OR", detail: "any condition" },
  { value: "NOT", label: "NOT", detail: "exclude the next condition" },
];

const MULTI_VALUE_FILTER_PREFIXES = new Set(["tag:", "folder:", "user:"]);

export function getSearchBuilderOperatorOptions(): SearchBuilderOperatorOption[] {
  return SEARCH_BUILDER_OPERATOR_OPTIONS;
}

const VISUAL_FILTER_PREFIXES = [
  "space:",
  "tag:",
  "property:",
  "folder:",
  "user:",
  "author:",
  "title:",
  "created:",
  "updated:",
  "has:",
  "is:",
] as const;

export function splitVisualSearchQuery(query: string): {
  tokens: VisualSearchToken[];
  text: string;
  advanced: boolean;
} {
  const trimmed = query.trim();
  const advanced = /^(?:dv:|dataview:)\s*/i.test(trimmed);
  const source = trimmed.replace(/^(?:dv:|dataview:)\s*/i, "");
  const parts = source.match(/\((?:[^"()]|"[^"]*")+\)|\S+:"[^"]*"|"[^"]+"|\S+/g) ?? [];
  const tokens: VisualSearchToken[] = [];
  const text: string[] = [];
  parts.forEach((raw, index) => {
    const group = parseVisualMultiValueGroup(raw);
    if (group) {
      tokens.push({ id: `${index}-${raw}`, raw, kind: "group", label: group.label });
      return;
    }
    const normalized = raw.toLowerCase();
    if (["and", "or", "not"].includes(normalized)) {
      tokens.push({
        id: `${index}-${raw}`,
        raw: raw.toUpperCase(),
        kind: "operator",
        label: raw.toUpperCase(),
      });
      return;
    }
    const prefix = VISUAL_FILTER_PREFIXES.find((candidate) => normalized.startsWith(candidate));
    if (prefix) {
      const value = raw.slice(raw.indexOf(":") + 1).replace(/^"|"$/g, "");
      const name = prefix.slice(0, -1);
      tokens.push({
        id: `${index}-${raw}`,
        raw,
        kind: "filter",
        label: `${name === "author" ? "user" : name}: ${value}`,
      });
      return;
    }
    text.push(raw.replace(/^"|"$/g, ""));
  });
  return { tokens, text: text.join(" "), advanced };
}

export function composeVisualSearchQuery(
  tokens: VisualSearchToken[],
  text: string,
  advanced = false,
): string {
  const body = [...tokens.map((token) => token.raw), text.trim()].filter(Boolean).join(" ");
  const requiresAdvanced =
    advanced ||
    tokens.some(
      (token) =>
        token.kind === "group" ||
        (token.kind === "operator" && ["OR", "NOT"].includes(token.raw.toUpperCase())),
    );
  return requiresAdvanced && body ? `dv:${body}` : body;
}

export function buildVisualMultiValueFilter(
  prefix: string,
  insertions: string[],
): VisualSearchToken {
  const normalizedPrefix = prefix.toLowerCase();
  const values = insertions.map((insertion) =>
    insertion.slice(insertion.indexOf(":") + 1).replace(/^"|"$/g, ""),
  );
  return {
    id: `${Date.now()}-${normalizedPrefix}${values.join("-")}`,
    raw: `(${insertions.join(" OR ")})`,
    kind: "group",
    label: `${normalizedPrefix.slice(0, -1)}: ${values.join(", ")}`,
  };
}

function parseVisualMultiValueGroup(raw: string): { label: string } | null {
  if (!raw.startsWith("(") || !raw.endsWith(")")) return null;
  const parts = raw.slice(1, -1).match(/\S+:"[^"]*"|\S+/g) ?? [];
  if (parts.length < 3 || parts.length % 2 === 0) return null;
  const filters = parts.filter((_, index) => index % 2 === 0);
  const operators = parts.filter((_, index) => index % 2 === 1);
  const firstPrefix = filters[0]?.slice(0, filters[0].indexOf(":") + 1).toLowerCase();
  if (
    !MULTI_VALUE_FILTER_PREFIXES.has(firstPrefix) ||
    operators.some((operator) => operator.toUpperCase() !== "OR") ||
    filters.some((filter) => filter.slice(0, filter.indexOf(":") + 1).toLowerCase() !== firstPrefix)
  ) {
    return null;
  }
  const values = filters.map((filter) =>
    filter.slice(filter.indexOf(":") + 1).replace(/^"|"$/g, ""),
  );
  return { label: `${firstPrefix.slice(0, -1)}: ${values.join(", ")}` };
}

function formatSearchResultDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  const now = new Date();
  const elapsed = now.getTime() - date.getTime();
  if (elapsed >= 0 && elapsed < 60 * 60 * 1000) {
    return `${Math.max(1, Math.floor(elapsed / 60000))}m ago`;
  }
  if (elapsed >= 0 && elapsed < 24 * 60 * 60 * 1000) {
    return `${Math.floor(elapsed / 3600000)}h ago`;
  }
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function SearchModal({ open, onClose, preset, presentation = "dialog" }: SearchModalProps) {
  const navigate = useNavigate();
  const modalRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const searchWasOpenRef = useRef(false);
  const initializedPresetNonceRef = useRef<number | undefined>(undefined);
  const [query, setQuery] = useState("");
  const [filters, setFilters] = useState<SearchFilters>({});
  const [results, setResults] = useState<SearchResultRecord[]>([]);
  const [starredOnly, setStarredOnly] = useState(false);
  const [starredDocuments, setStarredDocuments] = useState<StarredDocumentRecord[]>(() =>
    readStarredDocuments(),
  );
  const [savedSearches, setSavedSearches] = useState<SavedSearchView[]>(() =>
    readSavedSearchViews(),
  );
  const [saveSearchOpen, setSaveSearchOpen] = useState(false);
  const [saveSearchName, setSaveSearchName] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [suggestionIndex, setSuggestionIndex] = useState(0);
  const [suggestionData, setSuggestionData] = useState<SearchSuggestionData>({});
  const [advancedSearchOpen, setAdvancedSearchOpen] = useState(() =>
    Boolean(preset?.query && splitVisualSearchQuery(preset.query).advanced),
  );
  const [builderOpen, setBuilderOpen] = useState<"filter" | "operator" | null>(null);
  const [builderPrefix, setBuilderPrefix] = useState("");
  const [builderSearch, setBuilderSearch] = useState("");
  const [builderSelectedValues, setBuilderSelectedValues] = useState<string[]>([]);
  const [editingTokenId, setEditingTokenId] = useState<string | null>(null);
  const [editingTokenValue, setEditingTokenValue] = useState("");
  const [loading, setLoading] = useState(false);
  const [searchError, setSearchError] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const searchRequestIdRef = useRef(0);
  const suggestionRequestIdRef = useRef(0);
  const spacesRef = useRef<SpaceRecord[]>([]);
  const parsedQuery = parseSearchQueryInput(query);
  const activeStarredOnly = starredOnly || parsedQuery.starredOnly;
  const starredResults = activeStarredOnly
    ? filterStarredDocuments(starredDocuments, parsedQuery.query)
    : [];
  const visualQuery = useMemo(() => splitVisualSearchQuery(query), [query]);
  const suggestions = useMemo(
    () =>
      query.trim() && !builderOpen
        ? getSearchQuerySuggestions(query, suggestionData).filter(
            (suggestion) =>
              suggestion.id !== "operator-dataview" || advancedSearchOpen || visualQuery.advanced,
          )
        : [],
    [advancedSearchOpen, builderOpen, query, suggestionData, visualQuery.advanced],
  );

  useEffect(() => {
    if (visualQuery.advanced) setAdvancedSearchOpen(true);
  }, [visualQuery.advanced]);

  const updateQuery = useCallback((nextQuery: string) => setQuery(nextQuery), []);

  // Focus input when modal opens
  useEffect(() => {
    const opening = open && !searchWasOpenRef.current;
    const presetChanged =
      open && preset !== undefined && preset.nonce !== initializedPresetNonceRef.current;

    if (opening || presetChanged) {
      if (opening) {
        previousFocusRef.current = document.activeElement as HTMLElement | null;
      }
      const activeSpaceId =
        typeof window === "undefined"
          ? undefined
          : getActiveSearchSpaceId(window.location.pathname);
      const initialQuery = formatSearchQueryInput(preset?.query ?? "", {
        ...preset?.filters,
        space_id: preset?.filters?.space_id ?? activeSpaceId,
      });
      setQuery(initialQuery);
      setFilters({});
      setResults([]);
      setLoading(false);
      setSearchError(false);
      setStarredOnly(false);
      setStarredDocuments(readStarredDocuments());
      setSavedSearches(readSavedSearchViews());
      setSaveSearchOpen(false);
      setSaveSearchName("");
      setSelectedIndex(0);
      setSuggestionIndex(0);
      setBuilderOpen(null);
      setAdvancedSearchOpen(splitVisualSearchQuery(initialQuery).advanced);
      setBuilderPrefix("");
      setBuilderSearch("");
      setBuilderSelectedValues([]);
      setEditingTokenId(null);
      setTimeout(() => inputRef.current?.focus(), 50);
    } else if (!open && searchWasOpenRef.current) {
      previousFocusRef.current?.focus();
    }

    searchWasOpenRef.current = open;
    initializedPresetNonceRef.current = preset?.nonce;
  }, [open, preset]);

  useEffect(() => {
    if (!open) return;
    const refreshStarredDocuments = () => setStarredDocuments(readStarredDocuments());
    window.addEventListener("starlane:starred-documents-changed", refreshStarredDocuments);
    return () =>
      window.removeEventListener("starlane:starred-documents-changed", refreshStarredDocuments);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    return () => {
      searchRequestIdRef.current += 1;
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
        debounceRef.current = undefined;
      }
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const requestId = ++suggestionRequestIdRef.current;
    let cancelled = false;

    const canPublish = () =>
      !cancelled && isCurrentWorkspaceSearchRequest(requestId, suggestionRequestIdRef.current);

    void dataClient
      .listSpaces()
      .then((spaces) => {
        if (canPublish()) {
          spacesRef.current = spaces;
          setSuggestionData({ spaces });
          void Promise.all([
            Promise.all(
              spaces.map(async (space) =>
                (await dataClient.listFolders(space.id).catch(() => [])).map((folder) => ({
                  ...folder,
                  spaceId: space.id,
                  spaceName: space.name,
                })),
              ),
            ),
            Promise.all(spaces.map((space) => dataClient.listTags(space.id).catch(() => []))),
            Promise.all(
              spaces.map((space) => dataClient.listPropertySchemas(space.id).catch(() => [])),
            ),
            dataClient.listUserDirectory().catch(() => []),
          ]).then(([folders, tags, properties, users]) => {
            if (canPublish()) {
              setSuggestionData({
                spaces,
                folders: folders.flat(),
                tags: tags.flat(),
                properties: properties.flat(),
                users,
              });
            }
          });
        }
      })
      .catch(() => {
        if (canPublish()) {
          spacesRef.current = [];
          setSuggestionData({});
        }
      });

    return () => {
      cancelled = true;
      if (suggestionRequestIdRef.current === requestId) {
        suggestionRequestIdRef.current += 1;
      }
    };
  }, [open]);

  // Focus trap while modal is open
  useEffect(() => {
    if (!open) return;

    const handler = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const root = modalRef.current;
      if (!root) return;

      const focusables = root.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      );
      if (focusables.length === 0) return;

      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement as HTMLElement | null;

      if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  // Debounced search
  const doSearch = useCallback((q: string, nextFilters: SearchFilters = {}) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = undefined;
    const requestId = ++searchRequestIdRef.current;
    const canPublish = () => isCurrentWorkspaceSearchRequest(requestId, searchRequestIdRef.current);
    setSearchError(false);
    const dataviewQuery = extractDataviewQuery(q);
    if (dataviewQuery !== null) {
      if (dataviewQuery.length < 2) {
        setLoading(false);
        setResults([]);
        return;
      }
      debounceRef.current = setTimeout(async () => {
        if (!canPublish()) return;
        setLoading(true);
        try {
          const data = await dataClient.queryDataview(dataviewQuery, 50);
          if (!canPublish()) return;
          setResults(data);
          setSelectedIndex(0);
        } catch {
          if (!canPublish()) return;
          setResults([]);
          setSearchError(true);
        } finally {
          if (canPublish()) setLoading(false);
        }
      }, 250);
      return;
    }

    const parsed = parseSearchQueryInput(q);
    const effectiveFilters = resolveSearchSpaceFilter(
      { ...parsed.filters, ...nextFilters },
      spacesRef.current,
    );

    if (parsed.starredOnly) {
      setLoading(false);
      setResults([]);
      setSelectedIndex(0);
      return;
    }

    const hasEffectiveFilters = Object.values(effectiveFilters).some((value) =>
      Boolean(value?.trim()),
    );
    if (parsed.query.length < 2 && !hasEffectiveFilters) {
      setLoading(false);
      setResults([]);
      return;
    }

    debounceRef.current = setTimeout(async () => {
      if (!canPublish()) return;
      setLoading(true);
      try {
        const data = await dataClient.searchDocuments(parsed.query, 50, effectiveFilters);
        if (!canPublish()) return;
        setResults(data);
        setSelectedIndex(0);
      } catch {
        if (!canPublish()) return;
        setResults([]);
        setSearchError(true);
      } finally {
        if (canPublish()) setLoading(false);
      }
    }, 250);
  }, []);

  useEffect(() => {
    const spaces = suggestionData.spaces;
    if (!open || !spaces) return;
    const readableQuery = formatSearchSpaceNames(query, spaces);
    if (readableQuery === query) return;
    updateQuery(readableQuery);
    doSearch(readableQuery, {});
  }, [doSearch, open, query, suggestionData.spaces, updateQuery]);

  useEffect(() => {
    if (!open || !preset) return;
    const nextQuery = formatSearchQueryInput(preset.query ?? "", preset.filters);
    updateQuery(nextQuery);
    setFilters({});
    setStarredOnly(false);
    doSearch(nextQuery, {});
  }, [doSearch, open, preset, updateQuery]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    const activeResultCount = activeStarredOnly ? starredResults.length : results.length;
    if (suggestions.length > 0 && e.key === "ArrowDown") {
      e.preventDefault();
      setSuggestionIndex((index) => Math.min(index + 1, suggestions.length - 1));
      return;
    }
    if (suggestions.length > 0 && e.key === "ArrowUp") {
      e.preventDefault();
      setSuggestionIndex((index) => Math.max(index - 1, 0));
      return;
    }
    if ((e.key === "Enter" || e.key === "Tab") && suggestions[suggestionIndex]) {
      e.preventDefault();
      applySuggestion(suggestions[suggestionIndex]);
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, Math.max(0, activeResultCount - 1)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && activeStarredOnly && starredResults[selectedIndex]) {
      e.preventDefault();
      const r = starredResults[selectedIndex];
      navigate(getStarredSearchResultDocumentPath(r));
      onClose();
    } else if (e.key === "Enter" && results[selectedIndex]) {
      e.preventDefault();
      const r = results[selectedIndex];
      navigate(getSearchResultDocumentPath(r));
      onClose();
    } else if (e.key === "Escape") {
      onClose();
    }
  };

  const applySuggestion = (suggestion: SearchQuerySuggestion) => {
    const nextQuery = applySearchQuerySuggestion(query, suggestion);
    updateQuery(nextQuery);
    setSuggestionIndex(0);
    setSelectedIndex(0);
    setStarredOnly(false);
    doSearch(nextQuery, {});
    window.requestAnimationFrame(() => inputRef.current?.focus());
  };

  const openSaveSearchDialog = () => {
    setSaveSearchName(getDefaultSavedSearchViewName(query, activeStarredOnly));
    setSaveSearchOpen(true);
  };

  const closeSaveSearchDialog = () => {
    setSaveSearchOpen(false);
    setSaveSearchName("");
  };

  const clearSearch = () => {
    updateQuery("");
    setFilters({});
    setResults([]);
    setSearchError(false);
    setStarredOnly(false);
    setSelectedIndex(0);
    setSuggestionIndex(0);
    setBuilderOpen(null);
    setAdvancedSearchOpen(false);
    setBuilderPrefix("");
    setBuilderSearch("");
    setBuilderSelectedValues([]);
    window.requestAnimationFrame(() => inputRef.current?.focus());
  };

  const saveCurrentSearch = () => {
    if (!canSaveSearchViewName(saveSearchName)) return;
    setSavedSearches(saveSearchView(saveSearchName.trim(), { query, filters, starredOnly }));
    closeSaveSearchDialog();
  };

  const applySavedSearch = (view: SavedSearchView) => {
    const nextQuery = formatSearchQueryInput(view.query, view.filters, view.starredOnly);
    updateQuery(nextQuery);
    setFilters({});
    setStarredOnly(false);
    setSelectedIndex(0);
    setResults([]);
    doSearch(nextQuery, {});
  };

  const commitVisualQuery = (tokens: VisualSearchToken[], text = visualQuery.text) => {
    const nextQuery = composeVisualSearchQuery(tokens, text, visualQuery.advanced);
    updateQuery(nextQuery);
    setSelectedIndex(0);
    setStarredOnly(false);
    doSearch(nextQuery, {});
  };

  const removeVisualToken = (tokenId: string) => {
    commitVisualQuery(visualQuery.tokens.filter((token) => token.id !== tokenId));
    setEditingTokenId(null);
  };

  const saveEditedToken = (token: VisualSearchToken) => {
    const value = editingTokenValue.trim();
    if (!value) {
      removeVisualToken(token.id);
      return;
    }
    const prefix = token.raw.slice(0, token.raw.indexOf(":") + 1);
    const raw =
      token.kind === "operator" ? value.toUpperCase() : `${prefix}${formatSuggestionValue(value)}`;
    commitVisualQuery(
      visualQuery.tokens.map((item) =>
        item.id === token.id
          ? {
              ...item,
              raw,
              label: token.kind === "operator" ? raw : `${prefix.slice(0, -1)}: ${value}`,
            }
          : item,
      ),
    );
    setEditingTokenId(null);
  };

  const addVisualToken = (raw: string) => {
    const parsed = splitVisualSearchQuery(raw).tokens[0];
    if (!parsed) return;
    const tokens = raw.toLowerCase().startsWith("space:")
      ? visualQuery.tokens.filter((token) => !token.raw.toLowerCase().startsWith("space:"))
      : visualQuery.tokens;
    commitVisualQuery([...tokens, { ...parsed, id: `${Date.now()}-${raw}` }]);
    setBuilderOpen(null);
    setBuilderPrefix("");
    setBuilderSearch("");
    setBuilderSelectedValues([]);
    window.requestAnimationFrame(() => inputRef.current?.focus());
  };

  const addVisualOperator = (operator: SearchBuilderOperatorOption["value"]) => {
    addVisualToken(operator);
  };

  const multiValueFilter = MULTI_VALUE_FILTER_PREFIXES.has(builderPrefix.toLowerCase());
  const hasVisualCondition = visualQuery.tokens.some(
    (token) => token.kind === "filter" || token.kind === "group",
  );

  const toggleBuilderValue = (insertion: string) => {
    setBuilderSelectedValues((selected) =>
      selected.includes(insertion)
        ? selected.filter((value) => value !== insertion)
        : [...selected, insertion],
    );
  };

  const addSelectedBuilderValues = () => {
    if (builderSelectedValues.length === 0) return;
    if (builderSelectedValues.length === 1) {
      addVisualToken(builderSelectedValues[0]);
      return;
    }
    const group = buildVisualMultiValueFilter(builderPrefix, builderSelectedValues);
    commitVisualQuery([...visualQuery.tokens, group]);
    setBuilderOpen(null);
    setBuilderPrefix("");
    setBuilderSearch("");
    setBuilderSelectedValues([]);
    window.requestAnimationFrame(() => inputRef.current?.focus());
  };

  const builderSuggestions = getSearchBuilderValueSuggestions(
    builderPrefix,
    builderSearch,
    suggestionData,
  );

  const selectBuilderFilter = (insertion: string) => {
    if (insertion === "is:starred") {
      addVisualToken(insertion);
      return;
    }
    setBuilderPrefix(insertion);
    setBuilderSearch("");
    setBuilderSelectedValues([]);
  };

  const handleBuilderMenuKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const items = Array.from(
      event.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]:not([disabled])'),
    );
    if (items.length === 0) return;
    event.preventDefault();
    const activeIndex = items.indexOf(document.activeElement as HTMLElement);
    const nextIndex =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? items.length - 1
          : event.key === "ArrowDown"
            ? (activeIndex + 1 + items.length) % items.length
            : (activeIndex - 1 + items.length) % items.length;
    items[nextIndex].focus();
  };

  if (!open) return null;

  const saveSearchPanel = saveSearchOpen ? (
    <SearchSaveViewPanel
      name={saveSearchName}
      onNameChange={setSaveSearchName}
      onCancel={closeSaveSearchDialog}
      onSave={saveCurrentSearch}
    />
  ) : null;

  const filterOptions = getSearchBuilderFilterOptions();
  const spaceToken = visualQuery.tokens.find((token) =>
    token.raw.toLowerCase().startsWith("space:"),
  );
  const queryTokens = visualQuery.tokens.filter((token) => token !== spaceToken);
  const activeResultCount = activeStarredOnly ? starredResults.length : results.length;
  const spacePickerOpen = builderOpen === "filter" && builderPrefix === "space:";
  const filterPickerOpen = builderOpen === "filter" && builderPrefix !== "space:";

  const closeBuilder = () => {
    setBuilderOpen(null);
    setBuilderPrefix("");
    setBuilderSearch("");
    setBuilderSelectedValues([]);
  };

  const filterValuePicker = (
    <div>
      <div className="border-border flex items-center gap-2 border-b p-2">
        <SearchIcon size={15} className="text-text-muted" />
        <Input
          autoFocus
          value={builderSearch}
          onChange={(event) => setBuilderSearch(event.target.value)}
          className="text-text placeholder:text-text-muted h-8 min-w-0 flex-1 border-0 bg-transparent p-0 text-sm shadow-none focus:ring-0 focus:ring-offset-0 focus-visible:ring-0 focus-visible:ring-offset-0"
          placeholder="Search values…"
          aria-label="Search filter values"
        />
        {!spacePickerOpen && (
          <button
            type="button"
            onClick={() => setBuilderPrefix("")}
            className="text-text-muted hover:text-text"
            aria-label="Back to filter types"
          >
            <X size={15} />
          </button>
        )}
      </div>
      <div role="listbox" aria-label="Filter values" className="mt-1 max-h-64 overflow-y-auto">
        {builderSuggestions.map((suggestion) => {
          const Icon = suggestion.icon;
          const selected = builderSelectedValues.includes(suggestion.insertion);
          return (
            <button
              key={suggestion.id}
              type="button"
              role="option"
              aria-selected={selected}
              onClick={() =>
                multiValueFilter
                  ? toggleBuilderValue(suggestion.insertion)
                  : addVisualToken(suggestion.insertion)
              }
              className="text-text-muted hover:bg-surface hover:text-text rounded-input flex min-h-10 w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-100"
            >
              <Icon size={16} className="text-primary shrink-0" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm">{suggestion.label}</span>
                <span className="block truncate text-xs">{suggestion.detail}</span>
              </span>
              {multiValueFilter && selected && <Check size={15} className="text-primary" />}
            </button>
          );
        })}
        {builderSearch.trim() && builderSuggestions.length === 0 && (
          <button
            type="button"
            onClick={() => {
              const insertion = `${builderPrefix}${formatSuggestionValue(builderSearch)}`;
              if (multiValueFilter) {
                toggleBuilderValue(insertion);
                setBuilderSearch("");
              } else {
                addVisualToken(insertion);
              }
            }}
            className="text-primary hover:bg-surface rounded-input flex min-h-10 w-full items-center px-3 text-left text-sm transition-colors duration-100"
          >
            Use “{builderSearch.trim()}”
          </button>
        )}
      </div>
      {multiValueFilter && (
        <div className="border-border mt-1 flex items-center justify-between gap-3 border-t p-2">
          <span className="text-text-muted text-xs">
            {builderSelectedValues.length === 0
              ? "Select one or more values"
              : `${builderSelectedValues.length} selected`}
          </span>
          <Button
            type="button"
            size="sm"
            disabled={builderSelectedValues.length === 0}
            onClick={addSelectedBuilderValues}
          >
            Add {builderSelectedValues.length || "values"}
          </Button>
        </div>
      )}
    </div>
  );

  const filterTypePicker = (
    <div>
      <div className="border-border flex items-center gap-2 border-b p-2">
        <SearchIcon size={15} className="text-text-muted" />
        <Input
          autoFocus
          value={builderSearch}
          onChange={(event) => setBuilderSearch(event.target.value)}
          className="text-text placeholder:text-text-muted h-8 min-w-0 flex-1 border-0 bg-transparent p-0 text-sm shadow-none focus:ring-0 focus:ring-offset-0 focus-visible:ring-0 focus-visible:ring-offset-0"
          placeholder="Find a filter…"
          aria-label="Find a search filter"
        />
      </div>
      <div
        role="menu"
        aria-label="Search filters"
        className="mt-1 max-h-72 overflow-y-auto"
        onKeyDown={handleBuilderMenuKeyDown}
      >
        {filterOptions
          .filter((option) =>
            `${option.label} ${option.detail}`.toLowerCase().includes(builderSearch.toLowerCase()),
          )
          .map((option) => {
            const Icon = option.icon;
            return (
              <button
                key={option.id}
                type="button"
                role="menuitem"
                onClick={() => selectBuilderFilter(option.insertion)}
                className="text-text-muted hover:bg-surface hover:text-text rounded-input flex min-h-11 w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-100"
              >
                <Icon size={16} className="text-primary shrink-0" />
                <span>
                  <span className="text-text block text-sm">{option.label}</span>
                  <span className="block text-xs">{option.detail}</span>
                </span>
              </button>
            );
          })}
      </div>
    </div>
  );

  const content = (
    <div
      ref={modalRef}
      className={`flex min-h-0 flex-col overflow-hidden ${presentation === "fullscreen" ? "h-full pt-[max(0.5rem,env(safe-area-inset-top))] pb-[max(0.5rem,env(safe-area-inset-bottom))]" : "max-h-[min(48rem,calc(100dvh-2rem))] min-h-[min(30rem,calc(100dvh-2rem))]"}`}
    >
      <header className="border-border flex shrink-0 items-start justify-between gap-4 border-b px-6 py-4">
        <div>
          <h1 id="search-modal-title" className="text-text text-xl font-semibold">
            Search
          </h1>
          <p className="text-text-muted mt-1 text-sm">
            Find pages by their words, then narrow the results with filters.
          </p>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex size-10 shrink-0 items-center justify-center transition-colors duration-100"
          aria-label="Close search"
          title="Close search"
        >
          <X size={18} />
        </button>
      </header>

      <section
        className="border-border shrink-0 border-b px-6 py-4"
        aria-label="Search query builder"
      >
        <div className="border-border bg-bg rounded-input focus-within:ring-primary flex min-h-12 flex-wrap items-center gap-2 border px-3 py-2 focus-within:ring-2">
          <SearchIcon size={18} className="text-primary shrink-0" />
          <Popover
            open={spacePickerOpen}
            onOpenChange={(nextOpen) => {
              if (nextOpen) {
                setBuilderOpen("filter");
                setBuilderPrefix("space:");
                setBuilderSearch("");
                setBuilderSelectedValues([]);
              } else if (spacePickerOpen) {
                closeBuilder();
              }
            }}
          >
            <PopoverTrigger asChild>
              <button
                type="button"
                className="border-border bg-surface hover:bg-surface-raised text-text rounded-input inline-flex h-8 items-center gap-2 border px-2.5 text-sm transition-colors duration-100"
                aria-label="Choose active search space"
              >
                <Layers3 size={15} className="text-primary" />
                <span>{spaceToken?.label.replace(/^space:\s*/i, "") || "All spaces"}</span>
                <ChevronDown size={14} className="text-text-muted" />
              </button>
            </PopoverTrigger>
            <PopoverContent align="start" sideOffset={6} className="w-80 p-2">
              {filterValuePicker}
            </PopoverContent>
          </Popover>
          {queryTokens.map((token) => (
            <span
              key={token.id}
              className={`${token.kind === "operator" ? "border-primary bg-primary/10" : "border-border bg-surface-raised"} text-text rounded-input inline-flex h-8 max-w-full items-center border text-sm`}
            >
              {editingTokenId === token.id && token.kind === "filter" ? (
                <Input
                  autoFocus
                  value={editingTokenValue}
                  onChange={(event) => setEditingTokenValue(event.target.value)}
                  onBlur={() => saveEditedToken(token)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") saveEditedToken(token);
                    if (event.key === "Escape") setEditingTokenId(null);
                  }}
                  className="h-7 min-w-32 border-0 bg-transparent px-2 shadow-none focus:ring-0 focus:ring-offset-0 focus-visible:ring-0 focus-visible:ring-offset-0"
                  aria-label={`Edit ${token.label}`}
                />
              ) : (
                <button
                  type="button"
                  onClick={() => {
                    if (token.kind !== "filter") return;
                    setEditingTokenId(token.id);
                    setEditingTokenValue(token.label.slice(token.label.indexOf(":") + 1).trim());
                  }}
                  className="h-full min-w-0 truncate px-2.5 text-left"
                  title={
                    token.kind === "filter"
                      ? "Edit filter"
                      : token.kind === "group"
                        ? "Multi-value filter"
                        : token.label
                  }
                >
                  {token.label}
                </button>
              )}
              <button
                type="button"
                onClick={() => removeVisualToken(token.id)}
                className="text-text-muted hover:text-text flex h-full w-8 shrink-0 items-center justify-center transition-colors duration-100"
                aria-label={`Remove ${token.label}`}
              >
                <X size={13} />
              </button>
            </span>
          ))}
          <Input
            ref={inputRef}
            type="search"
            value={visualQuery.text}
            onChange={(event) => commitVisualQuery(visualQuery.tokens, event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Backspace" && !visualQuery.text && queryTokens.length > 0) {
                event.preventDefault();
                removeVisualToken(queryTokens[queryTokens.length - 1].id);
                return;
              }
              handleKeyDown(event);
            }}
            className="text-text placeholder:text-text-muted h-8 min-w-32 flex-1 appearance-none border-0 bg-transparent p-0 text-sm shadow-none focus:ring-0 focus:ring-offset-0 focus-visible:ring-0 focus-visible:ring-offset-0 [&::-webkit-search-cancel-button]:hidden"
            placeholder={
              visualQuery.tokens.length ? "Add words…" : "Search pages by title or content…"
            }
            aria-label="Search pages"
            role="combobox"
            aria-autocomplete="list"
            aria-controls="search-query-suggestions"
            aria-expanded={suggestions.length > 0 || builderOpen !== null}
          />
        </div>

        <div className="relative mt-2 flex flex-wrap items-center gap-3">
          <Popover
            open={filterPickerOpen}
            onOpenChange={(nextOpen) => {
              if (nextOpen) {
                setBuilderOpen("filter");
                setBuilderPrefix("");
                setBuilderSearch("");
                setBuilderSelectedValues([]);
              } else if (filterPickerOpen) {
                closeBuilder();
              }
            }}
          >
            <PopoverTrigger asChild>
              <button
                type="button"
                className="text-primary hover:text-text inline-flex h-8 items-center gap-1.5 text-sm transition-colors duration-100"
              >
                <Plus size={15} /> Add a filter
              </button>
            </PopoverTrigger>
            <PopoverContent align="start" sideOffset={6} className="w-80 p-2">
              {builderPrefix ? filterValuePicker : filterTypePicker}
            </PopoverContent>
          </Popover>
          <button
            type="button"
            onClick={openSaveSearchDialog}
            className="text-primary hover:text-text ml-auto inline-flex h-8 items-center gap-1.5 text-sm transition-colors duration-100"
          >
            <BookmarkPlus size={15} /> Save view
          </button>
          {query && (
            <button
              type="button"
              onClick={clearSearch}
              className="text-danger hover:text-danger inline-flex h-8 items-center gap-1.5 text-sm transition-colors duration-100"
            >
              <Eraser size={15} /> Clear search
            </button>
          )}
        </div>

        <div className="mt-2">
          <button
            type="button"
            className="text-text-muted hover:text-text focus-ring inline-flex min-h-10 items-center gap-2 rounded-input px-2 text-sm transition-colors duration-100"
            aria-expanded={advancedSearchOpen}
            aria-controls="advanced-search-controls"
            onClick={() => {
              setAdvancedSearchOpen((isOpen) => !isOpen);
              if (builderOpen === "operator") closeBuilder();
            }}
          >
            <ChevronDown
              size={15}
              className={advancedSearchOpen ? "rotate-0" : "-rotate-90"}
              aria-hidden="true"
            />
            Advanced search
          </button>
          <div
            id="advanced-search-controls"
            hidden={!advancedSearchOpen}
            className="border-border bg-surface mt-1 grid gap-3 rounded-input border p-3"
            aria-label="Advanced search controls"
          >
            <p className="text-text-muted text-xs leading-5">
              Combine conditions with AND, OR, and NOT. Use <code>field:value</code> for contains,
              <code> = </code> or <code>is</code> for exact matches, <code>is not</code> to exclude,
              <code>*</code> for wildcards, <code>~</code> for fuzzy matches, or start a Dataview
              query with <code>dv:</code>.
            </p>
              <Popover
                open={builderOpen === "operator"}
                onOpenChange={(nextOpen) => {
                  if (nextOpen) {
                    setBuilderOpen("operator");
                    setBuilderPrefix("");
                    setBuilderSearch("");
                    setBuilderSelectedValues([]);
                  } else if (builderOpen === "operator") {
                    closeBuilder();
                  }
                }}
              >
                <PopoverTrigger asChild>
                  <button
                    type="button"
                    className="text-primary hover:text-text inline-flex min-h-10 items-center gap-1.5 text-sm transition-colors duration-100"
                  >
                    <Plus size={15} /> Add an operator
                  </button>
                </PopoverTrigger>
                <PopoverContent align="start" sideOffset={6} className="w-72 p-2">
                  <div role="menu" aria-label="Search operators" onKeyDown={handleBuilderMenuKeyDown}>
                    <div className="grid gap-1">
                      {getSearchBuilderOperatorOptions().map((operator) => (
                        <button
                          key={operator.value}
                          type="button"
                          role="menuitem"
                          disabled={operator.value !== "NOT" && !hasVisualCondition}
                          onClick={() => addVisualOperator(operator.value)}
                          className="text-text hover:bg-surface disabled:text-text-muted rounded-input flex min-h-10 items-center justify-between gap-4 px-3 py-2 text-left text-sm transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-60"
                        >
                          <span>{operator.label}</span>
                          <span className="text-text-muted text-right text-xs">{operator.detail}</span>
                        </button>
                      ))}
                    </div>
                  </div>
                </PopoverContent>
              </Popover>
          </div>
        </div>
      </section>

      <SearchSuggestionPanel
        suggestions={suggestions}
        selectedIndex={suggestionIndex}
        onSelect={applySuggestion}
      />
      {saveSearchPanel}

      {savedSearches.length > 0 && (
        <div className="border-border flex shrink-0 items-center gap-2 overflow-x-auto border-b px-6 py-2">
          <span className="text-text-muted shrink-0 text-xs">Saved views</span>
          {savedSearches.map((view) => (
            <span
              key={view.id}
              className={`${CONTROL_CHROME_RAIL_CLASS} h-7 shrink-0 px-1.5 py-0 text-xs`}
            >
              <button
                type="button"
                onClick={() => applySavedSearch(view)}
                className={`${CONTROL_CHROME_ACTION_CLASS} hover:text-primary max-w-32 truncate px-1`}
              >
                {view.name}
              </button>
              <button
                type="button"
                onClick={() => setSavedSearches(deleteSavedSearchView(view.id))}
                className={`${CONTROL_CHROME_ACTION_CLASS} p-0.5`}
                aria-label={`Delete saved search ${view.name}`}
              >
                <Trash2 size={12} />
              </button>
            </span>
          ))}
        </div>
      )}

      <section className="flex min-h-0 flex-1 flex-col" aria-labelledby="search-results-title">
        <div className="border-border flex shrink-0 items-center justify-between border-b px-6 py-3">
          <h2 id="search-results-title" className="text-text text-sm font-medium">
            Results
          </h2>
          <span className="text-text-muted text-xs">
            {loading
              ? "Searching…"
              : searchError
                ? "Search failed"
                : `${activeResultCount} ${activeResultCount === 1 ? "match" : "matches"}`}
          </span>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {!activeStarredOnly && results.length > 0 && (
            <div id="search-results-listbox" role="listbox" aria-label="Search results">
              <div className="border-border text-text-muted hidden grid-cols-[minmax(16rem,1fr)_11rem_11rem_11rem_7rem] gap-3 border-b px-6 py-2 text-xs lg:grid">
                <span>Document</span>
                <span>Space</span>
                <span>Folder</span>
                <span>Author</span>
                <span>Updated</span>
              </div>
              {results.map((result, index) => (
                <button
                  key={result.document_id}
                  id={getSearchResultOptionId(result.document_id)}
                  type="button"
                  role="option"
                  aria-selected={index === selectedIndex}
                  onMouseEnter={() => setSelectedIndex(index)}
                  onClick={() => {
                    navigate(getSearchResultDocumentPath(result));
                    onClose();
                  }}
                  className={`border-border grid min-h-16 w-full gap-3 border-b px-6 py-3 text-left transition-colors duration-100 lg:grid-cols-[minmax(16rem,1fr)_11rem_11rem_11rem_7rem] lg:items-center ${index === selectedIndex ? "bg-primary/10 text-text" : "text-text-muted hover:bg-surface-raised"}`}
                >
                  <span className="flex min-w-0 items-center gap-3">
                    <FileText size={17} className="text-primary shrink-0" />
                    <span className="min-w-0">
                      <span className="text-text block truncate text-sm font-medium">
                        {result.title}
                      </span>
                      <span className="block truncate text-xs">
                        {result.excerpt || "No preview available"}
                      </span>
                    </span>
                  </span>
                  <span className="hidden truncate text-sm lg:block">{result.space_name}</span>
                  <span className="hidden truncate text-sm lg:block">
                    {result.folder_name || "No folder"}
                  </span>
                  <span className="hidden truncate text-sm lg:block">{result.author}</span>
                  <span className="hidden items-center gap-1.5 text-xs lg:flex">
                    <Clock3 size={13} />
                    {formatSearchResultDate(result.updated_at)}
                  </span>
                  <span className="text-text-muted flex flex-wrap gap-x-3 text-xs lg:hidden">
                    <span>{result.space_name}</span>
                    <span>{result.folder_name || "No folder"}</span>
                    <span>{result.author}</span>
                    <span>{formatSearchResultDate(result.updated_at)}</span>
                  </span>
                </button>
              ))}
            </div>
          )}

          {activeStarredOnly && starredResults.length > 0 && (
            <div id="search-results-listbox" role="listbox" aria-label="Favorite search results">
              {starredResults.map((result, index) => (
                <button
                  key={`${result.spaceId}:${result.id}`}
                  id={getSearchResultOptionId(`${result.spaceId}:${result.id}`, true)}
                  type="button"
                  role="option"
                  aria-selected={index === selectedIndex}
                  onMouseEnter={() => setSelectedIndex(index)}
                  onClick={() => {
                    navigate(getStarredSearchResultDocumentPath(result));
                    onClose();
                  }}
                  className={`border-border flex min-h-16 w-full items-center gap-3 border-b px-6 py-3 text-left ${index === selectedIndex ? "bg-primary/10 text-text" : "text-text-muted hover:bg-surface-raised"}`}
                >
                  <Star size={17} className="text-primary" fill="currentColor" />
                  <span>
                    <span className="text-text block text-sm font-medium">{result.title}</span>
                    <span className="text-xs">{result.spaceName ?? "Favorites"}</span>
                  </span>
                </button>
              ))}
            </div>
          )}

          {!loading && searchError ? (
            <SearchFailureState onRetry={() => doSearch(query, filters)} />
          ) : !loading && activeResultCount === 0 ? (
            <div className="text-text-muted flex min-h-48 flex-col items-center justify-center gap-3 px-6 text-center">
              <SearchIcon size={32} />
              <div>
                <p className="text-text text-sm font-medium">
                  {query ? "No matching pages" : "Start a search"}
                </p>
                <p className="mt-1 text-xs">
                  {query
                    ? "Try removing a filter or changing your search terms."
                    : "Type a term or add a filter to find pages."}
                </p>
              </div>
            </div>
          ) : null}
        </div>
      </section>

      <footer className="border-border text-text-muted flex shrink-0 flex-wrap items-center gap-4 border-t px-6 py-3 text-xs">
        <span>
          <kbd className="border-border rounded-input border px-1.5 py-0.5 font-mono">esc</kbd>{" "}
          Close
        </span>
        <span>
          <kbd className="border-border rounded-input border px-1.5 py-0.5 font-mono">enter</kbd>{" "}
          Open
        </span>
        <span>
          <kbd className="border-border rounded-input border px-1.5 py-0.5 font-mono">↑↓</kbd> Move
        </span>
        <span className="text-text-muted ml-auto hidden sm:block">
          Backspace removes the last block
        </span>
      </footer>
    </div>
  );

  if (presentation === "fullscreen") {
    return (
      <DialogShell labelledBy="search-modal-title" presentation="fullscreen" onClose={onClose}>
        {content}
      </DialogShell>
    );
  }

  return (
    <DialogShell labelledBy="search-modal-title" maxWidth="settings" onClose={onClose}>
      {content}
    </DialogShell>
  );
}

function extractDataviewQuery(query: string): string | null {
  const trimmed = query.trim();
  const lower = trimmed.toLowerCase();
  if (lower.startsWith("dv:")) return trimmed.slice(3).trim();
  if (lower.startsWith("dataview:")) return trimmed.slice(9).trim();
  return null;
}

function formatSuggestionValue(value: string): string {
  const trimmed = value.trim();
  return /\s/.test(trimmed) ? `"${trimmed.replace(/"/g, '\\"')}"` : trimmed;
}

function uniqueByName<T extends { name: string }>(items: T[]): T[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = item.name.trim().toLowerCase();
    if (!key || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function getDateSuggestions(prefix: "created:" | "updated:", now: Date): SearchQuerySuggestion[] {
  const dates = [
    ["Today", 0],
    ["Yesterday", -1],
    ["Seven days ago", -7],
    ["Thirty days ago", -30],
  ] as const;

  return dates.map(([label, offset]) => {
    const date = new Date(now);
    date.setDate(date.getDate() + offset);
    const value = [
      date.getFullYear(),
      String(date.getMonth() + 1).padStart(2, "0"),
      String(date.getDate()).padStart(2, "0"),
    ].join("-");
    return {
      id: `${prefix}${value}`,
      label,
      detail: value,
      insertion: `${prefix}${value}`,
      complete: true,
      icon: CalendarDays,
    };
  });
}
