import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { BookmarkPlus, FileText, Star, Trash2, X } from "lucide-react";
import { dataClient, type SearchFilters, type SearchResultRecord } from "@/lib/dataClient";
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
  toggleStarredDocumentSynced,
  type StarredDocumentRecord,
} from "@/lib/starredDocuments";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SearchInput } from "@/components/ui/search-input";

interface SearchModalProps {
  open: boolean;
  onClose: () => void;
  preset?: SearchModalPreset;
  presentation?: "dialog" | "popover" | "fullscreen";
  query?: string;
  onQueryChange?: (query: string) => void;
}

export interface SearchModalPreset {
  nonce: number;
  query?: string;
  filters?: SearchFilters;
}

export function getSearchResultDocumentPath(result: SearchResultRecord): string {
  return buildDocumentRoutePath(result.space_id, result.document_id);
}

export function getStarredSearchResultDocumentPath(result: StarredDocumentRecord): string {
  return buildDocumentRoutePath(result.spaceId, result.id);
}

export function getDefaultSavedSearchViewName(query: string, starredOnly: boolean): string {
  const trimmed = query.trim();
  if (trimmed) return trimmed.slice(0, 48);
  return starredOnly ? "Starred documents" : "Saved search";
}

export function canSaveSearchViewName(name: string): boolean {
  return name.trim().length > 0;
}

