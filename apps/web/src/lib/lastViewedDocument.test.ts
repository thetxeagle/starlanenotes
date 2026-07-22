import { describe, expect, it, vi } from "vitest";
import {
  LAST_VIEWED_DOCUMENT_KEY,
  buildDocumentRoutePath,
  clearLastViewedDocumentRoute,
  parseDocumentRoutePath,
  readLastViewedDocumentRoute,
  rememberLastViewedDocumentRoute,
} from "./lastViewedDocument";

function createStorage(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial));
  return {
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      values.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      values.delete(key);
    }),
  };
}

describe("lastViewedDocument", () => {
  it("builds and parses document routes", () => {
    const path = buildDocumentRoutePath("space one", "doc/two");
    expect(path).toBe("/spaces/space%20one/docs/doc%2Ftwo");
    expect(parseDocumentRoutePath(path)).toEqual({
      spaceId: "space one",
      docId: "doc/two",
      path,
    });
  });

  it("ignores non-document routes", () => {
    expect(parseDocumentRoutePath("/spaces/space-1/recent")).toBeNull();
    expect(parseDocumentRoutePath("/admin")).toBeNull();
    expect(parseDocumentRoutePath("/spaces/%/docs/doc-1")).toBeNull();
    expect(parseDocumentRoutePath("/spaces/%20%20/docs/doc-1")).toBeNull();
    expect(parseDocumentRoutePath("/spaces/space-1/docs/%20")).toBeNull();
  });

  it("persists and reads the last viewed document route", () => {
    const storage = createStorage();
    rememberLastViewedDocumentRoute("space-1", "doc-1", storage);

    expect(storage.setItem).toHaveBeenCalledWith(
      LAST_VIEWED_DOCUMENT_KEY,
      JSON.stringify({
        spaceId: "space-1",
        docId: "doc-1",
        path: "/spaces/space-1/docs/doc-1",
      }),
    );
    expect(readLastViewedDocumentRoute(storage)).toEqual({
      spaceId: "space-1",
      docId: "doc-1",
      path: "/spaces/space-1/docs/doc-1",
    });
  });

  it("clears malformed persisted routes", () => {
    const storage = createStorage({ [LAST_VIEWED_DOCUMENT_KEY]: "not-json" });

    expect(readLastViewedDocumentRoute(storage)).toBeNull();
    expect(storage.removeItem).toHaveBeenCalledWith(LAST_VIEWED_DOCUMENT_KEY);
  });

  it("clears persisted routes with invalid identity fields", () => {
    const storage = createStorage({
      [LAST_VIEWED_DOCUMENT_KEY]: JSON.stringify({ spaceId: "space-1", docId: "" }),
    });

    expect(readLastViewedDocumentRoute(storage)).toBeNull();
    expect(storage.removeItem).toHaveBeenCalledWith(LAST_VIEWED_DOCUMENT_KEY);
  });

  it("clears the remembered document route explicitly", () => {
    const storage = createStorage({
      [LAST_VIEWED_DOCUMENT_KEY]: JSON.stringify({ spaceId: "space-1", docId: "doc-1" }),
    });

    clearLastViewedDocumentRoute(storage);

    expect(storage.removeItem).toHaveBeenCalledWith(LAST_VIEWED_DOCUMENT_KEY);
    expect(readLastViewedDocumentRoute(storage)).toBeNull();
  });

  it("does not persist blank document route identities", () => {
    const storage = createStorage({
      [LAST_VIEWED_DOCUMENT_KEY]: JSON.stringify({ spaceId: "space-1", docId: "doc-1" }),
    });

    rememberLastViewedDocumentRoute(" ", "doc-2", storage);

    expect(storage.setItem).not.toHaveBeenCalled();
    expect(storage.removeItem).toHaveBeenCalledWith(LAST_VIEWED_DOCUMENT_KEY);
    expect(readLastViewedDocumentRoute(storage)).toBeNull();
  });
});
