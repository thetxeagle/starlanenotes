import {
  cacheServerResponse,
  listOfflineServerMutations,
  queueOfflineServerMutation,
  replaceOfflineServerMutations,
  serverCacheScopeForToken,
  type QueuedServerMutation,
  readCachedServerResponse,
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
}

export function getSessionClientDetails(): SessionClientDetails {
  const userAgent = typeof navigator === "undefined" ? "" : navigator.userAgent;
  const isMobile = /android|iphone|ipad|ipod/i.test(userAgent);
  if (isTauri()) {
    return {
      session_kind: "app",
      device_name: isMobile ? "Starlane Notes on mobile" : "Starlane Notes desktop app",
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
  return { session_kind: "browser", device_name: `${browser} on ${device}` };
}

interface ApiOptions {
  method?: string;
  body?: unknown;
  headers?: Record<string, string>;
}

interface ApiErrorBody {
  error?: unknown;
  message?: unknown;
}

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
  return message.includes("invalid or expired token") || message.includes("missing authentication token");
}

class ApiClient {
  private accessToken: string | null = null;

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
    const { method = "GET", body, headers = {} } = options;

    const token = this.getToken();
    if (token && !isPublicAuthFlowPath(path)) {
      headers["Authorization"] = `Bearer ${token}`;
    }

    const isFormData = typeof FormData !== "undefined" && body instanceof FormData;
    if (body && !isFormData) {
      headers["Content-Type"] = "application/json";
    }

    const apiBase = getApiBase();
    const cacheScope = serverCacheScopeForToken(token);
    let response: Response;
    try {
      response = await fetch(`${apiBase}${path}`, {
        method,
        headers,
        body: body ? (isFormData ? body : JSON.stringify(body)) : undefined,
      });
    } catch (error) {
      if (method === "GET") {
        const cached =
          readCachedServerResponse<T>(apiBase, path, cacheScope) ??
          (path === "/auth/config" ? readCachedServerResponse<T>(apiBase, path) : null);
        if (cached !== null) return cached;
      }

      const supportsOfflineCreate =
        method === "POST" &&
        /^\/spaces\/[^/]+\/(documents|folders)$/.test(path) &&
        body &&
        typeof body === "object" &&
        typeof (body as { source_id?: unknown }).source_id === "string";
      // Workspace updates/deletes and source-ID-backed creates are safe to queue.
      // Authentication and administration never claim completion while offline.
      if (
        (method === "PUT" || method === "DELETE" || supportsOfflineCreate) &&
        path.startsWith("/spaces/") &&
        this.isAuthenticated()
      ) {
        queueOfflineServerMutation(
          apiBase,
          path,
          method as "POST" | "PUT" | "DELETE",
          body,
          cacheScope,
        );
        return undefined as T;
      }

      throw error;
    }

    if (!response.ok) {
      const errorBody = await response.json().catch(() => ({ message: "Request failed" }));
      const { code, message } = normalizeApiErrorBody(errorBody);
      if (shouldAttemptTokenRefresh(path, token, response.status, { code, message })) {
        const refreshed = await this.tryRefresh();
        if (refreshed) {
          return this.fetch(path, options);
        }
        this.logout();
        window.location.href = "/login";
        throw new Error("Session expired");
      }
      throw new ApiError(message || `HTTP ${response.status}`, response.status, code);
    }

    if (response.status === 204) {
      return undefined as T;
    }

    const data = (await response.json()) as T;
    if (method === "GET") {
      cacheServerResponse(apiBase, path, data, cacheScope);
    }
    return data;
  }

  private async tryRefresh(): Promise<boolean> {
    const refreshToken = this.getRefreshToken();
    if (!refreshToken) return false;

    try {
      const response = await fetch(`${getApiBase()}/auth/refresh`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ refresh_token: refreshToken }),
      });

      if (!response.ok) return false;

      const data = await response.json();
      this.setToken(data.access_token);
      this.setRefreshToken(data.refresh_token);
      return true;
    } catch {
      return false;
    }
  }

  logout() {
    this.setToken(null);
    this.setRefreshToken(null);
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

/**
 * Replays local, idempotent workspace edits in their original order. A server
 * validation/conflict response remains queued for explicit recovery rather than
 * being overwritten on the device.
 */
export async function replayOfflineServerMutations(
  base = getApiBase(),
): Promise<OfflineServerMutationReplayResult> {
  const queue = listOfflineServerMutations(base);
  if (queue.length === 0) return { applied: 0, pending: 0, blocked: 0 };

  const retained: QueuedServerMutation[] = [];
  let applied = 0;
  let blocked = 0;

  for (const mutation of queue) {
    const headers: Record<string, string> = {};
    const token = api.getToken();
    if (token) headers.Authorization = `Bearer ${token}`;
    if (mutation.body !== undefined) headers["Content-Type"] = "application/json";

    try {
      const response = await fetch(`${mutation.serverBase}${mutation.path}`, {
        method: mutation.method,
        headers,
        body: mutation.body === undefined ? undefined : JSON.stringify(mutation.body),
      });
      if (
        response.ok ||
        (mutation.method === "POST" && response.status === 409 && mutation.body &&
          typeof mutation.body === "object" &&
          typeof (mutation.body as { source_id?: unknown }).source_id === "string")
      ) {
        applied += 1;
        continue;
      }
      retained.push({
        ...mutation,
        attempts: mutation.attempts + 1,
        lastError: `Server returned ${response.status}`,
      });
      blocked += 1;
    } catch (error) {
      retained.push({
        ...mutation,
        attempts: mutation.attempts + 1,
        lastError: error instanceof Error ? error.message : "Server is unreachable",
      });
    }
  }

  const otherServers = listOfflineServerMutations().filter(
    (mutation) => mutation.serverBase !== base.replace(/\/+$/, ""),
  );
  replaceOfflineServerMutations([...otherServers, ...retained]);
  return { applied, pending: retained.length - blocked, blocked };
}
