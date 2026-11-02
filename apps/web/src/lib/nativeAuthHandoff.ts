import { getApiBase, getSessionClientDetails, isTauri } from "@/lib/api";
import { openUrl } from "@tauri-apps/plugin-opener";

const STORAGE_KEY = "starlane-native-auth-handoff";
const BROWSER_HANDOFF_KEY = "starlane-browser-native-auth-handoff";

export const NATIVE_AUTH_HANDOFF_STARTED_EVENT = "starlane-native-auth-handoff-started";

export interface NativeAuthHandoffResult {
  access_token: string;
  refresh_token: string;
  user: unknown;
}

function base64Url(bytes: Uint8Array): string {
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/u, "");
}

async function verifierChallenge(verifier: string): Promise<string> {
  return base64Url(
    new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)),
    ),
  );
}

function readNativeAuthHandoff(): { handoffId: string; verifier: string } | null {
  try {
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") as {
      handoffId?: string;
      verifier?: string;
    } | null;
    return stored?.handoffId && stored.verifier
      ? { handoffId: stored.handoffId, verifier: stored.verifier }
      : null;
  } catch {
    return null;
  }
}

export function hasPendingNativeAuthHandoff(): boolean {
  return readNativeAuthHandoff() !== null;
}

export function parseNativeAuthHandoffCallback(url: string): { handoffId: string } | null {
  const callback = new URL(url);
  const handoffId = callback.searchParams.get("handoff_id");
  return callback.protocol === "starlane:" &&
    callback.hostname === "auth" &&
    callback.pathname === "/handoff" &&
    handoffId
    ? { handoffId }
    : null;
}

export async function beginNativeAuthHandoff(): Promise<void> {
  if (!isTauri()) throw new Error("Browser sign-in is available in the Starlane app only.");
  const verifier = base64Url(crypto.getRandomValues(new Uint8Array(32)));
  const response = await fetch(`${getApiBase()}/auth/native-handoff/start`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code_challenge: await verifierChallenge(verifier) }),
  });
  if (!response.ok) throw new Error("Unable to start browser sign-in.");
  const data = (await response.json()) as {
    handoff_id: string;
    authorization_url: string;
  };
  const authorizationUrl = new URL(data.authorization_url);
  if (authorizationUrl.protocol !== "https:" && authorizationUrl.protocol !== "http:") {
    throw new Error("The server returned an invalid browser sign-in URL.");
  }

  localStorage.setItem(
    STORAGE_KEY,
    JSON.stringify({ handoffId: data.handoff_id, verifier }),
  );
  try {
    await openUrl(authorizationUrl);
    if (typeof document !== "undefined") {
      document.dispatchEvent(new Event(NATIVE_AUTH_HANDOFF_STARTED_EVENT));
    }
  } catch {
    localStorage.removeItem(STORAGE_KEY);
    throw new Error("Unable to open the system browser for sign-in.");
  }
}

export async function exchangeNativeAuthHandoff(): Promise<NativeAuthHandoffResult | null> {
  const stored = readNativeAuthHandoff();
  if (!stored) return null;

  const response = await fetch(`${getApiBase()}/auth/native-handoff/exchange`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      handoff_id: stored.handoffId,
      code_verifier: stored.verifier,
      ...getSessionClientDetails(),
    }),
  });
  if (response.status === 202 || response.status === 429) return null;
  if (response.status >= 500) return null;
  if (!response.ok) {
    localStorage.removeItem(STORAGE_KEY);
    throw new Error("Browser sign-in expired or was rejected.");
  }

  localStorage.removeItem(STORAGE_KEY);
  return response.json() as Promise<NativeAuthHandoffResult>;
}

export async function finishNativeAuthHandoff(
  url: string,
): Promise<NativeAuthHandoffResult | null> {
  const callback = parseNativeAuthHandoffCallback(url);
  const stored = readNativeAuthHandoff();
  if (!callback || !stored || stored.handoffId !== callback.handoffId) {
    throw new Error("Browser sign-in could not be verified.");
  }
  return exchangeNativeAuthHandoff();
}

export function getBrowserNativeAuthHandoff(): string | null {
  const search = new URLSearchParams(window.location.search);
  return (
    search.get("handoff_id") ??
    search.get("native_handoff") ??
    sessionStorage.getItem(BROWSER_HANDOFF_KEY)
  );
}

export function getNativeAuthReturnUrl(handoffId: string): string {
  return `starlane://auth/handoff?handoff_id=${encodeURIComponent(handoffId)}`;
}

export async function completeNativeAuthHandoffInBrowser(): Promise<string> {
  const handoffId = getBrowserNativeAuthHandoff();
  if (!handoffId) throw new Error("This app sign-in request is missing or invalid.");

  const response = await fetch("/api/v1/auth/native-handoff/complete", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${localStorage.getItem("access_token") ?? ""}`,
    },
    body: JSON.stringify({ handoff_id: handoffId }),
  });
  if (!response.ok) {
    sessionStorage.removeItem(BROWSER_HANDOFF_KEY);
    throw new Error("This app sign-in request expired or was already used.");
  }
  sessionStorage.removeItem(BROWSER_HANDOFF_KEY);
  return handoffId;
}

export function rememberBrowserNativeAuthHandoff(): void {
  const search = new URLSearchParams(window.location.search);
  const handoffId = search.get("handoff_id") ?? search.get("native_handoff");
  if (handoffId) sessionStorage.setItem(BROWSER_HANDOFF_KEY, handoffId);
}
