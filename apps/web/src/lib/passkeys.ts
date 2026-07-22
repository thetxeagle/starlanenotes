type JsonPublicKeyContainer = {
  publicKey?: Record<string, unknown>;
  public_key?: Record<string, unknown>;
};

function isTauriWebView(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function browserHasPasskeySupport(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof navigator !== "undefined" &&
    window.isSecureContext !== false &&
    typeof window.PublicKeyCredential !== "undefined" &&
    Boolean(navigator.credentials) &&
    typeof navigator.credentials.create === "function" &&
    typeof navigator.credentials.get === "function"
  );
}

export function isPasskeySupported(): boolean {
  return browserHasPasskeySupport();
}

function base64UrlToBuffer(value: string, label = "Passkey value"): ArrayBuffer {
  const normalized = value.trim();
  if (!normalized) {
    throw new Error(`${label} is missing.`);
  }
  if (!/^[A-Za-z0-9_-]+={0,2}$/u.test(normalized)) {
    throw new Error(`${label} is malformed.`);
  }
  if (normalized.replace(/=+$/u, "").length % 4 === 1) {
    throw new Error(`${label} is malformed.`);
  }

  const base64 = normalized.replace(/-/g, "+").replace(/_/g, "/");
  const padded = base64.padEnd(base64.length + ((4 - (base64.length % 4)) % 4), "=");
  let binary: string;
  try {
    binary = atob(padded);
  } catch {
    throw new Error(`${label} is malformed.`);
  }
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes.buffer;
}

function base64UrlFieldToBuffer(value: unknown, label: string): ArrayBuffer {
  if (typeof value !== "string") {
    throw new Error(`${label} is missing.`);
  }
  return base64UrlToBuffer(value, label);
}

function bufferToBase64Url(buffer: ArrayBuffer | null): string | null {
  if (!buffer) return null;
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/u, "");
}

function unwrapPublicKeyOptions(challenge: unknown): Record<string, unknown> {
  if (!challenge || typeof challenge !== "object") {
    throw new Error("Passkey challenge is malformed.");
  }
  const container = challenge as JsonPublicKeyContainer;
  const publicKey = container.publicKey ?? container.public_key ?? (challenge as Record<string, unknown>);
  if (!publicKey || typeof publicKey !== "object") {
    throw new Error("Passkey challenge is missing public key options.");
  }
  return { ...publicKey };
}

function toCreationOptions(challenge: unknown): PublicKeyCredentialCreationOptions {
  const publicKey = unwrapPublicKeyOptions(challenge);
  publicKey.challenge = base64UrlFieldToBuffer(publicKey.challenge, "Passkey challenge");

  const user = publicKey.user as Record<string, unknown> | undefined;
  if (!user?.id) {
    throw new Error("Passkey user id is missing.");
  }
  if (user.id) {
    publicKey.user = { ...user, id: base64UrlFieldToBuffer(user.id, "Passkey user id") };
  }

  const excludeCredentials = publicKey.excludeCredentials as
    | Array<Record<string, unknown>>
    | undefined;
  if (excludeCredentials) {
    publicKey.excludeCredentials = excludeCredentials.map((credential, index) => ({
      ...credential,
      id: base64UrlFieldToBuffer(
        credential.id,
        `Passkey excluded credential ${index + 1} id`,
      ),
    }));
  }

  return publicKey as unknown as PublicKeyCredentialCreationOptions;
}

function toRequestOptions(challenge: unknown): PublicKeyCredentialRequestOptions {
  const publicKey = unwrapPublicKeyOptions(challenge);
  publicKey.challenge = base64UrlFieldToBuffer(publicKey.challenge, "Passkey challenge");

  const allowCredentials = publicKey.allowCredentials as Array<Record<string, unknown>> | undefined;
  if (allowCredentials) {
    publicKey.allowCredentials = allowCredentials.map((credential, index) => ({
      ...credential,
      id: base64UrlFieldToBuffer(
        credential.id,
        `Passkey allowed credential ${index + 1} id`,
      ),
    }));
  }

  return publicKey as unknown as PublicKeyCredentialRequestOptions;
}

