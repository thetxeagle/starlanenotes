import {
  isOfflineReplayOrigin,
  sameOfflineReplayOrigin,
  type OfflineReplayOrigin,
} from "./offlineReplayOrigin";
const CACHE_PREFIX = "starlane_server_cache_v1";
const CACHE_INDEX_KEY = `${CACHE_PREFIX}:index`;
const MUTATION_QUEUE_KEY = `${CACHE_PREFIX}:mutation_queue`;
const ATTACHMENT_QUEUE_KEY = `${CACHE_PREFIX}:attachment_queue`;
const SESSION_SCOPE_KEY = `${CACHE_PREFIX}:session-scope`;
export const OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT = "starlane:offline-server-workspace-changed";

export interface QueuedServerMutation {
  /** Immutable server deletion identity; queue evidence IDs remain separate. */
  deletionOperationId?: string;
  id: string;
  serverBase: string;
  path: string;
  method: "POST" | "PUT" | "DELETE";
  body?: unknown;
  queuedAt: string;
  /** Monotonic enqueue order; absent on legacy queues. */
  queueOrder?: number;
  origin?: OfflineReplayOrigin;
  replayState?: "held" | "unknown";
  hold?: { schemaVersion: 1; reason: string; status?: number; message: string };
  attempts: number;
  lastError?: string;
}

