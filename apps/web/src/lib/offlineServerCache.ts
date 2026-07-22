const CACHE_PREFIX = "starlane_server_cache_v1";
const CACHE_INDEX_KEY = `${CACHE_PREFIX}:index`;
const MUTATION_QUEUE_KEY = `${CACHE_PREFIX}:mutation_queue`;
export const OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT = "starlane:offline-server-workspace-changed";

export interface QueuedServerMutation {
  id: string;
  serverBase: string;
  path: string;
  method: "POST" | "PUT" | "DELETE";
  body?: unknown;
  queuedAt: string;
  attempts: number;
  lastError?: string;
}

export interface OfflineServerWorkspaceStatus {
  ready: boolean;
  queuedMutationCount: number;
}

const workspaceStatusCache = new Map<string, OfflineServerWorkspaceStatus>();

function storage(): Storage | null {
  try {
    return globalThis.localStorage;
  } catch {
    return null;
  }
}

function emitWorkspaceStatusChanged(): void {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event(OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT));
  }
}

function normalizedBase(base: string): string {
  return base.replace(/\/+$/, "");
}

function key(base: string, path: string, scope = "public"): string {
  return `${CACHE_PREFIX}:${encodeURIComponent(normalizedBase(base))}:${encodeURIComponent(scope)}:${encodeURIComponent(path)}`;
}

