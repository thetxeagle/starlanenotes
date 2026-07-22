import { api } from "@/lib/api";
import { getCurrentLaunchMode } from "@/lib/launchMode";

const STARRED_DOCS_STORAGE_KEY = "starlane.starredDocuments.v1";

export interface StarredDocumentRecord {
  id: string;
  spaceId: string;
  title: string;
  spaceName?: string;
  icon?: string;
  updated_at?: string;
  starred_at: string;
}

export interface StarredDocumentInput {
  id: string;
  spaceId: string;
  title: string;
  spaceName?: string;
  icon?: string;
  updated_at?: string;
}

export type StarredDocumentSortMode = "starred_desc" | "title_asc" | "updated_desc";

interface ServerStarredDocumentRecord {
  id: string;
  space_id: string;
  space_name?: string;
  title: string;
  icon?: string;
  updated_at?: string;
  starred_at: string;
}

export function getStarredDocumentKey(input: { spaceId: string; id: string }): string {
  return `${input.spaceId}:${input.id}`;
}

export function readStarredDocuments(): StarredDocumentRecord[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(STARRED_DOCS_STORAGE_KEY) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isStarredDocumentRecord);
  } catch {
    return [];
  }
}

export function writeStarredDocuments(items: StarredDocumentRecord[]) {
  localStorage.setItem(STARRED_DOCS_STORAGE_KEY, JSON.stringify(items));
  emitStarredDocumentsChanged();
}

export function clearStarredDocuments() {
  localStorage.removeItem(STARRED_DOCS_STORAGE_KEY);
  emitStarredDocumentsChanged();
}

export function isDocumentStarred(input: { spaceId: string; id: string }): boolean {
  const key = getStarredDocumentKey(input);
  return readStarredDocuments().some((item) => getStarredDocumentKey(item) === key);
}

export function toggleStarredDocument(input: StarredDocumentInput): boolean {
  const key = getStarredDocumentKey(input);
  const current = readStarredDocuments();
  const existing = current.find((item) => getStarredDocumentKey(item) === key);
  if (existing) {
    writeStarredDocuments(current.filter((item) => getStarredDocumentKey(item) !== key));
    return false;
  }

  writeStarredDocuments([
    {
      ...input,
      title: input.title.trim() || "Untitled",
      starred_at: new Date().toISOString(),
    },
    ...current,
  ]);
  return true;
}

export async function loadStarredDocuments(spaceId?: string): Promise<StarredDocumentRecord[]> {
  if (!shouldUseServerStars() || !spaceId) return readStarredDocuments();
  try {
    const serverItems = await api.fetch<ServerStarredDocumentRecord[]>(
      `/spaces/${spaceId}/documents/starred`,
    );
    const normalized = serverItems.map(normalizeServerStarredDocument);
    const others = readStarredDocuments().filter((item) => item.spaceId !== spaceId);
    writeStarredDocuments([...normalized, ...others]);
  } catch {
    return readStarredDocuments();
  }
  return readStarredDocuments();
}

export async function toggleStarredDocumentSynced(input: StarredDocumentInput): Promise<boolean> {
  const nextStarred = toggleStarredDocument(input);
  if (!shouldUseServerStars()) return nextStarred;

  try {
    await api.fetch(`/spaces/${input.spaceId}/documents/${input.id}/star`, {
      method: nextStarred ? "POST" : "DELETE",
    });
    await loadStarredDocuments(input.spaceId);
  } catch {
    toggleStarredDocument(input);
    throw new Error("Starred document could not be synced.");
  }

  return nextStarred;
}

export function updateStarredDocumentMetadata(input: StarredDocumentInput) {
  const key = getStarredDocumentKey(input);
  const current = readStarredDocuments();
  if (!current.some((item) => getStarredDocumentKey(item) === key)) return;
  writeStarredDocuments(mergeStarredDocumentMetadata(current, [input]));
}

export function mergeStarredDocumentMetadata(
  documents: StarredDocumentRecord[],
  updates: StarredDocumentInput[],
): StarredDocumentRecord[] {
  const updateMap = new Map(updates.map((item) => [getStarredDocumentKey(item), item]));
  return documents.map((item) => {
    const update = updateMap.get(getStarredDocumentKey(item));
    return update
      ? {
          ...item,
          title: update.title.trim() || item.title,
          spaceName: update.spaceName ?? item.spaceName,
          icon: update.icon ?? item.icon,
          updated_at: update.updated_at ?? item.updated_at,
        }
      : item;
  });
}

export function filterStarredDocuments(
  documents: StarredDocumentRecord[],
  query: string,
  sortMode: StarredDocumentSortMode = "starred_desc",
): StarredDocumentRecord[] {
  const normalizedQuery = query.trim().toLowerCase();
  return [...documents]
    .filter((doc) => {
      if (!normalizedQuery) return true;
      return [doc.title, doc.spaceName, doc.icon]
        .filter(Boolean)
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery);
    })
    .sort(compareStarredDocuments(sortMode));
}

export function compareStarredDocuments(sortMode: StarredDocumentSortMode) {
  return (a: StarredDocumentRecord, b: StarredDocumentRecord): number => {
    switch (sortMode) {
      case "title_asc":
        return (
          a.title.localeCompare(b.title) ||
          b.starred_at.localeCompare(a.starred_at) ||
          a.spaceId.localeCompare(b.spaceId)
        );
      case "updated_desc":
        return (
          (b.updated_at ?? "").localeCompare(a.updated_at ?? "") ||
          b.starred_at.localeCompare(a.starred_at) ||
          a.title.localeCompare(b.title)
        );
      case "starred_desc":
      default:
        return (
          b.starred_at.localeCompare(a.starred_at) ||
          a.title.localeCompare(b.title) ||
          a.spaceId.localeCompare(b.spaceId)
        );
    }
  };
}

function isStarredDocumentRecord(value: unknown): value is StarredDocumentRecord {
  if (!value || typeof value !== "object") return false;
  const record = value as Partial<StarredDocumentRecord>;
  return (
    typeof record.id === "string" &&
    typeof record.spaceId === "string" &&
    typeof record.title === "string" &&
    typeof record.starred_at === "string"
  );
}

function normalizeServerStarredDocument(item: ServerStarredDocumentRecord): StarredDocumentRecord {
  return {
    id: item.id,
    spaceId: item.space_id,
    title: item.title,
    spaceName: item.space_name,
    icon: item.icon,
    updated_at: item.updated_at,
    starred_at: item.starred_at,
  };
}

function shouldUseServerStars(): boolean {
  return getCurrentLaunchMode() !== "local" && api.isAuthenticated();
}

function emitStarredDocumentsChanged() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent("starlane:starred-documents-changed"));
}
