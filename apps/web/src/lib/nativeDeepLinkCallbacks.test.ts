import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  consumePendingNativeCallback,
  dispatchNativeCallbackUrl,
  NATIVE_AUTH_HANDOFF_EVENT,
  NATIVE_GOOGLE_DRIVE_CALLBACK_EVENT,
} from "./nativeDeepLinkCallbacks";

describe("native deep-link callbacks", () => {
  beforeEach(() => {
    const storage = new Map<string, string>();
    vi.stubGlobal("sessionStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key),
      clear: () => storage.clear(),
    });
    vi.stubGlobal("document", new EventTarget());
    vi.stubGlobal(
      "CustomEvent",
      class<T> extends Event {
        detail: T;

        constructor(type: string, init: CustomEventInit<T>) {
          super(type);
          this.detail = init.detail as T;
        }
      },
    );
  });

  afterEach(() => vi.unstubAllGlobals());

  it("dispatches and retains a verified auth handoff until its consumer is ready", () => {
    const listener = vi.fn();
    document.addEventListener(NATIVE_AUTH_HANDOFF_EVENT, listener);
    const url = "starlane://auth/handoff?handoff_id=h-1&code=c-1";

    expect(dispatchNativeCallbackUrl(url)).toBe("auth-handoff");
    expect(listener).toHaveBeenCalledOnce();
    expect(consumePendingNativeCallback("auth-handoff")).toBe(url);
    expect(consumePendingNativeCallback("auth-handoff")).toBeNull();

    document.removeEventListener(NATIVE_AUTH_HANDOFF_EVENT, listener);
  });

  it("dispatches and retains a Google Drive callback until Admin is ready", () => {
    const listener = vi.fn();
    document.addEventListener(NATIVE_GOOGLE_DRIVE_CALLBACK_EVENT, listener);
    const url =
      "starlane://provider/google-drive/callback?code=oauth-code&state=state-1";

    expect(dispatchNativeCallbackUrl(url)).toBe("google-drive");
    expect(listener).toHaveBeenCalledOnce();
    expect(consumePendingNativeCallback("google-drive")).toBe(url);

    document.removeEventListener(NATIVE_GOOGLE_DRIVE_CALLBACK_EVENT, listener);
  });

  it("ignores unrelated or lookalike callback URLs", () => {
    expect(dispatchNativeCallbackUrl("https://auth/handoff?code=nope")).toBeNull();
    expect(dispatchNativeCallbackUrl("starlane://provider/dropbox/callback")).toBeNull();
    expect(dispatchNativeCallbackUrl("not a URL")).toBeNull();
  });

  it("deduplicates the same callback when an iOS Opened event races getCurrent", () => {
    const listener = vi.fn();
    document.addEventListener(NATIVE_AUTH_HANDOFF_EVENT, listener);
    const url = "starlane://auth/handoff?handoff_id=h-race&code=c-race";

    dispatchNativeCallbackUrl(url);
    dispatchNativeCallbackUrl(url);

    expect(listener).toHaveBeenCalledOnce();
    document.removeEventListener(NATIVE_AUTH_HANDOFF_EVENT, listener);
  });
});
