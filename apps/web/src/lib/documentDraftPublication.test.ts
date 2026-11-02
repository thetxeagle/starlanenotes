import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { dataClient, captureDraftSaveSelection, type DraftRecord } from "./dataClient";
import { setCurrentLaunchMode } from "./launchMode";
import { setStorageMode, setSyncProvider } from "./storageProviders";
import { listOfflineServerMutations } from "./offlineServerCache";
import { mergeDraftPublication } from "./documentDraftPublication";
import type { DraftPublicationIntent, DraftPublicationSource } from "./documentDraftPublication";
const token = (account = "owner") =>
  `e30.${btoa(JSON.stringify({ sub: account, sid: `session-${account}`, token_type: "access" }))}.fixture`;
const saved: DraftRecord = {
  id: "draft",
  space_id: "space",
  document_id: "page",
  owner_id: "owner",
  owner_name: "Owner",
  document_title: "Page",
  title: "Saved title",
  content: "Exact saved body",
  icon: "icon",
  cover_image: "cover",
  access_role: "owner",
  can_publish: true,
  created_at: "created",
  updated_at: "updated",
  base_updated_at: "base",
  source_digest: "a".repeat(64),
};
const source: DraftPublicationSource = {
  schema_version: 1,
  space_id: "space",
  document_id: "page",
  draft_id: "draft",
  draft_source_digest: "a".repeat(64),
  page_source_digest: "b".repeat(64),
  page_published: true,
  page_updated_at: "published-base",
  draft: {
    title: saved.title,
    content: saved.content,
    icon: saved.icon!,
    cover_image: saved.cover_image!,
  },
};
const receipt = (intent: DraftPublicationIntent, disposition = "applied") => ({
  ...intent.request,
  space_id: "space",
  document_id: "page",
  draft_id: "draft",
  published_source_digest: "c".repeat(64),
  published_at: "published-stamp",
  publication: intent.expectedPublication,
  disposition,
});
let storage: Map<string, string>;
beforeEach(() => {
  vi.unstubAllGlobals();
  storage = new Map();
  vi.stubGlobal("localStorage", {
    get length() {
      return storage.size;
    },
    key: (i: number) => [...storage.keys()][i] ?? null,
    getItem: (k: string) => storage.get(k) ?? null,
    setItem: (k: string, v: string) => {
      storage.set(k, v);
    },
    removeItem: (k: string) => {
      storage.delete(k);
    },
  });
  vi.stubGlobal("window", { localStorage: globalThis.localStorage, dispatchEvent: vi.fn() });
  let tail: Promise<unknown> = Promise.resolve();
  vi.stubGlobal("navigator", {
    onLine: true,
    locks: {
      request: (_name: string, run: () => Promise<unknown>) => {
        const next = tail.then(run);
        tail = next.catch(() => {});
        return next;
      },
    },
  });
  setCurrentLaunchMode("server");
  setStorageMode("server");
  setSyncProvider("none");
  api.setToken(token());
  api.setRefreshToken("fixture-refresh");
});
const prepare = async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(source), { status: 200 })),
  );
  return dataClient.prepareDocumentDraftPublication(
    "space",
    "page",
    saved,
    captureDraftSaveSelection("owner"),
  );
};
describe("actual durable publication client", () => {
  it("prepares an immutable reviewed source without publishing or storing credentials", async () => {
    const caller = { ...saved };
    const selection = captureDraftSaveSelection("owner");
    const fetcher = vi.fn(
      async (_url: RequestInfo | URL, _init?: RequestInit) =>
        new Response(JSON.stringify(source), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetcher);
    const pending = dataClient.prepareDocumentDraftPublication("space", "page", caller, selection);
    caller.content = "Caller mutation";
    const intent = await pending;
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(fetcher.mock.calls[0]?.length).toBeGreaterThan(0);
    expect(intent.source.draft.content).toBe(saved.content);
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0].state).toBe("prepared");
    const journal = [...storage.entries()].filter(([key]) =>
      key.startsWith("starlane_draft_publication_v1:"),
    )[0][1];
    expect(journal).not.toContain(token());
    expect(journal).not.toContain("fixture-refresh");
    expect(listOfflineServerMutations()).toEqual([]);
  });
  it.each(["draft_source_digest", "page_source_digest", "title", "content", "icon", "cover_image"])(
    "does not silently prepare changed %s",
    async (field) => {
      const changed = JSON.parse(JSON.stringify(source));
      if (field === "draft_source_digest") changed[field] = "d".repeat(64);
      else if (field === "page_source_digest") changed[field] = "malformed";
      else changed.draft[field] = "Changed";
      vi.stubGlobal(
        "fetch",
        vi.fn(async () => new Response(JSON.stringify(changed), { status: 200 })),
      );
      await expect(
        dataClient.prepareDocumentDraftPublication(
          "space",
          "page",
          saved,
          captureDraftSaveSelection("owner"),
        ),
      ).rejects.toThrow();
      expect(dataClient.inspectDocumentDraftPublications("space", "page")).toEqual([]);
    },
  );
  it("confirms exact receipt and retains the operation as historical evidence", async () => {
    const intent = await prepare();
    const fetcher = vi.fn(async (_url: RequestInfo | URL, init?: RequestInit) => {
      expect(JSON.parse(String(init?.body))).toEqual(intent.request);
      return new Response(JSON.stringify(receipt(intent)), { status: 200 });
    });
    vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.publishPreparedDocumentDraft(intent)).resolves.toMatchObject({
      operation_id: intent.operation_id,
      disposition: "applied",
    });
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0]).toMatchObject({
      state: "confirmed",
      receipt: receipt(intent),
    });
    expect(listOfflineServerMutations()).toEqual([]);
  });
  it("retries a lost acknowledgement after draft consumption with no save or new source GET", async () => {
    const intent = await prepare();
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Lost after commit")));
    await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow(
      "outcome unknown",
    );
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0].state).toBe("unknown");
    const fetcher = vi.fn(async (_url: RequestInfo | URL, init?: RequestInit) => {
      expect(init?.method).toBe("POST");
      expect(JSON.parse(String(init?.body))).toEqual(intent.request);
      return new Response(JSON.stringify(receipt(intent, "historical")), { status: 200 });
    });
    vi.stubGlobal("fetch", fetcher);
    await expect(
      dataClient.retryDocumentDraftPublication(intent.operation_id),
    ).resolves.toMatchObject({ disposition: "historical" });
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it.each(["operation", "source", "body", "status"])(
    "retains an ambiguous %s response without an ACK",
    async (drift) => {
      const intent = await prepare();
      const response = receipt(intent);
      if (drift === "operation") response.operation_id = crypto.randomUUID();
      if (drift === "source") response.draft_source_digest = "d".repeat(64);
      if (drift === "body")
        response.publication = { ...response.publication, content: "Other body" };
      vi.stubGlobal(
        "fetch",
        vi.fn(
          async () =>
            new Response(JSON.stringify(response), { status: drift === "status" ? 202 : 200 }),
        ),
      );
      await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow(
        "outcome unknown",
      );
      expect(dataClient.inspectDocumentDraftPublications("space", "page")[0].state).toBe("unknown");
    },
  );
  it("holds a successful response under account drift and never admits it to another account", async () => {
    const intent = await prepare();
    let release!: (v: Response) => void;
    let signal!: () => void;
    const ready = new Promise<void>((resolve) => {
      signal = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(() => {
        signal();
        return new Promise<Response>((resolve) => {
          release = resolve;
        });
      }),
    );
    const task = dataClient.publishPreparedDocumentDraft(intent);
    await ready;
    api.setToken(token("other"));
    release(new Response(JSON.stringify(receipt(intent)), { status: 200 }));
    await expect(task).rejects.toThrow("outcome unknown");
    expect(dataClient.inspectDocumentDraftPublications("space", "page")).toEqual([]);
    api.setToken(token());
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0].state).toBe("unknown");
  });
  it("cancels only an undispatched preparation and preserves its exact archived source", async () => {
    const intent = await prepare();
    await dataClient.cancelPreparedDocumentDraftPublication(intent.operation_id);
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0]).toMatchObject({
      state: "cancelled",
      intent,
    });
    await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow(
      "cancelled or rejected",
    );
    const next = await prepare();
    expect(next.operation_id).not.toBe(intent.operation_id);
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Lost")));
    await expect(dataClient.publishPreparedDocumentDraft(next)).rejects.toThrow();
    await expect(
      dataClient.cancelPreparedDocumentDraftPublication(next.operation_id),
    ).rejects.toThrow("Only an undispatched");
  });
  it("blocks a second operation while the original request is prepared or unknown", async () => {
    const intent = await prepare();
    await expect(prepare()).rejects.toThrow("already exists");
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Lost")));
    await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow();
    await expect(prepare()).rejects.toThrow("already exists");
  });
  it("rejects direct generic server publication before any fetch", async () => {
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.publishDocumentDraft("space", "page", "draft")).rejects.toThrow(
      "source-bound",
    );
    expect(fetcher).not.toHaveBeenCalled();
  });
  it("requires durable preparation before network publication", async () => {
    const intent = await prepare();
    const original = localStorage.setItem;
    localStorage.setItem = () => {
      throw Error("Quota exceeded");
    };
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow("Quota exceeded");
    expect(fetcher).not.toHaveBeenCalled();
    localStorage.setItem = original;
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0].state).toBe("prepared");
  });
  it("does not redispatch mutated review bytes", async () => {
    const intent = await prepare();
    intent.request.page_source_digest = "d".repeat(64);
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    expect(() => dataClient.publishPreparedDocumentDraft(intent)).toThrow("source changed");
    expect(fetcher).not.toHaveBeenCalled();
  });
  it("accepts server JSON key ordering while preserving exact field bytes", async () => {
    const sortedSource = {
      ...source,
      draft: {
        content: saved.content,
        cover_image: saved.cover_image!,
        icon: saved.icon!,
        title: saved.title,
      },
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify(sortedSource), { status: 200 })),
    );
    const intent = await dataClient.prepareDocumentDraftPublication(
      "space",
      "page",
      saved,
      captureDraftSaveSelection("owner"),
    );
    const response = receipt(intent);
    response.publication = {
      icon: saved.icon!,
      title: saved.title,
      cover_image: saved.cover_image!,
      content: saved.content,
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify(response), { status: 200 })),
    );
    await expect(dataClient.publishPreparedDocumentDraft(intent)).resolves.toMatchObject({
      operation_id: intent.operation_id,
    });
  });

  it("retains unclassified409 commit ambiguity instead of authorizing a new publication", async () => {
    const intent = await prepare();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ message: "Publication commit outcome requires review" }), {
            status: 409,
          }),
      ),
    );
    await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow(
      "outcome unknown",
    );
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0].state).toBe("unknown");
    await expect(prepare()).rejects.toThrow("already exists");
  });

  it("preserves a confirmed receipt if later explicit receipt review loses access", async () => {
    const intent = await prepare();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify(receipt(intent)), { status: 200 })),
    );
    await dataClient.publishPreparedDocumentDraft(intent);
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ message: "Unavailable" }), { status: 404 })),
    );
    await expect(dataClient.retryDocumentDraftPublication(intent.operation_id)).rejects.toThrow();
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0]).toMatchObject({
      state: "confirmed",
      receipt: receipt(intent),
    });
  });

  it("fences context changes while awaiting response text, retaining original request bytes", async () => {
    const intent = await prepare();
    let release!: (text: string) => void;
    let signal!: () => void;
    const ready = new Promise<void>((resolve) => {
      signal = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        status: 200,
        ok: true,
        text: () => {
          signal();
          return new Promise<string>((resolve) => {
            release = resolve;
          });
        },
      })),
    );
    const task = dataClient.publishPreparedDocumentDraft(intent);
    await ready;
    setSyncProvider("google-drive");
    release(JSON.stringify(receipt(intent)));
    await expect(task).rejects.toThrow("outcome unknown");
    setSyncProvider("none");
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0]).toMatchObject({
      state: "unknown",
      intent,
    });
  });

  it("archives a proven first-attempt source rejection without rebinding its operation", async () => {
    const intent = await prepare();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ message: "source_conflict: reviewed page changed" }), {
            status: 409,
          }),
      ),
    );
    await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow("rejected");
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0]).toMatchObject({
      state: "rejected",
      intent,
    });
    await expect(dataClient.retryDocumentDraftPublication(intent.operation_id)).rejects.toThrow(
      "cancelled or rejected",
    );
    const next = await prepare();
    expect(next.operation_id).not.toBe(intent.operation_id);
  });

  it("serializes overlapping publication calls without sending an original operation twice", async () => {
    const intent = await prepare();
    let release!: (response: Response) => void;
    let signal!: () => void;
    const ready = new Promise<void>((resolve) => {
      signal = resolve;
    });
    const fetcher = vi.fn(() => {
      signal();
      return new Promise<Response>((resolve) => {
        release = resolve;
      });
    });
    vi.stubGlobal("fetch", fetcher);
    const first = dataClient.publishPreparedDocumentDraft(intent);
    await ready;
    const second = dataClient.publishPreparedDocumentDraft(intent);
    const rejected = expect(second).rejects.toThrow("retry that exact request");
    release(new Response(JSON.stringify(receipt(intent)), { status: 200 }));
    await first;
    await rejected;
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("retains an undispatched preparation offline and retries that same identity after reconnect", async () => {
    const intent = await prepare();
    vi.stubGlobal("navigator", {
      onLine: false,
      locks: { request: (_name: string, run: () => Promise<unknown>) => run() },
    });
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow("not dispatched");
    expect(fetcher).not.toHaveBeenCalled();
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0].state).toBe("prepared");
    vi.stubGlobal("navigator", {
      onLine: true,
      locks: { request: (_name: string, run: () => Promise<unknown>) => run() },
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify(receipt(intent)), { status: 200 })),
    );
    await expect(
      dataClient.retryDocumentDraftPublication(intent.operation_id),
    ).resolves.toMatchObject({ operation_id: intent.operation_id });
  });

  it("does not weaken publication serialization when Web Locks are unavailable", async () => {
    const intent = await prepare();
    vi.stubGlobal("navigator", { onLine: true });
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow("Web Locks");
    expect(fetcher).not.toHaveBeenCalled();
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0]).toMatchObject({
      state: "prepared",
      intent,
    });
  });

  it("retains original evidence when storing the acknowledged receipt fails", async () => {
    const intent = await prepare();
    const set = localStorage.setItem;
    localStorage.setItem = (key, value) => {
      if (
        key.startsWith("starlane_draft_publication_v1:") &&
        JSON.parse(value).state === "confirmed"
      )
        throw Error("Receipt quota");
      set(key, value);
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify(receipt(intent)), { status: 200 })),
    );
    await expect(dataClient.publishPreparedDocumentDraft(intent)).rejects.toThrow(
      "outcome unknown",
    );
    expect(dataClient.inspectDocumentDraftPublications("space", "page")[0]).toMatchObject({
      state: "unknown",
      intent,
    });
    localStorage.setItem = set;
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () => new Response(JSON.stringify(receipt(intent, "historical")), { status: 200 }),
      ),
    );
    await expect(
      dataClient.retryDocumentDraftPublication(intent.operation_id),
    ).resolves.toMatchObject({ operation_id: intent.operation_id, disposition: "historical" });
  });
});

