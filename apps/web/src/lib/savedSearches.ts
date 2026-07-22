import type { SearchFilters } from "@/lib/dataClient";

const SAVED_SEARCHES_STORAGE_KEY = "starlane.savedSearches.v1";

export interface SavedSearchView {
  id: string;
  name: string;
  query: string;
  filters: SearchFilters;
  starredOnly: boolean;
  created_at: string;
}

export interface SavedSearchInput {
  query: string;
  filters: SearchFilters;
  starredOnly: boolean;
}

export function readSavedSearchViews(): SavedSearchView[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(SAVED_SEARCHES_STORAGE_KEY) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isSavedSearchView);
  } catch {
    return [];
  }
}

export function writeSavedSearchViews(views: SavedSearchView[]) {
  localStorage.setItem(SAVED_SEARCHES_STORAGE_KEY, JSON.stringify(views.slice(0, 12)));
}

export function createSavedSearchView(name: string, input: SavedSearchInput): SavedSearchView {
  return {
    id: `search-${Date.now().toString(36)}-${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
    name,
    query: input.query.trim(),
    filters: compactSearchFilters(input.filters),
    starredOnly: input.starredOnly,
    created_at: new Date().toISOString(),
  };
}

export function saveSearchView(name: string, input: SavedSearchInput): SavedSearchView[] {
  const view = createSavedSearchView(name.trim(), input);
  const current = readSavedSearchViews();
  const next = [view, ...current.filter((item) => item.name !== view.name)];
  writeSavedSearchViews(next);
  return readSavedSearchViews();
}

export function deleteSavedSearchView(id: string): SavedSearchView[] {
  writeSavedSearchViews(readSavedSearchViews().filter((view) => view.id !== id));
  return readSavedSearchViews();
}

function compactSearchFilters(filters: SearchFilters): SearchFilters {
  return Object.fromEntries(
    Object.entries(filters).filter(([, value]) => typeof value === "string" && value.trim()),
  ) as SearchFilters;
}

function isSavedSearchView(value: unknown): value is SavedSearchView {
  if (!value || typeof value !== "object") return false;
  const view = value as Partial<SavedSearchView>;
  return (
    typeof view.id === "string" &&
    typeof view.name === "string" &&
    typeof view.query === "string" &&
    typeof view.starredOnly === "boolean" &&
    typeof view.created_at === "string" &&
    (!view.filters || typeof view.filters === "object")
  );
}
