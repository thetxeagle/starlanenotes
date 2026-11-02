import { afterEach, describe, expect, it, vi } from "vitest";
import {
  addDocumentLocationChangedListener,
  DOCUMENT_LOCATION_CHANGED_EVENT,
  emitDocumentLocationChanged,
} from "./documentLocationEvents";

class TestCustomEvent<T> extends Event {
  detail: T;

  constructor(type: string, init: CustomEventInit<T>) {
    super(type);
    this.detail = init.detail as T;
  }
}

describe("document location events", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("publishes location metadata and removes listeners on cleanup", () => {
    const target = new EventTarget();
    vi.stubGlobal("window", target);
    vi.stubGlobal("CustomEvent", TestCustomEvent);
    const received: unknown[] = [];
    const unsubscribe = addDocumentLocationChangedListener((detail) => received.push(detail));
    const detail = {
      spaceId: "space-1",
      documentId: "document-1",
      folderId: null,
      parentId: null,
    };

    emitDocumentLocationChanged(detail);
    unsubscribe();
    emitDocumentLocationChanged({ ...detail, folderId: "folder-2" });

    expect(received).toEqual([detail]);
    expect(target).toBeInstanceOf(EventTarget);
    expect(DOCUMENT_LOCATION_CHANGED_EVENT).toBe("starlane:document-location-changed");
  });
});
