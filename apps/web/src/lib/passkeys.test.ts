import { beforeEach, describe, expect, it, vi } from "vitest";
import { passkeyTestInternals } from "./passkeys";

function bytes(values: number[]): ArrayBuffer {
  return new Uint8Array(values).buffer;
}

function asBytes(value: ArrayBuffer | BufferSource): number[] {
  const buffer = value instanceof ArrayBuffer ? value : value.buffer;
  return Array.from(new Uint8Array(buffer));
}

describe("passkey browser adapter", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("requires PublicKeyCredential plus credential create/get methods before enabling passkeys", () => {
    vi.stubGlobal("window", { PublicKeyCredential: vi.fn(), isSecureContext: true });
    vi.stubGlobal("navigator", {
      credentials: {
        create: vi.fn(),
        get: vi.fn(),
      },
    });

    expect(passkeyTestInternals.browserHasPasskeySupport()).toBe(true);

    vi.stubGlobal("navigator", {
      credentials: {
        create: vi.fn(),
      },
    });
    expect(passkeyTestInternals.browserHasPasskeySupport()).toBe(false);

    vi.stubGlobal("navigator", {
      credentials: {},
    });
    expect(passkeyTestInternals.browserHasPasskeySupport()).toBe(false);
  });

  it("keeps passkeys disabled when the browser reports an insecure context", () => {
    vi.stubGlobal("window", { PublicKeyCredential: vi.fn(), isSecureContext: false });
    vi.stubGlobal("navigator", {
      credentials: {
        create: vi.fn(),
        get: vi.fn(),
      },
    });

    expect(passkeyTestInternals.browserHasPasskeySupport()).toBe(false);
  });

  it("converts registration challenge, user id, and excluded credential ids to ArrayBuffers", () => {
    const options = passkeyTestInternals.toCreationOptions({
      public_key: {
        challenge: "AQIDBA",
        rp: { id: "notes.example.com", name: "Starlane Notes" },
        user: { id: "BQYH", name: "eagle@example.test", displayName: "The Eagle" },
        pubKeyCredParams: [{ type: "public-key", alg: -7 }],
        excludeCredentials: [{ type: "public-key", id: "CAkK" }],
      },
    });

    expect(asBytes(options.challenge)).toEqual([1, 2, 3, 4]);
    expect(asBytes(options.user.id)).toEqual([5, 6, 7]);
    expect(asBytes(options.excludeCredentials?.[0]?.id as ArrayBuffer)).toEqual([8, 9, 10]);
  });

  it("converts login challenge and allowed credential ids to ArrayBuffers", () => {
    const options = passkeyTestInternals.toRequestOptions({
      publicKey: {
        challenge: "CwwNDg",
        allowCredentials: [{ type: "public-key", id: "DxAR" }],
        userVerification: "preferred",
      },
    });

    expect(asBytes(options.challenge)).toEqual([11, 12, 13, 14]);
    expect(asBytes(options.allowCredentials?.[0]?.id as ArrayBuffer)).toEqual([15, 16, 17]);
  });

  it("rejects malformed registration challenge payloads before invoking the authenticator", () => {
    expect(() =>
      passkeyTestInternals.toCreationOptions({
        public_key: {
          rp: { id: "notes.example.com", name: "Starlane Notes" },
          user: { id: "BQYH", name: "eagle@example.test", displayName: "The Eagle" },
          pubKeyCredParams: [{ type: "public-key", alg: -7 }],
        },
      }),
    ).toThrow("Passkey challenge is missing.");

    expect(() =>
      passkeyTestInternals.toCreationOptions({
        public_key: {
          challenge: "AQIDBA",
          rp: { id: "notes.example.com", name: "Starlane Notes" },
          user: { name: "eagle@example.test", displayName: "The Eagle" },
          pubKeyCredParams: [{ type: "public-key", alg: -7 }],
        },
      }),
    ).toThrow("Passkey user id is missing.");

    expect(() =>
      passkeyTestInternals.toCreationOptions({
        public_key: {
          challenge: "A",
          rp: { id: "notes.example.com", name: "Starlane Notes" },
          user: { id: "BQYH", name: "eagle@example.test", displayName: "The Eagle" },
          pubKeyCredParams: [{ type: "public-key", alg: -7 }],
        },
      }),
    ).toThrow("Passkey challenge is malformed.");
  });

  it("rejects malformed login allow-credential ids before invoking the authenticator", () => {
    expect(() =>
      passkeyTestInternals.toRequestOptions({
        publicKey: {
          challenge: "CwwNDg",
          allowCredentials: [{ type: "public-key" }],
          userVerification: "preferred",
        },
      }),
    ).toThrow("Passkey allowed credential 1 id is missing.");

    expect(() =>
      passkeyTestInternals.toRequestOptions({
        publicKey: {
          challenge: "CwwNDg",
          allowCredentials: [{ type: "public-key", id: "not valid!" }],
          userVerification: "preferred",
        },
      }),
    ).toThrow("Passkey allowed credential 1 id is malformed.");
  });

  it("serializes registration credential responses as base64url JSON for the API", () => {
    const credential = {
      id: "credential-id",
      rawId: bytes([1, 2, 3]),
      type: "public-key",
      getClientExtensionResults: () => ({ appid: true }),
      response: {
        clientDataJSON: bytes([4, 5, 6]),
        attestationObject: bytes([7, 8, 9]),
      },
    } as unknown as PublicKeyCredential;

    expect(passkeyTestInternals.serializeCreatedCredential(credential)).toEqual({
      id: "credential-id",
      rawId: "AQID",
      type: "public-key",
      extensions: { appid: true },
      response: {
        clientDataJSON: "BAUG",
        attestationObject: "BwgJ",
      },
    });
  });

  it("serializes login credential responses and preserves nullable user handles", () => {
    const credential = {
      id: "credential-id",
      rawId: bytes([1, 2, 3]),
      type: "public-key",
      getClientExtensionResults: () => ({}),
      response: {
        clientDataJSON: bytes([4, 5, 6]),
        authenticatorData: bytes([7, 8, 9]),
        signature: bytes([10, 11, 12]),
        userHandle: null,
      },
    } as unknown as PublicKeyCredential;

    expect(passkeyTestInternals.serializeAssertionCredential(credential)).toEqual({
      id: "credential-id",
      rawId: "AQID",
      type: "public-key",
      extensions: {},
      response: {
        clientDataJSON: "BAUG",
        authenticatorData: "BwgJ",
        signature: "CgsM",
        userHandle: null,
      },
    });
  });

  it("normalizes canceled passkey setup and sign-in ceremony errors", () => {
    expect(
      passkeyTestInternals.normalizePasskeyCeremonyError("setup", {
        name: "NotAllowedError",
      }).message,
    ).toBe("Passkey setup was canceled.");
    expect(
      passkeyTestInternals.normalizePasskeyCeremonyError("sign-in", {
        name: "AbortError",
      }).message,
    ).toBe("Passkey sign-in was canceled.");
  });

  it("explains native passkey cancellation as an app-associated-domain issue", () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });

    expect(
      passkeyTestInternals.normalizePasskeyCeremonyError("setup", {
        name: "NotAllowedError",
      }).message,
    ).toContain("could not complete in the Starlane app");
    expect(
      passkeyTestInternals.normalizePasskeyCeremonyError("sign-in", {
        name: "AbortError",
      }).message,
    ).toContain("sign-in could not complete in the Starlane app");
  });

  it("normalizes passkey origin and duplicate-credential ceremony errors", () => {
    expect(
      passkeyTestInternals.normalizePasskeyCeremonyError("setup", {
        name: "SecurityError",
      }).message,
    ).toBe(
      "Passkeys are blocked for this app origin. Check the server URL and WebAuthn origin configuration.",
    );
    expect(
      passkeyTestInternals.normalizePasskeyCeremonyError("setup", {
        name: "InvalidStateError",
      }).message,
    ).toBe("A passkey may already be registered on this device.");
  });

  it("normalizes unsupported passkey ceremony errors for setup and sign-in", () => {
    expect(
      passkeyTestInternals.normalizePasskeyCeremonyError("setup", {
        name: "NotSupportedError",
      }).message,
    ).toBe("This browser or authenticator cannot create a passkey for Starlane.");
    expect(
      passkeyTestInternals.normalizePasskeyCeremonyError("sign-in", {
        name: "NotSupportedError",
      }).message,
    ).toBe("This browser or authenticator cannot use a passkey for Starlane.");
  });
});
