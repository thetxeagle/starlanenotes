import {
  cacheServerResponse,
  hasCachedServerWorkspace,
  serverCacheScopeForToken,
} from "@/lib/offlineServerCache";

export interface ServerAuthConfigPreview {
  google_enabled?: boolean;
  github_enabled?: boolean;
  oidc_enabled?: boolean;
  oidc_display_name?: string;
  allow_public_signups?: boolean;
}

export interface ServerStartupCheckResult {
  serverUrl: string;
  config: ServerAuthConfigPreview;
}

const DEFAULT_SERVER_STARTUP_TIMEOUT_MS = 8_000;

export function normalizeServerUrl(value: string): string {
  const trimmed = value.trim().replace(/\/+$/, "");
  if (!trimmed) {
    throw new Error("Enter a server URL.");
  }

  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new Error("Enter a valid server URL, including https://.");
  }

  if (parsed.protocol !== "https:" && parsed.protocol !== "http:") {
    throw new Error("Server URL must start with http:// or https://.");
  }

  return trimmed;
}

export function describeServerStartupError(error: unknown, serverUrl: string): string {
  if (error instanceof DOMException && error.name === "AbortError") {
    return `Starlane Notes could not reach ${serverUrl} before the connection timed out.`;
  }

  if (error instanceof Error) {
    if (error.message.toLowerCase().includes("load failed")) {
      return `Starlane Notes could not load ${serverUrl}/api/v1/auth/config. Check that the phone can reach the server URL, TLS is trusted, and production CORS allows the native app origin.`;
    }
    return error.message;
  }

  return `Starlane Notes could not reach ${serverUrl}.`;
}

export async function checkServerStartup(
  value: string,
  timeoutMs = DEFAULT_SERVER_STARTUP_TIMEOUT_MS,
): Promise<ServerStartupCheckResult> {
  const serverUrl = normalizeServerUrl(value);
  const controller = new AbortController();
  const timeout = globalThis.setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(`${serverUrl}/api/v1/auth/config`, {
      cache: "no-store",
      signal: controller.signal,
    });

    if (!response.ok) {
      throw new Error(`Server returned ${response.status}.`);
    }

    const config = (await response.json()) as ServerAuthConfigPreview;
    cacheServerResponse(`${serverUrl}/api/v1`, "/auth/config", config);
    return { serverUrl, config };
  } catch (error) {
    const message = describeServerStartupError(error, serverUrl);
    throw new Error(message);
  } finally {
    globalThis.clearTimeout(timeout);
  }
}

export function hasOfflineServerWorkspace(value: string, accessToken: string | null): boolean {
  try {
    return hasCachedServerWorkspace(
      `${normalizeServerUrl(value)}/api/v1`,
      serverCacheScopeForToken(accessToken),
    );
  } catch {
    return false;
  }
}