/** Keeps cached workspaces separated without persisting a raw bearer token in a key. */
export function serverCacheScopeForToken(token: string | null): string {
  if (!token) return "public";
  let hash = 2166136261;
  for (let index = 0; index < token.length; index += 1) {
    hash ^= token.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return `session-${(hash >>> 0).toString(36)}`;
}

function readIndex(): string[] {
  const raw = storage()?.getItem(CACHE_INDEX_KEY);
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

function writeIndex(keys: string[]): void {
  storage()?.setItem(CACHE_INDEX_KEY, JSON.stringify([...new Set(keys)].slice(-300)));
}

export function cacheServerResponse(base: string, path: string, value: unknown, scope = "public"): void {
  const target = key(base, path, scope);
  const targetStorage = storage();
  if (!targetStorage) return;
  try {
    targetStorage.setItem(target, JSON.stringify({ cachedAt: new Date().toISOString(), value }));
    writeIndex([...readIndex(), target]);
    emitWorkspaceStatusChanged();
  } catch {
    // A full or disabled WebKit store must never prevent the online request from succeeding.
  }
}

export function readCachedServerResponse<T>(base: string, path: string, scope = "public"): T | null {
  const raw = storage()?.getItem(key(base, path, scope));
  if (!raw) return null;
  try {
    const record = JSON.parse(raw) as { value?: T };
    return record && "value" in record ? (record.value ?? null) : null;
  } catch {
    return null;
  }
}

export function hasCachedServerWorkspace(base: string, scope: string): boolean {
  return (
    readCachedServerResponse(base, "/auth/config") !== null &&
    readCachedServerResponse(base, "/auth/me", scope) !== null
  );
}

/** A small, safe status summary for native UI. Cached content stays session-scoped. */
export function getOfflineServerWorkspaceStatus(
  base: string,
  scope: string,
): OfflineServerWorkspaceStatus {
  const cacheKey = `${normalizedBase(base)}:${scope}`;
  const next = {
    ready: hasCachedServerWorkspace(base, scope),
    queuedMutationCount: listOfflineServerMutations(base).length,
  };
  const previous = workspaceStatusCache.get(cacheKey);
  if (
    previous &&
    previous.ready === next.ready &&
    previous.queuedMutationCount === next.queuedMutationCount
  ) {
    return previous;
  }
  workspaceStatusCache.set(cacheKey, next);
  return next;
}

function mutationId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `server-write-${crypto.randomUUID()}`;
  }
  return `server-write-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

function readMutationQueue(): QueuedServerMutation[] {
  const raw = storage()?.getItem(MUTATION_QUEUE_KEY);
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    return Array.isArray(value) ? value.filter(isQueuedServerMutation) : [];
  } catch {
    return [];
  }
}

function isQueuedServerMutation(value: unknown): value is QueuedServerMutation {
  if (!value || typeof value !== "object") return false;
  const item = value as Partial<QueuedServerMutation>;
  return (
    typeof item.id === "string" &&
    typeof item.serverBase === "string" &&
    typeof item.path === "string" &&
    (item.method === "POST" || item.method === "PUT" || item.method === "DELETE") &&
    typeof item.queuedAt === "string" &&
    typeof item.attempts === "number"
  );
}

function writeMutationQueue(queue: QueuedServerMutation[]): void {
  storage()?.setItem(MUTATION_QUEUE_KEY, JSON.stringify(queue));
  emitWorkspaceStatusChanged();
}

function updateCachedDocumentForMutation(base: string, path: string, body: unknown, scope: string): void {
  const documentCollectionMatch = path.match(/^\/spaces\/([^/]+)\/documents$/);
  if (documentCollectionMatch && body && typeof body === "object") {
    const [, spaceId] = documentCollectionMatch;
    const input = body as Record<string, unknown>;
    if (typeof input.source_id !== "string" || typeof input.title !== "string") return;
    const document = {
      id: input.source_id,
      title: input.title,
      folder_id: input.folder_id ?? undefined,
      parent_id: input.parent_id ?? undefined,
      created_at: input.created_at ?? new Date().toISOString(),
      updated_at: input.updated_at ?? new Date().toISOString(),
    };
    const cachedList = readCachedServerResponse<Record<string, unknown>[]>(
      base, `/spaces/${spaceId}/documents`, scope,
    );
    if (cachedList) {
      cacheServerResponse(base, `/spaces/${spaceId}/documents`, [document, ...cachedList], scope);
    }
    cacheServerResponse(base, `/spaces/${spaceId}/documents/${input.source_id}`, document, scope);
    cacheServerResponse(
      base,
      `/spaces/${spaceId}/documents/${input.source_id}/content`,
      { id: input.source_id, title: input.title, content: input.content ?? "" },
      scope,
    );
    return;
  }

  const folderCollectionMatch = path.match(/^\/spaces\/([^/]+)\/folders$/);
  if (folderCollectionMatch && body && typeof body === "object") {
    const [, spaceId] = folderCollectionMatch;
    const input = body as Record<string, unknown>;
    if (typeof input.source_id !== "string" || typeof input.name !== "string") return;
    const cachedList = readCachedServerResponse<Record<string, unknown>[]>(
      base, `/spaces/${spaceId}/folders`, scope,
    );
    if (cachedList) {
      cacheServerResponse(
        base,
        `/spaces/${spaceId}/folders`,
        [...cachedList, { id: input.source_id, name: input.name, parent_id: input.parent_id ?? undefined }],
        scope,
      );
    }
    return;
  }

  const contentMatch = path.match(/^\/spaces\/([^/]+)\/documents\/([^/]+)\/content$/);
  if (contentMatch && body && typeof body === "object" && typeof (body as { content?: unknown }).content === "string") {
    const [, spaceId, documentId] = contentMatch;
    const cached = readCachedServerResponse<Record<string, unknown>>(
      base, `/spaces/${spaceId}/documents/${documentId}/content`, scope,
    );
    if (cached) {
      cacheServerResponse(base, `/spaces/${spaceId}/documents/${documentId}/content`, {
        ...cached,
        content: (body as { content: string }).content,
      }, scope);
    }
    return;
  }

  const documentMatch = path.match(/^\/spaces\/([^/]+)\/documents\/([^/]+)$/);
  if (documentMatch) {
    const [, spaceId, documentId] = documentMatch;
    const cached = readCachedServerResponse<Record<string, unknown>>(
      base, `/spaces/${spaceId}/documents/${documentId}`, scope,
    );
    if (cached && body && typeof body === "object") {
      cacheServerResponse(base, `/spaces/${spaceId}/documents/${documentId}`, {
        ...cached,
        ...(body as Record<string, unknown>),
      }, scope);
    }
    const cachedList = readCachedServerResponse<Record<string, unknown>[]>(
      base, `/spaces/${spaceId}/documents`, scope,
    );
    if (cachedList) {
      cacheServerResponse(
        base,
        `/spaces/${spaceId}/documents`,
        path.endsWith(documentId) && body === undefined
          ? cachedList.filter((document) => document.id !== documentId)
          : cachedList.map((document) =>
              document.id === documentId && body && typeof body === "object"
                ? { ...document, ...(body as Record<string, unknown>) }
                : document,
            ),
        scope,
      );
    }
    return;
  }

  const folderMatch = path.match(/^\/spaces\/([^/]+)\/folders\/([^/]+)$/);
  if (folderMatch) {
    const [, spaceId, folderId] = folderMatch;
    const cachedList = readCachedServerResponse<Record<string, unknown>[]>(
      base, `/spaces/${spaceId}/folders`, scope,
    );
    if (cachedList) {
      cacheServerResponse(
        base,
        `/spaces/${spaceId}/folders`,
        body === undefined
          ? cachedList.filter((folder) => folder.id !== folderId)
          : cachedList.map((folder) =>
              folder.id === folderId && typeof body === "object"
                ? { ...folder, ...(body as Record<string, unknown>) }
                : folder,
            ),
        scope,
      );
    }
  }
}

export function queueOfflineServerMutation(
  base: string,
  path: string,
  method: "POST" | "PUT" | "DELETE",
  body?: unknown,
  scope = "public",
): QueuedServerMutation {
  const queue = readMutationQueue();
  const mutation: QueuedServerMutation = {
    id: mutationId(),
    serverBase: normalizedBase(base),
    path,
    method,
    body,
    queuedAt: new Date().toISOString(),
    attempts: 0,
  };
  writeMutationQueue([...queue, mutation]);
  updateCachedDocumentForMutation(base, path, body, scope);
  return mutation;
}

export function listOfflineServerMutations(base?: string): QueuedServerMutation[] {
  const normalized = base ? normalizedBase(base) : undefined;
  return readMutationQueue()
    .filter((item) => !normalized || item.serverBase === normalized)
    .sort((left, right) => left.queuedAt.localeCompare(right.queuedAt));
}

export function replaceOfflineServerMutations(queue: QueuedServerMutation[]): void {
  writeMutationQueue(queue);
}