function credentialBase(credential: PublicKeyCredential) {
  return {
    id: credential.id,
    rawId: bufferToBase64Url(credential.rawId),
    type: credential.type,
    extensions: credential.getClientExtensionResults(),
  };
}

function serializeCreatedCredential(credential: PublicKeyCredential): unknown {
  const response = credential.response as AuthenticatorAttestationResponse;
  return {
    ...credentialBase(credential),
    response: {
      clientDataJSON: bufferToBase64Url(response.clientDataJSON),
      attestationObject: bufferToBase64Url(response.attestationObject),
    },
  };
}

function serializeAssertionCredential(credential: PublicKeyCredential): unknown {
  const response = credential.response as AuthenticatorAssertionResponse;
  return {
    ...credentialBase(credential),
    response: {
      clientDataJSON: bufferToBase64Url(response.clientDataJSON),
      authenticatorData: bufferToBase64Url(response.authenticatorData),
      signature: bufferToBase64Url(response.signature),
      userHandle: bufferToBase64Url(response.userHandle),
    },
  };
}

function passkeyErrorName(error: unknown): string {
  if (error && typeof error === "object" && "name" in error) {
    const name = (error as { name?: unknown }).name;
    if (typeof name === "string") return name;
  }
  return "";
}

function normalizePasskeyCeremonyError(action: "setup" | "sign-in", error: unknown): Error {
  const name = passkeyErrorName(error);
  if (name === "NotAllowedError" || name === "AbortError") {
    if (isTauriWebView()) {
      return new Error(
        `Passkey ${action === "setup" ? "setup" : "sign-in"} could not complete in the Starlane app. Update the app, then try again; if it persists, verify its Associated Domains setup.`,
      );
    }
    return new Error(action === "setup" ? "Passkey setup was canceled." : "Passkey sign-in was canceled.");
  }
  if (name === "SecurityError") {
    return new Error(
      "Passkeys are blocked for this app origin. Check the server URL and WebAuthn origin configuration.",
    );
  }
  if (name === "NotSupportedError") {
    return new Error(
      action === "setup"
        ? "This browser or authenticator cannot create a passkey for Starlane."
        : "This browser or authenticator cannot use a passkey for Starlane.",
    );
  }
  if (name === "InvalidStateError" && action === "setup") {
    return new Error("A passkey may already be registered on this device.");
  }
  if (error instanceof Error && error.message.trim()) {
    return new Error(error.message);
  }
  return new Error(action === "setup" ? "Passkey setup failed." : "Passkey sign-in failed.");
}

export async function createPasskeyCredential(challenge: unknown): Promise<unknown> {
  if (!browserHasPasskeySupport()) {
    throw new Error("Passkeys are not supported by this browser or webview.");
  }

  const publicKey = toCreationOptions(challenge);
  let credential: PublicKeyCredential | null;
  try {
    credential = (await navigator.credentials.create({ publicKey })) as PublicKeyCredential | null;
  } catch (error) {
    throw normalizePasskeyCeremonyError("setup", error);
  }
  if (!credential) throw new Error("Passkey setup was canceled.");

  return serializeCreatedCredential(credential);
}

export async function getPasskeyCredential(challenge: unknown): Promise<unknown> {
  if (!browserHasPasskeySupport()) {
    throw new Error("Passkeys are not supported by this browser or webview.");
  }

  const publicKey = toRequestOptions(challenge);
  let credential: PublicKeyCredential | null;
  try {
    credential = (await navigator.credentials.get({ publicKey })) as PublicKeyCredential | null;
  } catch (error) {
    throw normalizePasskeyCeremonyError("sign-in", error);
  }
  if (!credential) throw new Error("Passkey sign-in was canceled.");

  return serializeAssertionCredential(credential);
}

export const passkeyTestInternals = {
  browserHasPasskeySupport,
  isTauriWebView,
  base64UrlToBuffer,
  bufferToBase64Url,
  toCreationOptions,
  toRequestOptions,
  serializeCreatedCredential,
  serializeAssertionCredential,
  normalizePasskeyCeremonyError,
};
