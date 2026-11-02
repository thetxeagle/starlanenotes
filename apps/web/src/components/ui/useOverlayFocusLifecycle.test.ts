import { describe, expect, it } from "vitest";
import { ownsOverlayKeyboard } from "./useOverlayFocusLifecycle";

function surface(parent?: HTMLElement, connected = true) {
  const node = {
    isConnected: connected,
    contains: (candidate: HTMLElement) => candidate === node || ancestors.get(candidate)?.includes(node as unknown as HTMLElement),
  } as unknown as HTMLElement;
  ancestors.set(node, parent ? [parent, ...(ancestors.get(parent) ?? [])] : []);
  return { current: node };
}
const ancestors = new Map<HTMLElement, HTMLElement[]>();

describe("overlay keyboard ownership", () => {
  it("gives a nested child ownership even when React registers its parent later", () => {
    const parent = surface();
    const child = surface(parent.current);
    expect(ownsOverlayKeyboard(parent, [child, parent])).toBe(false);
    expect(ownsOverlayKeyboard(child, [child, parent])).toBe(true);
  });
  it("keeps the child owner when it opens after the parent", () => {
    const parent = surface();
    const child = surface(parent.current);
    expect(ownsOverlayKeyboard(parent, [parent, child])).toBe(false);
    expect(ownsOverlayKeyboard(child, [parent, child])).toBe(true);
    expect(ownsOverlayKeyboard(parent, [parent])).toBe(true);
  });
  it("gives the latest sibling ownership and ignores disconnected overlays", () => {
    const first = surface();
    const second = surface();
    const removed = surface(undefined, false);
    expect(ownsOverlayKeyboard(first, [first, second])).toBe(false);
    expect(ownsOverlayKeyboard(second, [first, second, removed])).toBe(true);
    expect(ownsOverlayKeyboard(removed, [first, removed])).toBe(false);
  });
});
