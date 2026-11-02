import { describe, expect, it, vi } from "vitest";
import { NATIVE_AUTH_HANDOFF_EVENT } from "@/lib/nativeDeepLinkCallbacks";
import {
  dispatchCurrentNativeCallbackUrls,
  dispatchPendingNativeCallbackUrl,
  getTauriDeepLinkRoutePath,
} from "./useTauriEvents";

describe("getTauriDeepLinkRoutePath", () => {
  it("builds encoded document and space routes from native deep links", () => {
    expect(
      getTauriDeepLinkRoutePath({
        action: "open_note",
        space_id: "space/one",
        doc_id: "doc/two",
      }),
    ).toBe("/spaces/space%2Fone/docs/doc%2Ftwo");

    expect(
      getTauriDeepLinkRoutePath({
        action: "open_space",
        space_id: "space one",
      }),
    ).toBe("/spaces/space%20one/docs");
  });

  it("ignores incomplete or non-navigation native deep links", () => {
    expect(getTauriDeepLinkRoutePath({ action: "open_note", space_id: "space-1" })).toBeNull();
    expect(getTauriDeepLinkRoutePath({ action: "search", query: "roadmap" })).toBeNull();
  });

  it("replays the current retained native callback after an iOS resume", async () => {
    const pending = new Map<string, string>();
    vi.stubGlobal("document", new EventTarget());
    vi.stubGlobal("sessionStorage", {
      getItem: (key: string) => pending.get(key) ?? null,
      setItem: (key: string, value: string) => pending.set(key, value),
      removeItem: (key: string) => pending.delete(key),
    });
    const callback = "starlane://auth/handoff?handoff_id=h-native&code=c-native";
    const listener = vi.fn();
    document.addEventListener(NATIVE_AUTH_HANDOFF_EVENT, listener);

    const count = await dispatchCurrentNativeCallbackUrls(
      vi.fn().mockResolvedValue([callback]),
    );

    expect(count).toBe(1);
    expect(listener).toHaveBeenCalledTimes(1);
    document.removeEventListener(NATIVE_AUTH_HANDOFF_EVENT, listener);
  });

  it("consumes a callback retained by the native iOS bridge", async () => {
    const pending = new Map<string, string>();
    vi.stubGlobal("document", new EventTarget());
    vi.stubGlobal("sessionStorage", {
      getItem: (key: string) => pending.get(key) ?? null,
      setItem: (key: string, value: string) => pending.set(key, value),
      removeItem: (key: string) => pending.delete(key),
    });
    const callback = "starlane://auth/handoff?handoff_id=h&code=c";
    const listener = vi.fn();
    document.addEventListener(NATIVE_AUTH_HANDOFF_EVENT, listener);

    const consumed = await dispatchPendingNativeCallbackUrl(
      vi.fn().mockResolvedValue(callback),
    );

    expect(consumed).toBe(true);
    expect(listener).toHaveBeenCalledTimes(1);
    document.removeEventListener(NATIVE_AUTH_HANDOFF_EVENT, listener);
  });
});
