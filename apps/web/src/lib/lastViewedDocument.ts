export const LAST_VIEWED_DOCUMENT_KEY = "starlane_last_viewed_document";

export interface LastViewedDocumentRoute {
  spaceId: string;
  docId: string;
  path: string;
}

interface StorageLike {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
  removeItem: (key: string) => void;
}

function getBrowserStorage(): StorageLike | null {
  if (typeof window === "undefined") return null;
  return window.localStorage ?? null;
}

export function buildDocumentRoutePath(spaceId: string, docId: string): string {
  return `/spaces/${encodeURIComponent(spaceId)}/docs/${encodeURIComponent(docId)}`;
}

export function parseDocumentRoutePath(path: string): LastViewedDocumentRoute | null {
  const match = path.match(/^\/spaces\/([^/]+)\/docs\/([^/]+)\/?$/);
  if (!match) return null;

  try {
    const spaceId = decodeURIComponent(match[1]);
    const docId = decodeURIComponent(match[2]);
    if (!spaceId.trim() || !docId.trim()) return null;
    return {
      spaceId,
      docId,
      path: buildDocumentRoutePath(spaceId, docId),
    };
  } catch {
    return null;
  }
}

export function readLastViewedDocumentRoute(
  storage: StorageLike | null = getBrowserStorage(),
): LastViewedDocumentRoute | null {
  if (!storage) return null;

  try {
    const raw = storage.getItem(LAST_VIEWED_DOCUMENT_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<LastViewedDocumentRoute>;
    if (
      typeof parsed.spaceId !== "string" ||
      !parsed.spaceId.trim() ||
      typeof parsed.docId !== "string" ||
      !parsed.docId.trim()
    ) {
      storage.removeItem(LAST_VIEWED_DOCUMENT_KEY);
      return null;
    }
    return {
      spaceId: parsed.spaceId,
      docId: parsed.docId,
      path: buildDocumentRoutePath(parsed.spaceId, parsed.docId),
    };
  } catch {
    storage.removeItem(LAST_VIEWED_DOCUMENT_KEY);
    return null;
  }
}

export function rememberLastViewedDocumentRoute(
  spaceId: string,
  docId: string,
  storage: StorageLike | null = getBrowserStorage(),
): void {
  if (!storage) return;
  if (!spaceId.trim() || !docId.trim()) {
    clearLastViewedDocumentRoute(storage);
    return;
  }

  const route = {
    spaceId,
    docId,
    path: buildDocumentRoutePath(spaceId, docId),
  };
  storage.setItem(LAST_VIEWED_DOCUMENT_KEY, JSON.stringify(route));
}

export function clearLastViewedDocumentRoute(
  storage: StorageLike | null = getBrowserStorage(),
): void {
  storage?.removeItem(LAST_VIEWED_DOCUMENT_KEY);
}
