export const NATIVE_AUTH_HANDOFF_EVENT = "starlane:native-auth-handoff";
export const NATIVE_GOOGLE_DRIVE_CALLBACK_EVENT = "starlane:native-google-drive-callback";

const PENDING_AUTH_HANDOFF_KEY = "starlane-pending-native-auth-handoff";
const PENDING_GOOGLE_DRIVE_CALLBACK_KEY = "starlane-pending-native-google-drive-callback";
const dispatchedCallbackUrls = new Set<string>();

export type NativeCallbackKind = "auth-handoff" | "google-drive";

function callbackKind(url: string): NativeCallbackKind | null {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }

  if (parsed.protocol !== "starlane:") return null;
  if (parsed.hostname === "auth" && parsed.pathname === "/handoff") return "auth-handoff";
  if (
    parsed.hostname === "provider" &&
    parsed.pathname === "/google-drive/callback"
  ) {
    return "google-drive";
  }
  return null;
}

function storageKey(kind: NativeCallbackKind): string {
  return kind === "auth-handoff"
    ? PENDING_AUTH_HANDOFF_KEY
    : PENDING_GOOGLE_DRIVE_CALLBACK_KEY;
}

function eventName(kind: NativeCallbackKind): string {
  return kind === "auth-handoff"
    ? NATIVE_AUTH_HANDOFF_EVENT
    : NATIVE_GOOGLE_DRIVE_CALLBACK_EVENT;
}

export function dispatchNativeCallbackUrl(url: string): NativeCallbackKind | null {
  const kind = callbackKind(url);
  if (!kind) return null;
  if (dispatchedCallbackUrls.has(url)) return kind;

  dispatchedCallbackUrls.add(url);
  sessionStorage.setItem(storageKey(kind), url);
  document.dispatchEvent(new CustomEvent(eventName(kind), { detail: url }));
  return kind;
}

export function consumePendingNativeCallback(
  kind: NativeCallbackKind,
): string | null {
  const key = storageKey(kind);
  const url = sessionStorage.getItem(key);
  sessionStorage.removeItem(key);
  return url && callbackKind(url) === kind ? url : null;
}
