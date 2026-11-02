import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ownBrowserImagePreview,
  collectAbandonedBrowserImagePreviews,
} from "./browserAttachmentImageOwnership";
const A = "image-11111111-1111-4111-8111-111111111111";
function fixture() {
  const files = new Map<string, { kind: string }>([
    [A, { kind: "file" }],
    ["unrelated", { kind: "file" }],
    ["starlane-preview-legacy", { kind: "file" }],
    ["image-22222222-2222-4222-8222-222222222222", { kind: "directory" }],
  ]);
  const held = new Set<string>();
  const rootRemove = vi.fn(async () => {});
  const directory = {
    entries: async function* () {
      yield* files;
    },
    removeEntry: vi.fn(async (name: string) => {
      files.delete(name);
    }),
  };
  const manager = {
    request: vi.fn(
      async (
        name: string,
        options: { ifAvailable?: boolean },
        fn: (lock: unknown) => Promise<unknown>,
      ) => {
        if (held.has(name)) {
          if (options.ifAvailable) return fn(null);
          throw new Error("unexpected queued lease");
        }
        held.add(name);
        try {
          return await fn({ name });
        } finally {
          held.delete(name);
        }
      },
    ),
  };
  const root = { getDirectoryHandle: vi.fn(async () => directory), removeEntry: rootRemove };
  vi.stubGlobal("navigator", { locks: manager });
  return { files, held, root, directory, manager, rootRemove };
}
afterEach(() => vi.unstubAllGlobals());
describe("browser-owned preview collection", () => {
  it("preserves another live context, then deletes only exact leased-namespace files after release", async () => {
    const f = fixture();
    f.held.add("starlane-image-preview-v1:" + A);
    expect(await collectAbandonedBrowserImagePreviews(f.root as never)).toMatchObject({
      removed: 0,
      busy: 1,
    });
    expect(f.files.has(A)).toBe(true);
    f.held.clear();
    expect(await collectAbandonedBrowserImagePreviews(f.root as never)).toMatchObject({
      removed: 1,
      busy: 0,
    });
    expect([...f.files.keys()]).toEqual([
      "unrelated",
      "starlane-preview-legacy",
      "image-22222222-2222-4222-8222-222222222222",
    ]);
    expect(f.rootRemove).not.toHaveBeenCalled();
  });
  it("holds exclusive authority through asynchronous removal, and releases once", async () => {
    const f = fixture();
    const owner = await ownBrowserImagePreview(f.root as never);
    expect(f.held.has("starlane-image-preview-v1:" + owner.name)).toBe(true);
    let finish!: () => void;
    f.directory.removeEntry.mockImplementation(
      async () =>
        new Promise<void>((r) => {
          finish = r;
        }),
    );
    const released = owner.release();
    expect(owner.release()).toBe(released);
    await Promise.resolve();
    expect(f.held.size).toBe(1);
    finish();
    await released;
    expect(f.held.size).toBe(0);
    expect(f.directory.removeEntry).toHaveBeenCalledOnce();
  });
  it("unlocks failed deletion for a later collector without pretending deletion succeeded", async () => {
    const f = fixture();
    const owner = await ownBrowserImagePreview(f.root as never);
    f.directory.removeEntry.mockRejectedValueOnce(new DOMException("denied", "UnknownError"));
    await expect(owner.release()).rejects.toThrow("denied");
    expect(f.held.size).toBe(0);
  });
  it("cannot collect unleased legacy fallback when Web Locks are unavailable", async () => {
    const f = fixture();
    vi.stubGlobal("navigator", {});
    expect(await collectAbandonedBrowserImagePreviews(f.root as never)).toEqual({
      removed: 0,
      busy: 0,
      examined: 0,
      supported: false,
    });
    const owner = await ownBrowserImagePreview(f.root as never);
    expect(owner.name).toMatch(/^starlane-preview-/);
    await owner.release();
    expect(f.rootRemove).toHaveBeenCalledWith(owner.name);
    expect(f.root.getDirectoryHandle).not.toHaveBeenCalled();
  });
  it("limits collection to128 entries, preserving overflow for a future pass", async () => {
    const f = fixture();
    f.files.clear();
    for (let i = 0; i < 129; i++)
      f.files.set("image-" + String(i).padStart(8, "0") + "-1111-4111-8111-111111111111", {
        kind: "file",
      });
    expect(await collectAbandonedBrowserImagePreviews(f.root as never)).toMatchObject({
      removed: 128,
      examined: 128,
    });
    expect(f.files.size).toBe(1);
  });
});
