import { afterEach, describe, expect, it, vi } from "vitest";
import {
  previewBrowserAttachmentImage,
  releaseBrowserAttachmentImage,
} from "./browserAttachmentImage";
const state = vi.hoisted(() => ({ valid: true, selected: null as unknown }));
vi.mock("./api", () => ({
  api: { getToken: () => "original" },
  getApiBase: () => "https://owned.invalid/api/v1",
  captureOfflineServerContext: () => ({
    bearer: "original",
    origin: {
      version: 1,
      serverBase: "https://owned.invalid/api/v1",
      accountId: "owner",
      sessionId: "session",
      cacheScope: "session-owned",
      storageMode: "server",
      provider: "none",
    },
  }),
}));
vi.mock("./offlineReplayOrigin", () => ({
  sameOfflineReplayOrigin: (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b),
  verifyOfflineReplayContext: () => {
    if (!state.valid) throw new Error("Original session changed");
  },
}));
vi.mock("./nativeAttachmentProduct", () => ({
  fetchExactAttachmentVersion: async () => state.selected,
}));
const selection = () => ({
  origin: {
    version: 1 as const,
    serverBase: "https://owned.invalid/api/v1",
    accountId: "owner",
    sessionId: "session",
    cacheScope: "session-owned",
    storageMode: "server" as const,
    provider: "none" as const,
  },
  serverPage: { space_id: "space", document_id: "page" },
  rawVersion: JSON.stringify({
    version_id: "11111111-1111-4111-8111-111111111111",
    logical_attachment_id: "22222222-2222-4222-8222-222222222222",
    byte_length: 3,
    binary_sha256: "a".repeat(64),
    mime_type: "image/png",
  }),
});
function storage() {
  const writer = {
    write: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    abort: vi.fn(async () => {}),
  };
  const handle = {
    createWritable: vi.fn(async () => writer),
    getFile: vi.fn(async () => new File([new Uint8Array([0, 1, 255])], "owned.bin")),
  };
  const root = { getFileHandle: vi.fn(async () => handle), removeEntry: vi.fn(async () => {}) };
  vi.stubGlobal("navigator", { storage: { getDirectory: async () => root } });
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(new Uint8Array([0, 1, 255]))),
  );
  const s = selection();
  state.selected = s;
  return { s, root, writer, handle };
}
afterEach(() => {
  state.valid = true;
  vi.unstubAllGlobals();
});
describe("browser exact image File lifetime", () => {
  it("keeps a cross-tab lease through publication and explicit image release", async () => {
    const x = storage();
    const held = new Set<string>();
    const directory = { ...x.root, entries: async function* () {} };
    const root = { getDirectoryHandle: async () => directory };
    const locks = {
      request: async (name: string, _options: unknown, fn: () => Promise<void>) => {
        held.add(name);
        try {
          await fn();
        } finally {
          held.delete(name);
        }
      },
    };
    vi.stubGlobal("navigator", { storage: { getDirectory: async () => root }, locks });
    const url = await previewBrowserAttachmentImage(x.s);
    expect(held.size).toBe(1);
    expect(x.root.removeEntry).not.toHaveBeenCalled();
    releaseBrowserAttachmentImage(url);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(held.size).toBe(0);
    expect(x.root.removeEntry).toHaveBeenCalledOnce();
  });
  it("holds the backing file until the image URL is explicitly released", async () => {
    const x = storage();
    const url = await previewBrowserAttachmentImage(x.s);
    expect(x.writer.close).toHaveBeenCalledOnce();
    expect(x.root.removeEntry).not.toHaveBeenCalled();
    expect(url).toMatch(/^blob:/);
    releaseBrowserAttachmentImage(url);
    expect(x.root.removeEntry).toHaveBeenCalledOnce();
  });
  it("aborts a writer acquired immediately before original context changes", async () => {
    const x = storage();
    x.handle.createWritable.mockImplementation(async () => {
      state.valid = false;
      return x.writer;
    });
    await expect(previewBrowserAttachmentImage(x.s)).rejects.toThrow("session changed");
    expect(x.writer.abort).toHaveBeenCalledOnce();
    expect(x.writer.close).not.toHaveBeenCalled();
    expect(x.root.removeEntry).toHaveBeenCalledOnce();
  });
  it("keeps failed private close outcomes out of image URLs and cleans the owned entry", async () => {
    const x = storage();
    x.writer.close.mockRejectedValue(new Error("lost close acknowledgement"));
    await expect(previewBrowserAttachmentImage(x.s)).rejects.toThrow("outcome unknown");
    expect(x.handle.getFile).not.toHaveBeenCalled();
    expect(x.root.removeEntry).toHaveBeenCalledOnce();
  });
  it("refuses unavailable private file storage before opening the HTTP body", async () => {
    const x = storage();
    vi.stubGlobal("navigator", { storage: {} });
    await expect(previewBrowserAttachmentImage(x.s)).rejects.toThrow("cannot stream");
    expect(fetch).not.toHaveBeenCalled();
  });
});