export function SearchModal({
  open,
  onClose,
  preset,
  presentation = "dialog",
  query: controlledQuery,
  onQueryChange,
}: SearchModalProps) {
  const navigate = useNavigate();
  const modalRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
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
  const [loading, setLoading] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout>>();
  const parsedQuery = parseSearchQueryInput(query);
  const activeStarredOnly = starredOnly || parsedQuery.starredOnly;
  const starredResults = activeStarredOnly
    ? filterStarredDocuments(starredDocuments, parsedQuery.query)
    : [];
  const hasActiveFilters = Object.values(filters).some((value) => Boolean(value?.trim()));

  const updateQuery = useCallback(
    (nextQuery: string) => {
      setQuery(nextQuery);
      onQueryChange?.(nextQuery);
    },
    [onQueryChange],
  );

  useEffect(() => {
    if (controlledQuery !== undefined && controlledQuery !== query) {
      setQuery(controlledQuery);
    }
  }, [controlledQuery, query]);

  // Focus input when modal opens
  useEffect(() => {
    if (open) {
      previousFocusRef.current = document.activeElement as HTMLElement | null;
      const initialQuery = formatSearchQueryInput(
        preset?.query ?? controlledQuery ?? "",
        preset?.filters,
      );
      setQuery(initialQuery);
      if (preset?.query !== undefined) onQueryChange?.(initialQuery);
      setFilters({});
      setResults([]);
      setStarredOnly(false);
      setStarredDocuments(readStarredDocuments());
      setSavedSearches(readSavedSearchViews());
      setSaveSearchOpen(false);
      setSaveSearchName("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    } else {
      previousFocusRef.current?.focus();
    }
  }, [controlledQuery, onQueryChange, open, preset]);

  useEffect(() => {
    if (!open) return;
    const refreshStarredDocuments = () => setStarredDocuments(readStarredDocuments());
    window.addEventListener("starlane:starred-documents-changed", refreshStarredDocuments);
    return () =>
      window.removeEventListener("starlane:starred-documents-changed", refreshStarredDocuments);
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
  const doSearch = useCallback((q: string, nextFilters: SearchFilters = filters) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    const dataviewQuery = extractDataviewQuery(q);
    if (dataviewQuery !== null) {
      if (dataviewQuery.length < 2) {
        setResults([]);
        return;
      }
      debounceRef.current = setTimeout(async () => {
        setLoading(true);
        try {
          const data = await dataClient.queryDataview(dataviewQuery, 10);
          setResults(data);
          setSelectedIndex(0);
        } catch {
          setResults([]);
        } finally {
          setLoading(false);
        }
      }, 250);
      return;
    }

    const parsed = parseSearchQueryInput(q);
    const effectiveFilters = { ...parsed.filters, ...nextFilters };

    if (parsed.starredOnly) {
      setResults([]);
      setSelectedIndex(0);
      return;
    }

    const hasEffectiveFilters = Object.values(effectiveFilters).some((value) => Boolean(value?.trim()));
    if (parsed.query.length < 2 && !hasEffectiveFilters) {
      setResults([]);
      return;
    }

    debounceRef.current = setTimeout(async () => {
      setLoading(true);
      try {
        const data = await dataClient.searchDocuments(parsed.query, 10, effectiveFilters);
        setResults(data);
        setSelectedIndex(0);
      } catch {
        setResults([]);
      } finally {
        setLoading(false);
      }
    }, 250);
  }, [filters]);

  useEffect(() => {
    if (!open || controlledQuery === undefined) return;
    doSearch(controlledQuery);
  }, [controlledQuery, doSearch, open]);

  const updateFilter = (key: keyof SearchFilters, value: string) => {
    const nextFilters = { ...filters, [key]: value };
    if (!value.trim()) delete nextFilters[key];
    setFilters(nextFilters);
    doSearch(query, nextFilters);
  };

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

  // Global Cmd+K listener
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        if (open) onClose();
        else onClose(); // Toggle handled by parent
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  const toggleStarredOnly = () => {
    const next = !starredOnly;
    setStarredOnly(next);
    setResults([]);
    setSelectedIndex(0);
    if (!next) doSearch(query);
  };

  const openSaveSearchDialog = () => {
    setSaveSearchName(getDefaultSavedSearchViewName(query, activeStarredOnly));
    setSaveSearchOpen(true);
  };

  const closeSaveSearchDialog = () => {
    setSaveSearchOpen(false);
    setSaveSearchName("");
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

  if (!open) return null;

  const saveSearchPanel = saveSearchOpen ? (
    <div
      role="dialog"
      aria-labelledby="save-search-view-title"
      className="border-border bg-surface-raised border-b px-3 py-3"
    >
      <form
        className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]"
        onSubmit={(event) => {
          event.preventDefault();
          saveCurrentSearch();
        }}
      >
        <div className="min-w-0">
          <label id="save-search-view-title" htmlFor="save-search-view-name" className="text-text text-xs font-medium">
            Save search view
          </label>
          <Input
            id="save-search-view-name"
            value={saveSearchName}
            onChange={(event) => setSaveSearchName(event.target.value)}
            className="mt-1 h-8 bg-surface text-sm"
            placeholder="View name"
            aria-label="Saved search view name"
            autoComplete="off"
          />
        </div>
        <div className="flex items-end justify-end gap-2">
          <Button type="button" variant="ghost" size="sm" onClick={closeSaveSearchDialog}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSaveSearchViewName(saveSearchName)}>
            Save
          </Button>
        </div>
      </form>
    </div>
  ) : null;

  const mobileContent = (
    <div ref={modalRef} className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border p-2">
        <span id="search-modal-title" className="sr-only">
          Search documents
        </span>
        <SearchInput
          ref={inputRef}
          value={query}
          onChange={(e) => {
            updateQuery(e.target.value);
            setSelectedIndex(0);
            if (!starredOnly) doSearch(e.target.value);
          }}
          onKeyDown={handleKeyDown}
          aria-label="Search documents"
          placeholder={'Search SNQL: "notes" tag:work folder:project has:attachment'}
          containerClassName="min-w-0 flex-1"
          className="h-9 bg-surface text-sm"
        />
        <button
          type="button"
          onClick={onClose}
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
          aria-label="Close search"
        >
          <X size={17} />
        </button>
      </div>

      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border p-2">
        <button
          type="button"
          onClick={openSaveSearchDialog}
          className="inline-flex h-8 items-center gap-1 rounded-input border border-border bg-surface px-2 text-xs text-text-muted hover:bg-surface-raised hover:text-text"
        >
          <BookmarkPlus size={13} />
          Save view
        </button>
        {savedSearches.map((view) => (
          <span
            key={view.id}
            className="inline-flex h-8 items-center gap-1 rounded-input border border-border bg-surface px-1.5 text-xs text-text"
          >
            <button
              type="button"
              onClick={() => applySavedSearch(view)}
              className="max-w-32 truncate hover:text-primary"
              title={view.name}
            >
              {view.name}
            </button>
            <button
              type="button"
              onClick={() => setSavedSearches(deleteSavedSearchView(view.id))}
              className="rounded-input p-0.5 text-text-muted hover:bg-surface-raised hover:text-text"
              aria-label={`Delete saved search ${view.name}`}
            >
              <Trash2 size={12} />
            </button>
          </span>
        ))}
        {query && (
          <button
            type="button"
            onClick={() => {
              updateQuery("");
              setFilters({});
              setResults([]);
            }}
            className="ml-auto rounded-button px-2 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
          >
            Clear
          </button>
        )}
      </div>
      {saveSearchPanel}

      <div className="min-h-0 flex-1 overflow-y-auto p-1">
        {activeStarredOnly && starredResults.length > 0 && (
          <div>
            {starredResults.map((r, i) => (
              <div
                key={`${r.spaceId}:${r.id}`}
                className={`rounded-button flex w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-100 ${
                  i === selectedIndex
                    ? "bg-surface-raised text-text"
                    : "text-text-muted hover:bg-surface-raised"
                }`}
              >
                <button
                  type="button"
                  onClick={(event) => {
                    event.stopPropagation();
                    navigate(getStarredSearchResultDocumentPath(r));
                    onClose();
                  }}
                  className="flex min-w-0 flex-1 items-center gap-3 text-left"
                >
                  <FileText size={16} className="shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm">{r.title}</div>
                    <div className="truncate text-xs text-text-muted">{r.spaceName ?? "Starred"}</div>
                  </div>
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void toggleStarredDocumentSynced(r).finally(() =>
                      setStarredDocuments(readStarredDocuments()),
                    )
                  }
                  className="rounded-button p-1 text-primary hover:bg-primary/10"
                  aria-label={`Remove star from ${r.title}`}
                  title="Remove star"
                >
                  <Star size={14} fill="currentColor" />
                </button>
              </div>
            ))}
          </div>
        )}

        {results.length > 0 && (
          <div>
            {results.map((r, i) => (
              <button
                key={r.document_id}
                type="button"
                onClick={() => {
                  navigate(getSearchResultDocumentPath(r));
                  onClose();
                }}
                className={`rounded-button flex w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-100 ${
                  i === selectedIndex
                    ? "bg-surface-raised text-text"
                    : "text-text-muted hover:bg-surface-raised"
                }`}
              >
                <FileText size={16} className="shrink-0" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{r.title}</div>
                  <div className="truncate text-xs text-text-muted">{r.space_name}</div>
                </div>
              </button>
            ))}
          </div>
        )}

        {activeStarredOnly && starredResults.length === 0 && (
          <div className="p-4 text-center text-sm text-text-muted">No starred documents found</div>
        )}
        {!activeStarredOnly && (query.length >= 2 || hasActiveFilters) && results.length === 0 && !loading && (
          <div className="p-4 text-center text-sm text-text-muted">No results found</div>
        )}
      </div>
    </div>
  );

  const content = (
    <div ref={modalRef} className="overflow-hidden">
      {presentation === "popover" && (
        <span id="search-modal-title" className="sr-only">
          Search results
        </span>
      )}
      {presentation !== "popover" && (
        <>
        {/* Search input */}
        <div className="border-border flex items-center gap-2 border-b px-3 py-2">
          <span id="search-modal-title" className="sr-only">
            Search documents
          </span>
          <SearchInput
            ref={inputRef}
            value={query}
            onChange={(e) => {
              updateQuery(e.target.value);
              setSelectedIndex(0);
              if (!starredOnly) doSearch(e.target.value);
            }}
            onKeyDown={handleKeyDown}
            aria-label="Search documents"
            placeholder="Search documents..."
            containerClassName="min-w-0 flex-1"
            className="h-10 bg-surface text-sm"
          />
          {query && (
            <button
              type="button"
              onClick={() => {
                updateQuery("");
                setFilters({});
                setResults([]);
              }}
              className="text-text-muted hover:text-text"
              aria-label="Clear search"
            >
              <X size={14} />
            </button>
          )}
        </div>

        <div className="border-border grid gap-2 border-b px-3 py-2 sm:grid-cols-[auto_1fr_1fr_1fr]">
          <button
            type="button"
            onClick={toggleStarredOnly}
            className={`inline-flex h-8 items-center justify-center gap-1 rounded-input border px-2 text-xs transition-colors duration-100 ${
              starredOnly
                ? "border-primary bg-primary/10 text-primary"
                : "border-border bg-surface text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
            aria-pressed={starredOnly}
          >
            <Star size={13} fill={starredOnly ? "currentColor" : "none"} />
            Starred
          </button>
          <Input
            type="text"
            value={filters.tag ?? ""}
            onChange={(event) => updateFilter("tag", event.target.value)}
            placeholder="Tag"
            disabled={starredOnly}
            aria-label="Search tag filter"
            className="h-8 bg-surface px-2 text-xs"
          />
          <Input
            type="text"
            value={filters.property_name ?? ""}
            onChange={(event) => updateFilter("property_name", event.target.value)}
            placeholder="Property"
            disabled={starredOnly}
            aria-label="Search property filter"
            className="h-8 bg-surface px-2 text-xs"
          />
          <Input
            type="text"
            value={filters.property_value ?? ""}
            onChange={(event) => updateFilter("property_value", event.target.value)}
            placeholder="Value"
            disabled={starredOnly}
            aria-label="Search property value filter"
            className="h-8 bg-surface px-2 text-xs"
          />
        </div>
        <div className="border-border flex flex-wrap items-center gap-2 border-b px-3 py-2">
          <button
            type="button"
            onClick={openSaveSearchDialog}
            className="inline-flex h-7 items-center gap-1 rounded-input border border-border bg-surface px-2 text-xs text-text-muted hover:bg-surface-raised hover:text-text"
          >
            <BookmarkPlus size={13} />
            Save view
          </button>
          {savedSearches.map((view) => (
            <span
              key={view.id}
              className="inline-flex h-7 items-center gap-1 rounded-input border border-border bg-surface px-1.5 text-xs text-text"
            >
              <button
                type="button"
                onClick={() => applySavedSearch(view)}
                className="max-w-32 truncate hover:text-primary"
                title={view.name}
              >
                {view.name}
              </button>
              <button
                type="button"
                onClick={() => setSavedSearches(deleteSavedSearchView(view.id))}
                className="rounded-input p-0.5 text-text-muted hover:bg-surface-raised hover:text-text"
                aria-label={`Delete saved search ${view.name}`}
              >
                <Trash2 size={12} />
              </button>
            </span>
          ))}
        </div>
        {saveSearchPanel}
        </>
      )}

        {/* Results */}
        {activeStarredOnly && starredResults.length > 0 && (
          <div className="max-h-72 overflow-y-auto p-1">
            {starredResults.map((r, i) => (
              <div
                key={`${r.spaceId}:${r.id}`}
                className={`rounded-button flex w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-100 ${
                  i === selectedIndex
                    ? "bg-surface-raised text-text"
                    : "text-text-muted hover:bg-surface-raised"
                }`}
              >
                <button
                  type="button"
                  onClick={(event) => {
                    event.stopPropagation();
                    navigate(getStarredSearchResultDocumentPath(r));
                    onClose();
                  }}
                  className="flex min-w-0 flex-1 items-center gap-3 text-left"
                >
                  <FileText size={16} className="shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm">{r.title}</div>
                    <div className="text-text-muted truncate text-xs">{r.spaceName ?? "Starred"}</div>
                  </div>
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void toggleStarredDocumentSynced(r).finally(() =>
                      setStarredDocuments(readStarredDocuments()),
                    )
                  }
                  className="rounded-button p-1 text-primary hover:bg-primary/10"
                  aria-label={`Remove star from ${r.title}`}
                  title="Remove star"
                >
                  <Star size={14} fill="currentColor" />
                </button>
              </div>
            ))}
          </div>
        )}
        {results.length > 0 && (
          <div className="max-h-72 overflow-y-auto p-1">
            {results.map((r, i) => (
              <button
                key={r.document_id}
                type="button"
                onClick={() => {
                  navigate(getSearchResultDocumentPath(r));
                  onClose();
                }}
                className={`rounded-button flex w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-100 ${
                  i === selectedIndex
                    ? "bg-surface-raised text-text"
                    : "text-text-muted hover:bg-surface-raised"
                }`}
              >
                <FileText size={16} className="shrink-0" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{r.title}</div>
                  <div className="text-text-muted truncate text-xs">{r.space_name}</div>
                </div>
              </button>
            ))}
          </div>
        )}

        {/* Empty state */}
        {activeStarredOnly && starredResults.length === 0 && (
          <div className="text-text-muted p-4 text-center text-sm">No starred documents found</div>
        )}
        {!activeStarredOnly && (query.length >= 2 || Object.values(filters).some(Boolean)) && results.length === 0 && !loading && (
          <div className="text-text-muted p-4 text-center text-sm">No results found</div>
        )}

        {/* Footer hint */}
        <div className="border-border text-text-muted flex items-center gap-3 border-t px-3 py-1.5 text-[10px]">
          <span>
            <kbd className="border-border rounded border px-1 font-mono">↑↓</kbd> navigate
          </span>
          <span>
            <kbd className="border-border rounded border px-1 font-mono">↵</kbd> open
          </span>
          <span>
            <kbd className="border-border rounded border px-1 font-mono">esc</kbd> close
          </span>
        </div>
    </div>
  );

  if (presentation === "popover") {
    return (
      <div
        role="dialog"
        aria-modal="false"
        aria-labelledby="search-modal-title"
        className="starlane-menu-surface absolute left-1/2 top-full z-50 mt-2 w-[min(42rem,calc(100vw-2rem))] -translate-x-1/2 overflow-hidden shadow-card"
      >
        {content}
      </div>
    );
  }

  if (presentation === "fullscreen") {
    return (
      <div className="fixed inset-0 z-[60] bg-surface pt-[max(0.5rem,env(safe-area-inset-top))] pb-[max(0.5rem,env(safe-area-inset-bottom))]">
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby="search-modal-title"
          className="flex h-full min-h-0 w-full flex-col overflow-hidden bg-surface"
        >
          {mobileContent}
        </div>
      </div>
    );
  }

  return (
    <DialogShell labelledBy="search-modal-title" maxWidth="xl" onClose={onClose}>
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
