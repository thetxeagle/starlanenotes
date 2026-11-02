import {validateUploadAudience} from "./uploadOperationStore";
import { cleanupAttemptFromAcknowledgedMutation, retainAcknowledgedCleanup } from "./permanentDocumentCleanup";
import { canonicalDeleteOperation, DELETE_OPERATION_HEADER, deleteRoute, originalDeleteTarget } from "./documentDeleteIntent";
import { getCurrentLaunchMode } from "./launchMode";
import { getStorageSelection } from "./storageProviders";
import { captureOfflineReplayOrigin, verifyOfflineReplayContext, sameOfflineReplayOrigin, offlineReplayTokenMatchesOrigin, type OfflineReplayContext } from "./offlineReplayOrigin";
import {
  cacheServerResponse,
  clearServerCacheSessionScope,
  listOfflineServerAttachments,
  listOfflineServerMutations,
  acknowledgeOfflineServerAttachment,
  updateOfflineServerAttachment,
  queueOfflineServerMutation,
  acknowledgeOfflineServerMutation,
  updateOfflineServerMutation,
  retainUnknownOfflineServerMutation,
  retainUnknownOfflineServerAttachment,
  serverCacheScopeForSession,
  type QueuedServerAttachment,
  readCachedServerResponse,
  updateCachedServerWorkspaceForMutation,
} from "@/lib/offlineServerCache";

function isTauri(): boolean {
  if (typeof window === "undefined") return false;
  return "__TAURI_INTERNALS__" in window;
}

function getApiBase(): string {
  if (isTauri()) {
    const serverUrl = localStorage.getItem("server_url");
    if (serverUrl) {
      return `${serverUrl.replace(/\/+$/, "")}/api/v1`;
    }
  }
  return "/api/v1";
}

export { isTauri, getApiBase };

export interface SessionClientDetails {
  session_kind: "browser" | "app";
  device_name: string;
  client_platform: "web" | "mobile_app" | "desktop_app";
}

export function getSessionClientDetails(): SessionClientDetails {
  const userAgent = typeof navigator === "undefined" ? "" : navigator.userAgent;
  const isMobile = /android|iphone|ipad|ipod/i.test(userAgent);
  if (isTauri()) {
    return {
      session_kind: "app",
      device_name: isMobile ? "Starlane Notes on mobile" : "Starlane Notes desktop app",
      client_platform: isMobile ? "mobile_app" : "desktop_app",
    };
  }

  const browser = /firefox/i.test(userAgent)
    ? "Firefox"
    : /edg/i.test(userAgent)
      ? "Microsoft Edge"
      : /chrome|crios/i.test(userAgent)
        ? "Chrome"
        : /safari/i.test(userAgent)
          ? "Safari"
          : "Browser";
  const device = /iphone/i.test(userAgent)
    ? "iPhone"
    : /ipad/i.test(userAgent)
      ? "iPad"
      : /android/i.test(userAgent)
        ? "Android device"
        : "desktop";
  return {
    session_kind: "browser",
    device_name: `${browser} on ${device}`,
    client_platform: "web",
  };
}

interface ApiOptions {
  method?: string;
  body?: unknown;
  headers?: Record<string, string>;
  expectedStatuses?: readonly number[];
}

// One scoped mutation contract, shared by initial dispatch and queued REST replay.
// The digest is a server-bound source; client validation does not manufacture it.
function sourceBoundDraftPath(method: string, path: string, body: unknown): RegExpExecArray | null {
  if (method !== "PUT" || !body || typeof body !== "object" || Array.isArray(body) || !Object.prototype.hasOwnProperty.call(body, "source_digest")) return null;
  return /^\/spaces\/([^/]+)\/documents\/([^/]+)\/drafts\/([^/?]+)$/.exec(path);
}
function confirmsDraftSave(path: RegExpExecArray, body: unknown, value: unknown, actor?: string): boolean {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>, patch = body as Record<string, unknown>;
  if (typeof patch.source_digest !== "string" || !/^[0-9a-f]{64}$/.test(patch.source_digest)) return false;
  if (row.space_id !== path[1] || row.document_id !== path[2] || row.id !== path[3] || typeof row.source_digest !== "string" || !/^[0-9a-f]{64}$/.test(row.source_digest)) return false;
  // A partial patch still requires a complete, scoped saved-draft acknowledgement.
  if (typeof row.title !== "string" || typeof row.content !== "string" || typeof row.document_title !== "string" || typeof row.owner_id !== "string" || !row.owner_id || typeof row.owner_name !== "string" || !["owner","editor"].includes(String(row.access_role)) || typeof row.can_publish !== "boolean" || row.can_publish && row.access_role !== "owner" || actor && row.access_role === "owner" && row.owner_id !== actor || !["created_at","updated_at","base_updated_at"].every(key => typeof row[key] === "string" && !!row[key]) || ![row.icon,row.cover_image].every(field => field === undefined || field === null || typeof field === "string")) return false;
  for (const field of ["title", "icon", "cover_image", "content"] as const) {
    if (patch[field] === undefined || patch[field] === null) continue;
    if (typeof patch[field] !== "string") return false;
    const input = patch[field];
    const expected = field === "title" ? input.trim() : (field === "icon" || field === "cover_image") && !input.trim() ? null : input;
    if (row[field] !== expected) return false;
  }
  return true;
}