describe("mergeDraftPublication", () => {
  it("preserves the loaded document while applying published draft metadata and response times", () => {
    const current = {
      id: "doc-1",
      title: "Old title",
      icon: "book",
      cover_image: "old-cover.png",
      folder_id: "folder-1",
      space_id: "space-1",
      created_by: "author-1",
      created_at: "2026-09-01T09:00:00Z",
      updated_at: "2026-10-01T09:00:00Z",
      locked: true,
      locked_by: "editor-2",
      public: true,
      audience: ["member", "guest"],
    };
    const publishedAt = "2026-10-05T22:00:00Z";

    expect(
      mergeDraftPublication(
        current,
        { title: "Published title", icon: "rocket", cover_image: "new-cover.png" },
        { document_id: "doc-1", published_at: publishedAt },
        "publisher-1",
      ),
    ).toEqual({
      ...current,
      title: "Published title",
      icon: "rocket",
      cover_image: "new-cover.png",
      published_at: publishedAt,
      updated_at: publishedAt,
      last_updated_by: "publisher-1",
    });
  });

  it("uses an explicit updated_at and keeps the loaded ID and existing fields", () => {
    const current = {
      id: "doc-1",
      title: "Old title",
      public: false,
      published_at: "2026-10-01T09:00:00Z",
      updated_at: "2026-10-01T09:00:00Z",
    };
    const updatedAt = "2026-10-05T22:01:00Z";

    expect(
      mergeDraftPublication(
        current,
        { title: "New title" },
        {
          document_id: "unexpected-id",
          published_at: "2026-10-05T22:00:00Z",
          updated_at: updatedAt,
        },
        "publisher-1",
      ),
    ).toEqual({
      ...current,
      title: "New title",
      icon: undefined,
      cover_image: undefined,
      published_at: "2026-10-05T22:00:00Z",
      updated_at: updatedAt,
      last_updated_by: "publisher-1",
    });
  });
});
