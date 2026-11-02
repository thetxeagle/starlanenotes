import { getCurrentLaunchMode } from "./launchMode";
import { getStorageSelection } from "./storageProviders";

export interface OfflineReplayOrigin {
  version: 1;
  serverBase: string;
  accountId: string;
  sessionId: string;
  cacheScope: string;
  storageMode: "server" | "local";
  provider: "none" | "icloud-drive" | "google-drive";
}
export interface OfflineReplayContext {
  origin: OfflineReplayOrigin;
  bearer: string;
  refresh: string | null;
}
const nonempty = (v: unknown): v is string => typeof v === "string" && v.trim().length > 0;
export function isOfflineReplayOrigin(value: unknown): value is OfflineReplayOrigin {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const v = value as Record<string, unknown>;
  return Object.keys(v).length === 7 && v.version === 1 && nonempty(v.serverBase) &&
    v.serverBase === v.serverBase.replace(/\/+$/, "") && nonempty(v.accountId) &&
    nonempty(v.sessionId) && nonempty(v.cacheScope) && v.cacheScope.startsWith("session-") &&
    (v.storageMode === "server" || v.storageMode === "local") &&
    (v.provider === "none" || v.provider === "icloud-drive" || v.provider === "google-drive");
}
export function sameOfflineReplayOrigin(a: unknown, b: unknown): boolean {
  return isOfflineReplayOrigin(a) && isOfflineReplayOrigin(b) &&
    a.serverBase === b.serverBase && a.accountId === b.accountId && a.sessionId === b.sessionId &&
    a.cacheScope === b.cacheScope && a.storageMode === b.storageMode && a.provider === b.provider;
}
export function captureOfflineReplayOrigin(base: string, bearer: string | null, cacheScope: string): OfflineReplayContext {
  if (!bearer || bearer !== localStorage.getItem("access_token") || getCurrentLaunchMode() !== "server") {
    throw new Error("Offline work requires its original authenticated server context");
  }
  let claims: Record<string, unknown>;
  try {
    const payload = bearer.split(".");
    if (payload.length !== 3) throw new Error();
    const raw = atob(payload[1].replace(/-/g, "+").replace(/_/g, "/"));
    claims = JSON.parse(new TextDecoder().decode(Uint8Array.from(raw, c => c.charCodeAt(0))));
    if (!claims || typeof claims !== "object" || !nonempty(claims.sub) || !nonempty(claims.sid) || claims.token_type !== "access") throw new Error();
  } catch { throw new Error("Offline work requires identifiable original account and session"); }
  // Claims identify queue provenance only. The server authenticates the actual bearer.
  const selection = getStorageSelection();
  const origin: OfflineReplayOrigin = { version: 1, serverBase: base.replace(/\/+$/, ""),
    accountId: claims.sub as string, sessionId: claims.sid as string, cacheScope,
    storageMode: selection.mode, provider: selection.provider };
  if (!isOfflineReplayOrigin(origin) || `session-${localStorage.getItem("starlane_server_cache_v1:session-scope")}` !== cacheScope) {
    throw new Error("Offline session partition is unavailable");
  }
  return { origin, bearer, refresh: localStorage.getItem("refresh_token") };
}
export function verifyOfflineReplayContext(context: OfflineReplayContext, base: string, memoryToken: string | null): void {
  const selection = getStorageSelection();
  if (base.replace(/\/+$/, "") !== context.origin.serverBase || memoryToken !== context.bearer ||
    localStorage.getItem("access_token") !== context.bearer || localStorage.getItem("refresh_token") !== context.refresh ||
    `session-${localStorage.getItem("starlane_server_cache_v1:session-scope")}` !== context.origin.cacheScope ||
    getCurrentLaunchMode() !== "server" || selection.mode !== context.origin.storageMode || selection.provider !== context.origin.provider) {
    throw new Error("Offline replay context changed; retain pending work for review");
  }
}

/** Matching identity only; the server validates the actual signed credential. */
export function offlineReplayTokenMatchesOrigin(bearer: string, origin: OfflineReplayOrigin): boolean {
  try {
    const parts = bearer.split("."); if (parts.length !== 3) return false;
    const raw = atob(parts[1].replace(/-/g, "+").replace(/_/g, "/"));
    const claims = JSON.parse(new TextDecoder().decode(Uint8Array.from(raw, c => c.charCodeAt(0))));
    return claims?.token_type === "access" && claims.sub === origin.accountId && claims.sid === origin.sessionId;
  } catch { return false; }
}