interface ApiErrorBody {
  error?: unknown;
  message?: unknown;
}

type TokenRefreshResult = "refreshed" | "rejected" | "unavailable";

export class ApiError extends Error {
  readonly status: number;
  readonly code?: string;

  constructor(message: string, status: number, code?: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

export function isApiError(error: unknown): error is ApiError {
  return error instanceof ApiError;
}

function normalizeApiErrorBody(body: unknown): { code?: string; message?: string } {
  if (!body || typeof body !== "object") return {};

  const { error, message } = body as ApiErrorBody;
  return {
    code: typeof error === "string" && error.trim() ? error : undefined,
    message: typeof message === "string" && message.trim() ? message : undefined,
  };
}

function isPublicAuthFlowPath(path: string): boolean {
  return (
    path === "/auth/login" ||
    path === "/auth/register" ||
    path === "/auth/refresh" ||
    path === "/auth/password-reset/request" ||
    path === "/auth/password-reset/confirm" ||
    path === "/auth/passkeys/login/start" ||
    path === "/auth/passkeys/login/finish"
  );
}

function shouldAttemptTokenRefresh(
  path: string,
  token: string | null,
  status: number,
  error: { code?: string; message?: string },
): boolean {
  if (status !== 401 || !token || isPublicAuthFlowPath(path)) return false;

  const message = error.message?.toLowerCase() ?? "";
  return (
    message.includes("invalid or expired token") || message.includes("missing authentication token")
  );
}

class ApiClient {
  private accessToken: string | null = null;
  private refreshPromise: Promise<TokenRefreshResult> | null = null;
  private refreshOwner: string | null = null;
  private readonly inFlightGets = new Map<string, Promise<unknown>>();

  setToken(token: string | null) {
    this.accessToken = token;
    if (token) {
      localStorage.setItem("access_token", token);
    } else {
      localStorage.removeItem("access_token");
    }
  }

  getToken(): string | null {
    if (!this.accessToken) {
      this.accessToken = localStorage.getItem("access_token");
    }
    return this.accessToken;
  }

  setRefreshToken(token: string | null) {
    if (token) {
      localStorage.setItem("refresh_token", token);
    } else {
      localStorage.removeItem("refresh_token");
    }
  }

  getRefreshToken(): string | null {
    return localStorage.getItem("refresh_token");
  }

  async fetch<T>(path: string, options: ApiOptions = {}): Promise<T> {
    const method = options.method ?? "GET";
    if (deleteRoute(method, path)) throw new Error("Destructive deletion requires its retained original operation and page identity; review the original page.");
    if (method !== "GET") return this.fetchUncached<T>(path, options);

    const token = this.getToken();
    const cacheScope = serverCacheScopeForSession(token, this.getRefreshToken());
    const key = `${getApiBase()}|${cacheScope}|${path}`;
    const existing = this.inFlightGets.get(key);
    if (existing) return existing as Promise<T>;

    const request = this.fetchUncached<T>(path, options);
    this.inFlightGets.set(key, request);
    const clearRequest = () => {
      if (this.inFlightGets.get(key) === request) this.inFlightGets.delete(key);
    };
    void request.then(clearRequest, clearRequest);
    return request;
  }

  private async fetchUncached<T>(path: string, options: ApiOptions = {}): Promise<T> {
    const { method = "GET", body: inputBody, headers = {} } = options;
    const expectedStatuses = options.expectedStatuses ? [...options.expectedStatuses] : undefined;
    const isFormData = typeof FormData !== "undefined" && inputBody instanceof FormData;
    // Retain the dispatched JSON bytes even if the caller edits its object while awaiting.
    const body = inputBody && !isFormData ? JSON.parse(JSON.stringify(inputBody)) : inputBody;
    const draftSavePath = sourceBoundDraftPath(method, path, body);

    const token = this.getToken();
    if (token && !isPublicAuthFlowPath(path)) {
      headers["Authorization"] = `Bearer ${token}`;
    }

    if (body && !isFormData) {
      headers["Content-Type"] = "application/json";
    }

    const apiBase = getApiBase();
    const originalRefresh = this.getRefreshToken();
    const cacheScope = serverCacheScopeForSession(token, originalRefresh);
    // Capture BEFORE the network wait. Failed A requests cannot become B work.
    let offlineContext: OfflineReplayContext | null = null;
    try { offlineContext = captureOfflineReplayOrigin(apiBase, token, cacheScope); } catch { /* Online requests still use their ordinary server authority. */ }
    const supportsOfflineCreate = method === "POST" && /^\/spaces\/[^/]+\/(documents|folders)$/.test(path) &&
      body && typeof body === "object" && typeof (body as { source_id?: unknown }).source_id === "string";
    const supportsOfflineMutation = !isFormData && path.startsWith("/spaces/") &&
      (method === "PUT" || method === "DELETE" || supportsOfflineCreate);
    const originalMode = getCurrentLaunchMode(), originalSelection = getStorageSelection();
    const verifyOriginalMutation = () => {
      if (!supportsOfflineMutation) return;
      if (offlineContext) verifyOfflineReplayContext(offlineContext, getApiBase(), this.getToken());
      else if (getApiBase() !== apiBase || this.getToken() !== token || localStorage.getItem("access_token") !== token ||
        this.getRefreshToken() !== originalRefresh || getCurrentLaunchMode() !== originalMode ||
        JSON.stringify(getStorageSelection()) !== JSON.stringify(originalSelection)) throw new Error("Original mutation context changed");
    };
    const retainOfflineMutation = (unknown: boolean) => {
      if (!offlineContext) throw new Error("Offline work lacks original authenticated provenance");
      if (!unknown) verifyOfflineReplayContext(offlineContext, getApiBase(), this.getToken());
      queueOfflineServerMutation(apiBase, path, method as "POST" | "PUT" | "DELETE", body,
        cacheScope, offlineContext.origin, unknown ? "unknown" : undefined);
      // Retention uses the captured origin only; drift is reported after unknown bytes
      // are durable, without writing the current workspace cache.
      if (unknown) verifyOfflineReplayContext(offlineContext, getApiBase(), this.getToken());
      // The delayed native mirror selects a vault after awaits and cannot carry this
      // original context. Retain browser intent; native reopen parity is deferred.
    };
    if (supportsOfflineMutation && typeof navigator !== "undefined" && navigator.onLine === false) {
      // Deliberately do not dispatch: this is known-unsent work, unlike fetch rejection.
      retainOfflineMutation(false);
      return undefined as T;
    }
    let response: Response;
    try {
      response = await fetch(`${apiBase}${path}`, {
        method,
        headers,
        body: body ? (isFormData ? body : JSON.stringify(body)) : undefined,
      });
      verifyOriginalMutation();
    } catch (error) {
      if (method === "GET") {
        const cached =
          readCachedServerResponse<T>(apiBase, path, cacheScope) ??
          (path === "/auth/config" ? readCachedServerResponse<T>(apiBase, path) : null);
        if (cached !== null) return cached;
      }

      if (supportsOfflineMutation && offlineContext) {
        // A dispatched rejection cannot prove that the server did not commit.
        retainOfflineMutation(true);
        return undefined as T;
      }

      throw error;
    }

    if (!response.ok) {
      const errorBody = await response.json().catch(() => ({ message: "Request failed" }));
      const { code, message } = normalizeApiErrorBody(errorBody);
      if (shouldAttemptTokenRefresh(path, token, response.status, { code, message })) {
        const verifyRefreshOrigin = () => {
          if (offlineContext) verifyOfflineReplayContext(offlineContext, getApiBase(), this.getToken());
          else if (getApiBase() !== apiBase || this.getToken() !== token || localStorage.getItem("access_token") !== token || this.getRefreshToken() !== originalRefresh) throw new Error("Original request context changed during refresh");
        };
        verifyRefreshOrigin();
        const refreshResult = await this.tryRefresh(apiBase, token, originalRefresh, offlineContext, verifyRefreshOrigin);
        if (refreshResult === "refreshed") {
          if (getApiBase() !== apiBase) throw new Error("Original request server changed during refresh");
          if (offlineContext) {
            const rotated = captureOfflineReplayOrigin(apiBase, this.getToken(), cacheScope);
            if (!sameOfflineReplayOrigin(rotated.origin, offlineContext.origin)) throw new Error("Original request identity changed during refresh");
          }
          // Retry the copied dispatched body, never the caller's now-editable object.
          return this.fetchUncached(path, { ...options, body, expectedStatuses });
        }
        if (refreshResult === "rejected") {
          verifyRefreshOrigin();
          this.logout();
          if (typeof window !== "undefined") window.location.href = "/login";
          throw new ApiError("Session expired", 401, "session_expired");
        }
        if (method === "GET") {
          const cached = readCachedServerResponse<T>(apiBase, path, cacheScope);
          if (cached !== null) return cached;
        }
        throw new ApiError(
          "Session refresh is temporarily unavailable",
          503,
          "session_refresh_unavailable",
        );
      }
      throw new ApiError(message || `HTTP ${response.status}`, response.status, code);
    }

    if (draftSavePath ? response.status !== 200 : expectedStatuses && !expectedStatuses.includes(response.status)) {
      // A successful HTTP status is not proof of this caller's exact outcome.
      // Retain the original intent before rejecting, without cache or mirror effects.
      if (supportsOfflineMutation && offlineContext) retainOfflineMutation(true);
      throw new ApiError("Could not confirm this save. Your edits are retained for review.", response.status, "outcome_unproven");
    }

    if (response.status === 204) {
      verifyOriginalMutation();
      if (method !== "GET") {
        updateCachedServerWorkspaceForMutation(apiBase, path, method, body, cacheScope);
        if (!supportsOfflineMutation) void import("@/lib/serverVaultMirror")
          .then(({ queueServerMutationForVaultMirror }) =>
            queueServerMutationForVaultMirror(
              apiBase,
              path,
              method as "POST" | "PUT" | "DELETE",
              body,
            ),
          )
          .catch(() => undefined);
      }
      return undefined as T;
    }

    let data: T;
    try {
      const responseText = await response.text();
      verifyOriginalMutation();
      data = (responseText.trim() ? JSON.parse(responseText) : undefined) as T;
      verifyOriginalMutation();
      if (draftSavePath && !confirmsDraftSave(draftSavePath, body, data, offlineContext?.origin.accountId)) throw new ApiError("Could not confirm this draft save. Your edits are retained for review.", response.status, "outcome_unproven");
    } catch (error) {
      if (supportsOfflineMutation && offlineContext) retainOfflineMutation(true);
      throw error;
    }
    if (method === "GET") {
      cacheServerResponse(apiBase, path, data, cacheScope);
    } else {
      updateCachedServerWorkspaceForMutation(apiBase, path, method, body, cacheScope);
    }
    if (!supportsOfflineMutation) void import("@/lib/serverVaultMirror")
      .then(({ queueServerResponseForVaultMirror }) =>
        queueServerResponseForVaultMirror(apiBase, path, data),
      )
      .catch(() => undefined);
    return data;
  }

  private tryRefresh(base: string, bearer: string | null, refreshToken: string | null,
    context: OfflineReplayContext | null, guard: () => void): Promise<TokenRefreshResult> {
    const owner = JSON.stringify([base, bearer, refreshToken]);
    if (this.refreshPromise) {
      if (this.refreshOwner !== owner) return Promise.resolve("unavailable");
      return this.refreshPromise;
    }
    this.refreshOwner = owner;
    this.refreshPromise = this.performTokenRefresh(base, refreshToken, context, guard).finally(() => {
      this.refreshPromise = null; this.refreshOwner = null;
    });
    return this.refreshPromise;
  }

  private async performTokenRefresh(base: string, refreshToken: string | null,
    context: OfflineReplayContext | null, guard: () => void): Promise<TokenRefreshResult> {
    if (!refreshToken) return "rejected";
    try {
      guard();
      const response = await fetch(`${base}/auth/refresh`, {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ refresh_token: refreshToken }),
      });
      guard();
      if (response.status === 401 || response.status === 403) return "rejected";
      if (!response.ok) return "unavailable";
      const data = (await response.json()) as { access_token?: unknown; refresh_token?: unknown };
      guard();
      if (typeof data.access_token !== "string" || !data.access_token ||
        typeof data.refresh_token !== "string" || !data.refresh_token ||
        context && !offlineReplayTokenMatchesOrigin(data.access_token, context.origin)) return "unavailable";
      this.setToken(data.access_token); this.setRefreshToken(data.refresh_token);
      return "refreshed";
    } catch { return "unavailable"; }
  }

