import { getApiBase, getSessionClientDetails, isTauri } from "@/lib/api";

const STORAGE_KEY = "starlane-native-auth-handoff";
const BROWSER_HANDOFF_KEY = "starlane-browser-native-auth-handoff";

function base64Url(bytes: Uint8Array): string {
  let binary = "";
  bytes.forEach((byte) => { binary += String.fromCharCode(byte); });
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/u, "");
}

async function verifierChallenge(verifier: string): Promise<string> {
  return base64Url(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))));
}

export function parseNativeAuthHandoffCallback(url: string): { handoffId: string; code: string } | null {
  const callback = new URL(url);
  const handoffId = callback.searchParams.get("handoff_id");
  const code = callback.searchParams.get("code");
  return callback.protocol === "starlane:" && callback.hostname === "auth" && callback.pathname === "/handoff" && handoffId && code
    ? { handoffId, code }
    : null;
}

export async function beginNativeAuthHandoff(): Promise<void> {
  if (!isTauri()) throw new Error("Browser sign-in is available in the Starlane app only.");
  const verifier = base64Url(crypto.getRandomValues(new Uint8Array(32)));
  const response = await fetch(`${getApiBase()}/auth/native-handoff/start`, {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ code_challenge: await verifierChallenge(verifier) }),
  });
  if (!response.ok) throw new Error("Unable to start browser sign-in.");
  const data = await response.json() as { handoff_id: string; authorization_url: string };
  sessionStorage.setItem(STORAGE_KEY, JSON.stringify({ handoffId: data.handoff_id, verifier }));
  window.open(data.authorization_url, "_blank", "noopener,noreferrer");
}

export async function finishNativeAuthHandoff(url: string): Promise<{ access_token: string; refresh_token: string; user: unknown }> {
  const callback = parseNativeAuthHandoffCallback(url);
  let stored: { handoffId?: string; verifier?: string } | null = null;
  try { stored = JSON.parse(sessionStorage.getItem(STORAGE_KEY) ?? "null") as { handoffId?: string; verifier?: string } | null; } catch { /* invalid stale storage */ }
  if (!callback || !stored?.verifier || stored.handoffId !== callback.handoffId) throw new Error("Browser sign-in could not be verified.");
  const response = await fetch(`${getApiBase()}/auth/native-handoff/exchange`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ handoff_id: callback.handoffId, code: callback.code, code_verifier: stored.verifier, ...getSessionClientDetails() }) });
  sessionStorage.removeItem(STORAGE_KEY);
  if (!response.ok) throw new Error("Browser sign-in expired or was rejected.");
  return response.json();
}

export function completeNativeAuthHandoffInBrowser(): Promise<void> | null {
  const handoffId = new URLSearchParams(window.location.search).get("native_handoff") ?? sessionStorage.getItem(BROWSER_HANDOFF_KEY);
  if (!handoffId) return null;
  return fetch("/api/v1/auth/native-handoff/complete", { method: "POST", headers: { "Content-Type": "application/json", Authorization: `Bearer ${localStorage.getItem("access_token") ?? ""}` }, body: JSON.stringify({ handoff_id: handoffId }) })
    .then(async (response) => { if (!response.ok) throw new Error("Unable to complete app sign-in."); return response.json() as Promise<{ handoff_id: string; code: string }>; })
    .then(({ handoff_id, code }) => {
      sessionStorage.removeItem(BROWSER_HANDOFF_KEY);
      window.location.assign(`starlane://auth/handoff?handoff_id=${encodeURIComponent(handoff_id)}&code=${encodeURIComponent(code)}`);
    });
}

export function rememberBrowserNativeAuthHandoff(): void {
  const handoffId = new URLSearchParams(window.location.search).get("native_handoff");
  if (handoffId) sessionStorage.setItem(BROWSER_HANDOFF_KEY, handoffId);
}