export interface QueuedServerAttachment {
  scopeVersion?:2;
  draftId?:string;
  id: string;
  /** Immutable server retry identity, persisted before any dispatch; absent for legacy entries. */
  uploadOperationId?: string;
  serverBase: string;
  path: string;
  spaceId: string;
  documentId: string;
  name: string;
  mimeType: string;
  dataUrl: string;
  queuedAt: string;
  /** Monotonic enqueue order; absent on legacy queues. */
  queueOrder?: number;
  origin?: OfflineReplayOrigin;
  replayState?: "unknown";
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
  if (typeof window !== "undefined" && typeof window.dispatchEvent === "function") {
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

export function serverCacheScopeForSession(
  accessToken: string | null,
  refreshToken: string | null,
): string {
  const targetStorage = storage();
  if (!accessToken && !refreshToken) return "public";
  const existing = targetStorage?.getItem(SESSION_SCOPE_KEY);
  if (existing) return `session-${existing}`;
  const seed =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  targetStorage?.setItem(SESSION_SCOPE_KEY, seed);
  return `session-${seed}`;
}

export function clearServerCacheSessionScope(): void {
  storage()?.removeItem(SESSION_SCOPE_KEY);
}

function readIndex(): string[] {
  const raw = storage()?.getItem(CACHE_INDEX_KEY);
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    return Array.isArray(value)
      ? value.filter((item): item is string => typeof item === "string")
      : [];
  } catch {
    return [];
  }
}

function writeIndex(keys: string[]): void {
  storage()?.setItem(CACHE_INDEX_KEY, JSON.stringify([...new Set(keys)].slice(-300)));
}

export function cacheServerResponse(
  base: string,
  path: string,
  value: unknown,
  scope = "public",
): void {
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

export function readCachedServerResponse<T>(
  base: string,
  path: string,
  scope = "public",
): T | null {
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

function attachmentId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `server-attachment-${crypto.randomUUID()}`;
  }
  return `server-attachment-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

// Each new item owns one localStorage key: unrelated enqueues and acknowledgements
// cannot overwrite one another across tabs. Legacy arrays remain readable; a null
// journal entry acknowledges a legacy ID without rewriting that shared array.
function readQueue<T extends { id: string; queuedAt: string; queueOrder?: number }>(
  queueKey: string,
  valid: (value: unknown) => value is T,
): T[] {
  const target = storage();
  if (!target) return [];
  const items = new Map<string, T>();
  const legacyOrder = new Map<string, number>();
  try {
    const legacy: unknown = JSON.parse(target.getItem(queueKey) ?? "[]");
    if (Array.isArray(legacy)) {
      for (const [index, item] of legacy.entries()) {
        if (valid(item)) {
          items.set(item.id, item);
          legacyOrder.set(item.id, index);
        }
      }
    }
  } catch {
    /* A malformed legacy array must not hide valid journal items. */
  }
  const prefix = `${queueKey}:item:`;
  const keys: string[] = [];
  for (let index = 0; index < target.length; index += 1) {
    const itemKey = target.key(index);
    if (itemKey?.startsWith(prefix)) keys.push(itemKey);
  }
  for (const itemKey of keys) {
    try {
      const id = decodeURIComponent(itemKey.slice(prefix.length));
      const raw = target.getItem(itemKey);
      if (raw === null) continue;
      const item: unknown = JSON.parse(raw);
      if (item === null) items.delete(id);
      else if (valid(item) && item.id === id) items.set(id, item);
    } catch {
      /* Preserve other valid pending work. */
    }
  }
  return [...items.values()].sort((left, right) => {
    const leftLegacy = legacyOrder.get(left.id);
    const rightLegacy = legacyOrder.get(right.id);
    // Legacy array position is authoritative even if timestamps are equal or
    // the device clock moved backwards; never derive old dependencies from time.
    if (leftLegacy !== undefined || rightLegacy !== undefined) {
      if (leftLegacy === undefined) return 1;
      if (rightLegacy === undefined) return -1;
      return leftLegacy - rightLegacy;
    }
    const leftOrder = validQueueOrder(left.queueOrder) ? left.queueOrder : undefined;
    const rightOrder = validQueueOrder(right.queueOrder) ? right.queueOrder : undefined;
    if (leftOrder === undefined && rightOrder !== undefined) return -1;
    if (rightOrder === undefined && leftOrder !== undefined) return 1;
    if (leftOrder !== undefined && rightOrder !== undefined && leftOrder !== rightOrder) {
      return leftOrder - rightOrder;
    }
    // Concurrent independent inserts can observe the same maximum. A stable
    // tie-breaker makes every tab agree without relying on Storage.key order.
    return left.queuedAt.localeCompare(right.queuedAt) || left.id.localeCompare(right.id);
  });
}

const lastQueueOrders = new Map<string, number>();

function validQueueOrder(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function nextQueueOrder(queueKey: string, queue: { queueOrder?: number }[]): number {
  const maximum = queue.reduce(
    (max, item) => (validQueueOrder(item.queueOrder) ? Math.max(max, item.queueOrder) : max),
    Math.max(queue.length, lastQueueOrders.get(queueKey) ?? 0),
  );
  if (maximum >= Number.MAX_SAFE_INTEGER) throw new Error("Offline queue order is exhausted.");
  const next = maximum + 1;
  lastQueueOrders.set(queueKey, next);
  return next;
}

function writeQueueItem<T extends { id: string }>(queueKey: string, item: T): void {
  const target = storage();
  if (!target) {
    throw new Error(
      "Offline server mutations could not be queued because local storage is unavailable.",
    );
  }
  target.setItem(`${queueKey}:item:${encodeURIComponent(item.id)}`, JSON.stringify(item));
  emitWorkspaceStatusChanged();
}

function acknowledgeQueueItem(queueKey: string, id: string): void {
  const target = storage();
  if (!target) throw new Error("Offline queue storage is unavailable.");
  // Only legacy IDs require permanent tombstones. New IDs can release their bytes.
  let legacy: unknown = [];
  try {
    legacy = JSON.parse(target.getItem(queueKey) ?? "[]");
  } catch {
    /* No readable legacy IDs. */
  }
  const itemKey = `${queueKey}:item:${encodeURIComponent(id)}`;
  if (Array.isArray(legacy) && legacy.some((item) => item?.id === id)) {
    target.setItem(itemKey, "null");
  } else {
    target.removeItem(itemKey);
  }
  emitWorkspaceStatusChanged();
}

function readMutationQueue(): QueuedServerMutation[] {
  return readQueue(MUTATION_QUEUE_KEY, isQueuedServerMutation);
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

function readAttachmentQueue(): QueuedServerAttachment[] {
  return readQueue(ATTACHMENT_QUEUE_KEY, isQueuedServerAttachment);
}

function isQueuedServerAttachment(value: unknown): value is QueuedServerAttachment {
  if (!value || typeof value !== "object") return false;
  const item = value as Partial<QueuedServerAttachment>;
  return (
    typeof item.id === "string" &&
    typeof item.serverBase === "string" &&
    typeof item.path === "string" &&
    typeof item.spaceId === "string" &&
    typeof item.documentId === "string" &&
    typeof item.name === "string" &&
    typeof item.mimeType === "string" &&
    typeof item.dataUrl === "string" &&
    typeof item.queuedAt === "string" &&
    typeof item.attempts === "number" &&
    (item.uploadOperationId === undefined || typeof item.uploadOperationId === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(item.uploadOperationId))
  );
}

export function queueOfflineServerAttachment(
  input: Omit<QueuedServerAttachment, "id" | "queuedAt" | "queueOrder" | "attempts">,
): QueuedServerAttachment {
  if(!((input.scopeVersion===undefined&&input.draftId===undefined)||(input.scopeVersion===2&&typeof input.draftId==="string"&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(input.draftId))))throw new Error("Original queued attachment draft scope invalid");
  if(input.uploadOperationId !== undefined && !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(input.uploadOperationId)) throw new Error("Upload operation identity is invalid");
  if (input.origin !== undefined && (!isOfflineReplayOrigin(input.origin) || input.origin.serverBase !== normalizedBase(input.serverBase))) throw new Error("Offline attachment origin is invalid");
  const attachment: QueuedServerAttachment = {
    ...input,
    serverBase: normalizedBase(input.serverBase),
    id: attachmentId(),
    uploadOperationId: input.uploadOperationId ?? crypto.randomUUID(),
    queueOrder: nextQueueOrder(ATTACHMENT_QUEUE_KEY, readAttachmentQueue()),
    queuedAt: new Date().toISOString(),
    attempts: 0,
  };
  writeQueueItem(ATTACHMENT_QUEUE_KEY, attachment);
  return attachment;
}

export function listOfflineServerAttachments(base?: string): QueuedServerAttachment[] {
  const normalized = base ? normalizedBase(base) : undefined;
  return readAttachmentQueue().filter((item) => !normalized || item.serverBase === normalized);
}

export function acknowledgeOfflineServerAttachment(
  id: string,
  expected?: QueuedServerAttachment,
): void {
  if (
    expected &&
    JSON.stringify(readAttachmentQueue().find((item) => item.id === id)) !==
      JSON.stringify(expected)
  )
    throw new Error("Offline attachment changed before acknowledgement");
  acknowledgeQueueItem(ATTACHMENT_QUEUE_KEY, id);
}

export function updateOfflineServerAttachment(item: QueuedServerAttachment, expected?: QueuedServerAttachment): void {
  const current = readAttachmentQueue().find(entry => entry.id === item.id);
  if (expected && JSON.stringify(current) !== JSON.stringify(expected)) throw new Error("Offline queue item changed during replay");
  if (current && current.uploadOperationId !== item.uploadOperationId) throw new Error("Upload operation identity cannot change after enqueue");
  if (!current || (current.origin || item.origin) && !sameOfflineReplayOrigin(current.origin, item.origin)) throw new Error("Offline attachment origin changed");
  if (current.scopeVersion!==item.scopeVersion || current.draftId!==item.draftId || current.serverBase !== item.serverBase || current.path !== item.path || current.spaceId !== item.spaceId || current.documentId !== item.documentId || current.dataUrl !== item.dataUrl || current.name !== item.name || current.mimeType !== item.mimeType || current.queuedAt !== item.queuedAt || current.queueOrder !== item.queueOrder) throw new Error("Offline attachment identity changed");
  writeQueueItem(ATTACHMENT_QUEUE_KEY, item);
}

export function updateCachedServerWorkspaceForMutation(
  base: string,
  path: string,
  method: string,
  body: unknown,
  scope: string,
): void {
  const documentCollectionMatch = path.match(/^\/spaces\/([^/]+)\/documents$/);
  if (method === "POST" && documentCollectionMatch && body && typeof body === "object") {
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
      base,
      `/spaces/${spaceId}/documents`,
      scope,
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
  if (method === "POST" && folderCollectionMatch && body && typeof body === "object") {
    const [, spaceId] = folderCollectionMatch;
    const input = body as Record<string, unknown>;
    if (typeof input.source_id !== "string" || typeof input.name !== "string") return;
    const cachedList = readCachedServerResponse<Record<string, unknown>[]>(
      base,
      `/spaces/${spaceId}/folders`,
      scope,
    );
    if (cachedList) {
      cacheServerResponse(
        base,
        `/spaces/${spaceId}/folders`,
        [
          ...cachedList,
          { id: input.source_id, name: input.name, parent_id: input.parent_id ?? undefined },
        ],
        scope,
      );
    }
    return;
  }

  const contentMatch = path.match(/^\/spaces\/([^/]+)\/documents\/([^/]+)\/content$/);
  if (
    method === "PUT" &&
    contentMatch &&
    body &&
    typeof body === "object" &&
    typeof (body as { content?: unknown }).content === "string"
  ) {
    const [, spaceId, documentId] = contentMatch;
    const cached = readCachedServerResponse<Record<string, unknown>>(
      base,
      `/spaces/${spaceId}/documents/${documentId}/content`,
      scope,
    );
    if (cached) {
      cacheServerResponse(
        base,
        `/spaces/${spaceId}/documents/${documentId}/content`,
        {
          ...cached,
          content: (body as { content: string }).content,
        },
        scope,
      );
    }
    return;
  }

  const documentMatch = path.match(/^\/spaces\/([^/]+)\/documents\/([^/]+)$/);
  if (documentMatch) {
    const [, spaceId, documentId] = documentMatch;
    const cached = readCachedServerResponse<Record<string, unknown>>(
      base,
      `/spaces/${spaceId}/documents/${documentId}`,
      scope,
    );
    if (cached && body && typeof body === "object") {
      cacheServerResponse(
        base,
        `/spaces/${spaceId}/documents/${documentId}`,
        {
          ...cached,
          ...(body as Record<string, unknown>),
        },
        scope,
      );
    }
    const cachedList = readCachedServerResponse<Record<string, unknown>[]>(
      base,
      `/spaces/${spaceId}/documents`,
      scope,
    );
    if (cachedList) {
      cacheServerResponse(
        base,
        `/spaces/${spaceId}/documents`,
        method === "DELETE"
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
      base,
      `/spaces/${spaceId}/folders`,
      scope,
    );
    if (cachedList) {
      cacheServerResponse(
        base,
        `/spaces/${spaceId}/folders`,
        method === "DELETE"
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
  origin?: OfflineReplayOrigin,
  replayState?: "unknown",
  deletionOperationId?: string,
): QueuedServerMutation {
  if (
    origin !== undefined &&
    (!isOfflineReplayOrigin(origin) || origin.serverBase !== normalizedBase(base))
  )
    throw new Error("Offline mutation origin is invalid");
  const mutation: QueuedServerMutation = {
    id: mutationId(),
    ...(deletionOperationId ? { deletionOperationId } : {}),
    queueOrder: nextQueueOrder(MUTATION_QUEUE_KEY, readMutationQueue()),
    ...(origin ? { origin: { ...origin } } : {}),
    ...(replayState
      ? { replayState, lastError: "Server outcome unknown; explicit review required" }
      : {}),
    serverBase: normalizedBase(base),
    path,
    method,
    body,
    queuedAt: new Date().toISOString(),
    attempts: 0,
  };
  writeQueueItem(MUTATION_QUEUE_KEY, mutation);
  if (replayState !== "unknown")
    updateCachedServerWorkspaceForMutation(base, path, method, body, scope);
  return mutation;
}

export function listOfflineServerMutations(base?: string): QueuedServerMutation[] {
  const normalized = base ? normalizedBase(base) : undefined;
  return readMutationQueue().filter((item) => !normalized || item.serverBase === normalized);
}

export function acknowledgeOfflineServerMutation(
  id: string,
  expected?: QueuedServerMutation,
): void {
  if (
    expected &&
    JSON.stringify(readMutationQueue().find((item) => item.id === id)) !== JSON.stringify(expected)
  )
    throw new Error("Offline mutation changed before acknowledgement");
  acknowledgeQueueItem(MUTATION_QUEUE_KEY, id);
}

export function updateOfflineServerMutation(item: QueuedServerMutation, expected?: QueuedServerMutation): void {
  const current = readMutationQueue().find(entry => entry.id === item.id);
  if (expected && JSON.stringify(current) !== JSON.stringify(expected)) throw new Error("Offline queue item changed during replay");
  if (!current || (current.origin || item.origin) && !sameOfflineReplayOrigin(current.origin, item.origin)) throw new Error("Offline mutation origin changed");
  if (current.serverBase !== item.serverBase || current.path !== item.path || current.method !== item.method || current.queuedAt !== item.queuedAt || current.queueOrder !== item.queueOrder || current.deletionOperationId !== item.deletionOperationId || (current.deletionOperationId !== undefined && JSON.stringify(current.body) !== JSON.stringify(item.body))) throw new Error("Offline mutation identity changed");
  writeQueueItem(MUTATION_QUEUE_KEY, item);
}

/** Retain captured dispatched bytes without changing the newer workspace cache. */
export function retainUnknownOfflineServerMutation(
  original: QueuedServerMutation,
  reason: string,
): void {
  const evidence: QueuedServerMutation = {
    ...original,
    id: mutationId(),
    queueOrder: nextQueueOrder(MUTATION_QUEUE_KEY, readMutationQueue()),
    replayState: "unknown",
    lastError: reason,
  };
  writeQueueItem(MUTATION_QUEUE_KEY, evidence);
}
export function retainUnknownOfflineServerAttachment(
  original: QueuedServerAttachment,
  reason: string,
): void {
  const evidence: QueuedServerAttachment = {
    ...original,
    id: attachmentId(),
    queueOrder: nextQueueOrder(ATTACHMENT_QUEUE_KEY, readAttachmentQueue()),
    replayState: "unknown",
    lastError: reason,
  };
  writeQueueItem(ATTACHMENT_QUEUE_KEY, evidence);
}