  /** Exact original deletion, without refresh, fallback queue or current-target rebasing. */
  async fetchDocumentDelete(context: OfflineReplayContext, path: string, operation: string, body: unknown): Promise<void> {
    if (!deleteRoute("DELETE", path) || !canonicalDeleteOperation(operation) || !originalDeleteTarget(body)) throw new Error("Original deletion intent is malformed; retain it for review");
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    const response = await fetch(`${context.origin.serverBase}${path}`, { method: "DELETE", headers: { Authorization: `Bearer ${context.bearer}`, "Content-Type": "application/json", [DELETE_OPERATION_HEADER]: operation }, body: JSON.stringify(body) });
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    if (response.status !== 204 || response.headers.get(DELETE_OPERATION_HEADER) !== operation) throw new Error("Deletion outcome is unproven; retain the original operation and review before retrying");
  }

  /** One captured-context upload, with no refresh/foreign-authority redispatch. */
  async fetchOriginalContext<T>(context: OfflineReplayContext, path: string, options: ApiOptions): Promise<T> {
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    const isForm = typeof FormData !== "undefined" && options.body instanceof FormData;
    const headers = { ...options.headers, Authorization: `Bearer ${context.bearer}` };
    if (options.body !== undefined && !isForm) Object.assign(headers, { "Content-Type": "application/json" });
    const response = await fetch(`${context.origin.serverBase}${path}`, {
      method: options.method ?? "GET",
      headers,
      body: isForm ? options.body as FormData : options.body === undefined ? undefined : JSON.stringify(options.body),
    });
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    if (!response.ok) {
      const body = await response.json().catch(() => ({ message: "Request failed" }));
      verifyOfflineReplayContext(context, getApiBase(), this.getToken());
      const { code, message } = normalizeApiErrorBody(body);
      throw new ApiError(message || `HTTP ${response.status}`, response.status, code);
    }
    const text = await response.text();
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    return (text.trim() ? JSON.parse(text) : undefined) as T;
  }

  /** Explicit birth only: one captured request, no refresh, cache, offline queue,
   * or deferred current-vault mirror. Caller journals the immutable intent. */
  async fetchDraftBirth<T>(context: OfflineReplayContext, path: string, body: unknown, validate: (value: unknown, status: number) => T): Promise<T> {
    if (!/^\/spaces\/[^/?#]+\/documents\/[^/?#]+\/drafts$/.test(path)) throw new Error("Invalid draft birth route");
    const request = JSON.stringify(body);
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    const response = await fetch(`${context.origin.serverBase}${path}`, { method: "POST", headers: { Authorization: `Bearer ${context.bearer}`, "Content-Type": "application/json" }, body: request });
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    const text = await response.text();
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    if (response.status !== 200 && response.status !== 201) {
      if (response.ok) throw new ApiError("Draft birth outcome is unproven; review the original request", response.status, "birth_outcome_unproven");
      let decoded: unknown; try { decoded = JSON.parse(text); } catch { decoded = { message: "Draft request failed" }; }
      const { code, message } = normalizeApiErrorBody(decoded);
      throw new ApiError(message || `HTTP ${response.status}`, response.status, code);
    }
    const decoded: unknown = JSON.parse(text);
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    const result = validate(decoded, response.status);
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    return result;
  }

  /** Fresh owner lifecycle request: one captured authority, exact200/204.
   * No queue/cache/refresh or derived native mirror can report an ACK. */
  async requestDraftOwner<T = void>(path: string, method: "GET" | "PUT" | "DELETE", body?: { role: "viewer" | "editor" }): Promise<T> {
    const draft = /^\/spaces\/[^/?#]+\/documents\/[^/?#]+\/drafts\/[^/?#]+$/;
    const access = /^\/spaces\/[^/?#]+\/documents\/[^/?#]+\/drafts\/[^/?#]+\/access\/[^/?#]+$/;
    const catalog = /^\/spaces\/[^/?#]+\/documents\/[^/?#]+\/drafts\/[^/?#]+\/(access|access-candidates)$/;
    if (!(method === "GET" ? catalog.test(path) : method === "PUT" ? access.test(path) : draft.test(path) || access.test(path))) throw new Error("Invalid owner draft management route");
    const context = captureOfflineServerContext();
    const request = body === undefined ? undefined : JSON.stringify(body);
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    if (typeof navigator !== "undefined" && navigator.onLine === false) throw new Error("Draft owner request was not dispatched. Keep the selection and retry when connected.");
    let response: Response;
    let text: string;
    try {
      response = await fetch(`${context.origin.serverBase}${path}`, { method, headers: { Authorization: `Bearer ${context.bearer}`, ...(request === undefined ? {} : { "Content-Type": "application/json" }) }, body: request });
      verifyOfflineReplayContext(context, getApiBase(), this.getToken());
      text = await response.text();
      verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    } catch {
      throw new Error(method === "GET" ? "Draft participants could not be inspected. Existing selections remain open." : "Owner draft management outcome could not be confirmed. Keep the selected draft and review its state before retrying.");
    }
    if (!response.ok) {
      let decoded: unknown; try { decoded = JSON.parse(text); } catch { decoded = { message: "Owner draft management failed" }; }
      const { code, message } = normalizeApiErrorBody(decoded);
      throw new ApiError(message || `HTTP ${response.status}`, response.status, code);
    }
    const expected = method === "GET" ? 200 : 204;
    if (response.status !== expected || method !== "GET" && text.trim()) throw new ApiError("Owner draft response is unproven. Keep the selection and review its state.", response.status, "outcome_unproven");
    const value: unknown = method === "GET" ? JSON.parse(text) : undefined;
    if (method === "GET" && (!Array.isArray(value) || value.some((entry: unknown) => {
      if (!entry || typeof entry !== "object" || Array.isArray(entry)) return true;
      const row = entry as Record<string, unknown>;
      return typeof row.user_id !== "string" || !row.user_id.trim() || typeof row.display_name !== "string" || typeof row.email !== "string" || !path.endsWith("/access-candidates") && row.role !== "viewer" && row.role !== "editor";
    }))) throw new Error("Draft participant response is unavailable; keep the current selection");
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    return value as T;
  }

  /** Whole-space purge is a fresh request, never an offline queued DELETE. */
  async requestDraftPurge(spaceId: string): Promise<number> {
    if (!spaceId || /[/?#]/.test(spaceId)) throw new Error("Invalid draft purge space");
    const context = captureOfflineServerContext();
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    if (typeof navigator !== "undefined" && navigator.onLine === false) throw new Error("Draft purge was not dispatched. Keep the drafts and reconnect before reviewing this action.");
    let response: Response;
    let text: string;
    try {
      response = await fetch(`${context.origin.serverBase}/spaces/${spaceId}/drafts`, { method: "DELETE", headers: { Authorization: `Bearer ${context.bearer}` } });
      verifyOfflineReplayContext(context, getApiBase(), this.getToken());
      text = await response.text();
      verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    } catch {
      throw new Error("Draft purge outcome could not be confirmed. Keep the current rows and reload Drafts before reviewing another purge.");
    }
    if (!response.ok) {
      let decoded: unknown; try { decoded = JSON.parse(text); } catch { decoded = { message: "Draft purge failed" }; }
      const { code, message } = normalizeApiErrorBody(decoded);
      throw new ApiError(message || `HTTP ${response.status}`, response.status, code);
    }
    let value: unknown; try { value = JSON.parse(text); } catch { value = null; }
    if (response.status !== 200 || !value || typeof value !== "object" || Array.isArray(value) ||
        !Number.isSafeInteger((value as { deleted: number }).deleted) || (value as { deleted: number }).deleted < 0) {
      throw new ApiError("Draft purge response is unproven. Keep the current rows and reload Drafts before reviewing another purge.", response.status, "outcome_unproven");
    }
    verifyOfflineReplayContext(context, getApiBase(), this.getToken());
    return (value as { deleted: number }).deleted;
  }

  logout() {
    this.setToken(null);
    this.setRefreshToken(null);
    clearServerCacheSessionScope();
  }

  async logoutOnServer(): Promise<void> {
    const token = this.getToken();
    this.logout();
    if (!token) return;

    try {
      await fetch(`${getApiBase()}/auth/logout`, {
        method: "POST",
        headers: { Authorization: `Bearer ${token}` },
      });
    } catch {
      // Local credentials must clear even if the server cannot be reached.
    }
  }

  isAuthenticated(): boolean {
    return !!this.getToken();
  }
}

export const api = new ApiClient();

export interface OfflineServerMutationReplayResult {
  applied: number;
  pending: number;
  blocked: number;
}

function dataUrlToBlob(dataUrl: string, mimeType: string): Blob | null {
  const match = dataUrl.match(/^data:([^;,]+)?(;base64)?,(.*)$/s);
  if (!match) return null;
  const encoded = match[3] ?? "";
  try {
    if (match[2]) {
      const binary = atob(encoded);
      const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
      return new Blob([bytes], { type: match[1] || mimeType });
    }
    return new Blob([decodeURIComponent(encoded)], { type: match[1] || mimeType });
  } catch {
    return null;
  }
}

export function captureOfflineServerContext(): OfflineReplayContext {
  const bearer = api.getToken();
  return captureOfflineReplayOrigin(getApiBase(), bearer, serverCacheScopeForSession(bearer, api.getRefreshToken()));
}
function verifyContext(context: OfflineReplayContext): void {
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
}
function replaceQueuedAttachmentReferences(attachment: QueuedServerAttachment, uploadedUrl: string, context: OfflineReplayContext): void {
  verifyContext(context);
  const pagePrefix = `/spaces/${attachment.spaceId}/documents/${attachment.documentId}`;
  for (const mutation of listOfflineServerMutations(attachment.serverBase)) {
    const draftPath = mutation.path.match(/^\/spaces\/([^/]+)\/documents\/([^/]+)\/drafts\/([^/?#]+)$/);
    const samePageDraft = draftPath?.[1] === attachment.spaceId && draftPath?.[2] === attachment.documentId;
    if(attachment.scopeVersion===2&&(!samePageDraft||draftPath?.[3]!==attachment.draftId))continue;
    if (!sameOfflineReplayOrigin(mutation.origin, attachment.origin) || mutation.method !== "PUT" ||
      !(mutation.path === `${pagePrefix}/content` || samePageDraft) ||
      !mutation.body || typeof mutation.body !== "object") continue;
    const body = mutation.body as Record<string, unknown>;
    if (typeof body.content !== "string" || !body.content.includes(attachment.dataUrl)) continue;
    if (samePageDraft && mutation.replayState !== undefined) throw new Error("Held original draft bytes cannot be rewritten by an attachment outcome");
    verifyContext(context);
    updateOfflineServerMutation({ ...mutation, body: { ...body, content: body.content.split(attachment.dataUrl).join(uploadedUrl) } }, mutation);
  }
}

const activeReplays = new Map<string, Promise<OfflineServerMutationReplayResult>>();
export function replayOfflineServerMutations(base = getApiBase()): Promise<OfflineServerMutationReplayResult> {
  const normalized = base.replace(/\/+$/, "");
  const active = activeReplays.get(normalized);
  if (active) return active;
  // Do not create or re-enroll a cache partition when inspecting legacy work.
  const pendingCount = () => listOfflineServerMutations(normalized).length + listOfflineServerAttachments(normalized).length;
  if (!pendingCount()) return Promise.resolve({ applied: 0, pending: 0, blocked: 0 });
  let context: OfflineReplayContext;
  try {
    const seed = localStorage.getItem("starlane_server_cache_v1:session-scope");
    context = captureOfflineReplayOrigin(getApiBase(), api.getToken(), seed ? `session-${seed}` : "public");
    if (context.origin.serverBase !== normalized) throw new Error("Server changed");
  } catch { return Promise.resolve({ applied: 0, pending: pendingCount(), blocked: pendingCount() }); }
  const run = async () => {
    if (typeof navigator !== "undefined" && navigator.locks) {
      return navigator.locks.request(`starlane:offline-replay:${normalized}`, () => replayQueue(normalized, context));
    }
    // Browser/Tauri fallback cannot prove cross-client serialization.
    try { verifyContext(context); } catch { return { applied: 0, pending: pendingCount(), blocked: pendingCount() }; }
    const lastError = "Offline replay requires Web Locks; use a supported HTTPS browser to review and sync pending work.";
    for (const item of listOfflineServerMutations(normalized)) if (sameOfflineReplayOrigin(item.origin, context.origin)) updateOfflineServerMutation({ ...item, lastError });
    for (const item of listOfflineServerAttachments(normalized)) if (sameOfflineReplayOrigin(item.origin, context.origin)) updateOfflineServerAttachment({ ...item, lastError });
    return { applied: 0, pending: pendingCount(), blocked: pendingCount() };
  };
  const pending = run().finally(() => activeReplays.delete(normalized));
  activeReplays.set(normalized, pending);
  return pending;
}
async function replayQueue(base: string, context: OfflineReplayContext): Promise<OfflineServerMutationReplayResult> {
  const count = () => listOfflineServerMutations(base).length + listOfflineServerAttachments(base).length;
  let applied = 0;
  const result = () => ({ applied, pending: count(), blocked: count() });
  try { verifyContext(context); } catch { return result(); }
  // Pre-upgrade failed draft requests have no durable outcome classification.
  for (const item of listOfflineServerMutations(base)) {
    if (item.replayState === undefined && (item.attempts > 0 || item.lastError) && sourceBoundDraftPath(item.method, item.path, item.body)) {
      if (!sameOfflineReplayOrigin(item.origin, context.origin)) return result();
      updateOfflineServerMutation({ ...item, replayState: "held", hold: { schemaVersion: 1, reason: "outcome_unproven", message: "Legacy draft outcome requires deliberate review" } }, item);
    }
  }
  for(const item of listOfflineServerAttachments(base)){
    const validScope=(item.scopeVersion===undefined&&item.draftId===undefined)||(item.scopeVersion===2&&typeof item.draftId==="string"&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(item.draftId));
    if(!validScope){updateOfflineServerAttachment({...item,replayState:"unknown",lastError:"Original attachment draft scope is invalid; retain its original bytes for review"},item);return result();}
  }
  // Held and unknown outcomes require review before any work for this server is transmitted.
  if ([...listOfflineServerMutations(base), ...listOfflineServerAttachments(base)].some(item => item.replayState !== undefined)) return result();
  for (const attachment of listOfflineServerAttachments(base)) {
    // Unknown original owners/dependencies require explicit review, never adoption.
    if (attachment.replayState !== undefined || !sameOfflineReplayOrigin(attachment.origin, context.origin) || attachment.path !== `/spaces/${attachment.spaceId}/documents/${attachment.documentId}/attachments`) return result();
    // An upload cannot leapfrog an unresolved create for its own page. Existing
    // cross-queue counters do not prove a dependency-safe historical order.
    if (listOfflineServerMutations(base).some(item => item.method === "POST" &&
      item.path === `/spaces/${attachment.spaceId}/documents` && item.body &&
      typeof item.body === "object" && (item.body as { source_id?: unknown }).source_id === attachment.documentId)) {
      updateOfflineServerAttachment({ ...attachment, lastError: "Attachment depends on an unresolved page creation; review the retained page and attachment together before replay" }, attachment);
      return result();
    }
    let dispatched = false;
    let knownFailure = false;
    try {
      verifyContext(context);
      const blob = dataUrlToBlob(attachment.dataUrl, attachment.mimeType);
      if (!blob) throw new Error("Offline attachment bytes are invalid");
      const formData = new FormData(); formData.append("file", blob, attachment.name);
      dispatched = true;
      const response = await fetch(`${base}${attachment.path}`, { method: "POST", headers: { Authorization: `Bearer ${context.bearer}`, ...(attachment.scopeVersion===2?{"X-Starlane-Upload-Draft":attachment.draftId!}:{}), ...(attachment.uploadOperationId ? { "X-Starlane-Upload-Operation": attachment.uploadOperationId } : {}) }, body: formData });
      verifyContext(context);
      if (response.status !== 200 && response.status !== 201) { knownFailure = response.status >= 400 && response.status < 500; throw new Error(`Server returned ${response.status}`); }
      const uploaded: unknown = await response.json();
      validateUploadAudience(attachment.scopeVersion===2?attachment.draftId:undefined,uploaded&&typeof uploaded==="object"?(uploaded as {audience?:unknown}).audience:undefined);
      verifyContext(context);
      const url = uploaded && typeof uploaded === "object" ? (uploaded as { url?: unknown }).url : null;
      if (typeof url !== "string" || !url.trim()) throw new Error("Attachment upload response did not include a URL");
      if (JSON.stringify(listOfflineServerAttachments(base).find(item => item.id === attachment.id)) !== JSON.stringify(attachment)) throw new Error("Offline attachment changed during replay");
      replaceQueuedAttachmentReferences(attachment, url, context);
      verifyContext(context);
      acknowledgeOfflineServerAttachment(attachment.id, attachment);
      applied += 1;
    } catch (error) {
      // Context drift after dispatch is UNKNOWN, not rollback or permission to retry.
      // Mark uncertainty on the ORIGINAL item; no rebinding to the new session.
      try {
        updateOfflineServerAttachment({ ...attachment, ...(dispatched && !knownFailure ? { replayState: "unknown" as const } : {}), attempts: attachment.attempts + 1, lastError: `${error instanceof Error ? error.message : "Upload failed"}${dispatched && !knownFailure ? "; server outcome unknown; explicit review required" : ""}` }, attachment);
      } catch (persistError) {
        if (!dispatched) throw persistError;
        // A concurrently edited record is distinct from these already dispatched bytes.
        // Keep both; evidence has a new LOCAL journal ID and is never auto-replayed.
        try { retainUnknownOfflineServerAttachment(attachment, "Concurrent queue change; original server outcome unknown; explicit review required"); }
        catch { throw new Error("Could not retain unknown server outcome; preserve this workspace and review pending work before retrying"); }
      }
      return result();
    }
  }
  for (const originalMutation of listOfflineServerMutations(base)) {
    let mutation = originalMutation;
    if (mutation.replayState !== undefined || !sameOfflineReplayOrigin(mutation.origin, context.origin)) return result();
    const originalDraftPath = sourceBoundDraftPath(mutation.method, mutation.path, mutation.body);
    let dispatched = false;
    let knownFailure = false;
    try {
      verifyContext(context);
      if (deleteRoute(mutation.method, mutation.path) && (!canonicalDeleteOperation(mutation.deletionOperationId) || !originalDeleteTarget(mutation.body))) throw new Error("Legacy deletion has no original page identity; explicit review required");
      const headers: Record<string, string> = { Authorization: `Bearer ${context.bearer}` };
      if (mutation.body !== undefined) headers["Content-Type"] = "application/json";
      if (deleteRoute(mutation.method, mutation.path)) headers[DELETE_OPERATION_HEADER] = mutation.deletionOperationId!;
      if (originalDraftPath) {
        const pending = { ...mutation, replayState: "unknown" as const, hold: { schemaVersion: 1 as const, reason: "outcome_unproven", message: "Original queued save dispatched; confirmation pending" } };
        updateOfflineServerMutation(pending, mutation); mutation = pending;
      }
      dispatched = true;
      const response = await fetch(`${base}${mutation.path}`, { method: mutation.method, headers, body: mutation.body === undefined ? undefined : JSON.stringify(mutation.body) });
      verifyContext(context);
      if (deleteRoute(mutation.method, mutation.path) && (response.status !== 204 || response.headers.get(DELETE_OPERATION_HEADER) !== mutation.deletionOperationId)) throw new Error("Original deletion acknowledgement missing; review retained operation");
      const draftPath = sourceBoundDraftPath(mutation.method, mutation.path, mutation.body);
      // 409 is never lost-ACK proof.202 has no generic completed-mutation meaning.
      if (!(draftPath ? response.status === 200 : [200, 201, 204].includes(response.status))) { knownFailure = response.status >= 400 && response.status < 500; throw new Error(`Server returned ${response.status}; original intent requires review`); }
      if (draftPath) {
        const outcome: unknown = await response.json();
        verifyContext(context);
        if (!confirmsDraftSave(draftPath, mutation.body, outcome, context.origin.accountId)) throw new Error("Draft save result requires review");
      }
      verifyContext(context);
      if (deleteRoute(mutation.method, mutation.path)) {
        await retainAcknowledgedCleanup(cleanupAttemptFromAcknowledgedMutation(mutation), () => verifyContext(context));
        verifyContext(context);
      }
      acknowledgeOfflineServerMutation(mutation.id, mutation);
      applied += 1;
    } catch (error) {
      try {
        updateOfflineServerMutation({ ...mutation, ...(dispatched && originalDraftPath ? { replayState: knownFailure ? "held" as const : "unknown" as const, hold: { schemaVersion: 1 as const, reason: "outcome_unproven", message: "Original save needs deliberate review; HTTP status does not prove nonapplication" } } : dispatched && !knownFailure ? { replayState: "unknown" as const } : {}), attempts: mutation.attempts + 1, lastError: `${error instanceof Error ? error.message : "Mutation failed"}${dispatched && !knownFailure ? "; server outcome unknown; explicit review required" : ""}` }, mutation);
      } catch (persistError) {
        if (!dispatched) throw persistError;
        // A concurrently edited record is distinct from these already dispatched bytes.
        // Keep both; evidence has a new LOCAL journal ID and is never auto-replayed.
        try { retainUnknownOfflineServerMutation(mutation, "Concurrent queue change; original server outcome unknown; explicit review required"); }
        catch { throw new Error("Could not retain unknown server outcome; preserve this workspace and review pending work before retrying"); }
      }
      // A failed create/update is an unresolved dependency: never leapfrog it.
      return result();
    }
  }
  return { applied, pending: count(), blocked: 0 };
}
