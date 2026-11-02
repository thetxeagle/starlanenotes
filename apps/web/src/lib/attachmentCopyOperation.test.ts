import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SelectedAttachmentVersion } from "./nativeAttachmentProduct";
const mocks = vi.hoisted(() => ({
  fetch: vi.fn(),
  origin: {
    version: 1 as const,
    serverBase: "/api/v1",
    accountId: "original",
    sessionId: "original-session",
    cacheScope: "session-original",
    storageMode: "server" as const,
    provider: "none" as const,
  },
}));
vi.mock("./api", () => ({
  api: { getToken: () => "token", fetchOriginalContext: mocks.fetch },
  captureOfflineServerContext: () => ({
    origin: { ...mocks.origin },
    bearer: "token",
    refresh: null,
  }),
  getApiBase: () => "/api/v1",
}));
vi.mock("./offlineReplayOrigin", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./offlineReplayOrigin")>()),
  verifyOfflineReplayContext: vi.fn(),
}));
import {
  prepareAttachmentCopy,
  adoptReviewedAttachmentCopy,
  prepareLegacyAttachmentCopy,
  fetchLegacyAttachmentCopySource,
  runAttachmentCopy,
  markAttachmentCopyAdopted,
  retainedAttachmentCopies,
  validateAttachmentCopyOperation,
  type AttachmentCopyOperation,
} from "./attachmentCopyOperation";
const sourceId = "11111111-1111-4111-8111-111111111111",
  copiedId = "22222222-2222-4222-8222-222222222222",
  draftA = "33333333-3333-4333-8333-333333333333",
  draftB = "44444444-4444-4444-8444-444444444444";
const target = { spaceId: "space", documentId: "target", draftId: draftA };
const rawVersion =
  '{ "version_id":"' +
  sourceId +
  '", "logical_attachment_id":"' +
  sourceId +
  '", "version_number":2,"original_name":"exact.txt","mime_type":"text/plain","byte_length":5,"binary_sha256":"' +
  "a".repeat(64) +
  '","unknown":{"original":"retained"}}';
const selected: SelectedAttachmentVersion = {
  origin: mocks.origin,
  serverPage: { space_id: "space", document_id: "source" },
  rawVersion,
};
function result(draftId = draftA) {
  return {
    id: copiedId,
    document_id: "target",
    original_name: "exact.txt",
    filename: "stored.txt",
    mime_type: "text/plain",
    size_bytes: 5,
    url: "/attachment/copied",
    created_at: "2026-10-16T00:00:00Z",
    audience: {
      schema_version: 2 as const,
      draft_id: draftId,
      draft_created_at: "2026-10-16T00:00:00Z",
    },
  };
}
function storage() {
  const rows = new Map<string, AttachmentCopyOperation>();
  let refuseWrites = false;
  const db = {
    close: vi.fn(),
    transaction: () => {
      const tx = {
        oncomplete: null as null | (() => void),
        onabort: null as null | (() => void),
        onerror: null as null | (() => void),
        aborted: false,
        abort() {
          this.aborted = true;
          queueMicrotask(() => this.onabort?.());
        },
        objectStore: () => ({
          add(v: AttachmentCopyOperation) {
            if (refuseWrites) {
              tx.abort();
              return;
            }
            rows.set(v.id, structuredClone(v));
            queueMicrotask(() => {
              if (!tx.aborted) tx.oncomplete?.();
            });
          },
          put(v: AttachmentCopyOperation) {
            if (refuseWrites) {
              tx.abort();
              return;
            }
            rows.set(v.id, structuredClone(v));
          },
          get(id: string) {
            const request = {
              result: undefined as unknown,
              onsuccess: null as null | (() => void),
            };
            queueMicrotask(() => {
              request.result = structuredClone(rows.get(id));
              request.onsuccess?.();
              if (!tx.aborted) tx.oncomplete?.();
            });
            return request;
          },
          openCursor() {
            const values = Array.from(rows.values()),
              r = { result: null as unknown, onsuccess: null as null | (() => void) };
            let index = 0;
            const next = () =>
              queueMicrotask(() => {
                r.result =
                  index < values.length
                    ? { value: structuredClone(values[index++]), continue: next }
                    : null;
                r.onsuccess?.();
                if (!r.result && !tx.aborted) tx.oncomplete?.();
              });
            next();
            return r;
          },
        }),
      };
      return tx;
    },
  };
  vi.stubGlobal("indexedDB", {
    open: () => {
      const r = { result: db, onsuccess: null as null | (() => void) };
      queueMicrotask(() => r.onsuccess?.());
      return r;
    },
  });
  return {
    rows,
    refuseWrites: () => {
      refuseWrites = true;
    },
  };
}
describe("durable immutable-source attachment copies", () => {
  beforeEach(() => {
    mocks.fetch.mockReset();
    mocks.origin.accountId = "original";
    vi.unstubAllGlobals();
  });
  afterEach(() => vi.unstubAllGlobals());
  it("captures exact original metadata and target before storage awaits, with no file-byte request", async () => {
    const { rows } = storage(),
      mutableTarget = { ...target },
      mutableSelection = { ...selected };
    const preparing = prepareAttachmentCopy(mutableSelection, mutableTarget);
    mutableTarget.draftId = draftB;
    mutableSelection.rawVersion = "{}";
    const id = await preparing;
    expect(rows.get(id)).toEqual(
      expect.objectContaining({
        target,
        source: {
          kind: "version_v1",
          documentId: "source",
          versionId: sourceId,
          rawSource: rawVersion,
        },
        state: "prepared",
      }),
    );
    mocks.fetch.mockResolvedValue(result());
    expect(await runAttachmentCopy(id)).toEqual(result());
    const [context, path, options] = mocks.fetch.mock.calls[0];
    expect(context.origin.accountId).toBe("original");
    expect(path).toBe(`/spaces/space/documents/target/drafts/${draftA}/attachments/copy`);
    expect(options).toEqual({
      method: "POST",
      body: {
        schema_version: 1,
        operation_id: id,
        source_document_id: "source",
        version_id: sourceId,
        expected_source_version: JSON.parse(rawVersion),
      },
    });
    expect(options.body).not.toBeInstanceOf(Blob);
    expect(rows.get(id)?.source.rawSource).toBe(rawVersion);
  });
  it("holds wrong audience ACK and retries the same original operation, then reloads its stored ACK without HTTP", async () => {
    const { rows } = storage(),
      id = await prepareAttachmentCopy(selected, target);
    mocks.fetch.mockResolvedValueOnce(result(draftB));
    await expect(runAttachmentCopy(id)).rejects.toThrow("original draft audience");
    expect(rows.get(id)?.state).toBe("unknown");
    mocks.fetch.mockResolvedValue(result());
    await runAttachmentCopy(id);
    await runAttachmentCopy(id);
    expect(mocks.fetch).toHaveBeenCalledTimes(2);
    expect(mocks.fetch.mock.calls[0][2]).toEqual(mocks.fetch.mock.calls[1][2]);
    expect(rows.get(id)?.state).toBe("acknowledged");
  });
  it("discovers only original target/account operations, preserves acknowledged work until explicit adoption", async () => {
    storage();
    const id = await prepareAttachmentCopy(selected, target);
    mocks.fetch.mockResolvedValue(result());
    await runAttachmentCopy(id);
    expect((await retainedAttachmentCopies(target)).map((x) => x.id)).toEqual([id]);
    expect(await retainedAttachmentCopies({ ...target, draftId: draftB })).toEqual([]);
    await expect(markAttachmentCopyAdopted(id, { ...target, draftId: draftB })).rejects.toThrow(
      "cannot be adopted",
    );
    mocks.origin.accountId = "other";
    expect(await retainedAttachmentCopies(target)).toEqual([]);
    await expect(runAttachmentCopy(id)).rejects.toThrow("account or session changed");
    mocks.origin.accountId = "original";
    await markAttachmentCopyAdopted(id, target);
    expect(await retainedAttachmentCopies(target)).toEqual([]);
    expect(await runAttachmentCopy(id)).toEqual(result());
  });
  it("does not dispatch when preparation or predispatch persistence fails", async () => {
    const store = storage();
    store.refuseWrites();
    await expect(prepareAttachmentCopy(selected, target)).rejects.toThrow("persistence failed");
    expect(mocks.fetch).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
    const second = storage(),
      id = await prepareAttachmentCopy(selected, target);
    second.refuseWrites();
    await expect(runAttachmentCopy(id)).rejects.toThrow("persistence failed");
    expect(mocks.fetch).not.toHaveBeenCalled();
  });
  it("rejects tampered durable source/request and same-version or wrong-length ACKs without adoption", async () => {
    const { rows } = storage(),
      id = await prepareAttachmentCopy(selected, target);
    const op = rows.get(id)!;
    expect(() => validateAttachmentCopyOperation({ ...op, requestJSON: "{}" })).toThrow(
      "source proof differs",
    );
    rows.set(id, { ...op, requestJSON: "{}" });
    await expect(runAttachmentCopy(id)).rejects.toThrow("source proof differs");
    expect(mocks.fetch).not.toHaveBeenCalled();
    rows.set(id, op);
    mocks.fetch.mockResolvedValue({ ...result(), id: sourceId });
    await expect(runAttachmentCopy(id)).rejects.toThrow("acknowledgement differs");
    mocks.fetch.mockResolvedValue({ ...result(), size_bytes: 4 });
    await expect(runAttachmentCopy(id)).rejects.toThrow("acknowledgement differs");
    expect(rows.get(id)?.state).toBe("unknown");
  });
  it("retains the exact server legacy proof with an opaque asset ID, then retries metadata-only to the original draft", async () => {
    const { rows } = storage();
    const assetId = "legacy report 2019.pdf";
    const raw = JSON.stringify({
      kind: "legacy_snapshot_v1",
      asset_id: assetId,
      space_id: "space",
      document_id: "source",
      original_name: "exact.txt",
      mime_type: "text/plain",
      size_bytes: 5,
      created_at: "2020-01-01T00:00:00Z",
      storage_path: "spaces/space/legacy",
      binary_sha256: "b".repeat(64),
      unknown: { retained: true },
    });
    const network = vi.fn().mockResolvedValue(new Response(raw));
    vi.stubGlobal("fetch", network);
    const selection = await fetchLegacyAttachmentCopySource("space", "source", assetId);
    expect(selection.rawLegacySource).toBe(raw);
    expect(network.mock.calls[0][0]).toContain("legacy%20report%202019.pdf");
    const id = await prepareLegacyAttachmentCopy(selection, target);
    mocks.fetch.mockRejectedValueOnce(new Error("Reply lost"));
    await expect(runAttachmentCopy(id)).rejects.toThrow("Reply lost");
    expect(rows.get(id)?.source).toEqual({
      kind: "legacy_snapshot_v1",
      documentId: "source",
      versionId: assetId,
      rawSource: raw,
    });
    mocks.fetch.mockResolvedValue(result());
    await runAttachmentCopy(id);
    expect(mocks.fetch.mock.calls[1][2].body).toEqual({
      schema_version: 1,
      operation_id: id,
      source_document_id: "source",
      version_id: assetId,
      expected_legacy_source: JSON.parse(raw),
    });
    expect(mocks.fetch.mock.calls[0][2]).toEqual(mocks.fetch.mock.calls[1][2]);
    expect(network).toHaveBeenCalledTimes(1);
  });
  it("does not manufacture a legacy proof for a registered/conflicting or wrong-association review response", async () => {
    storage();
    const network = vi.fn().mockResolvedValueOnce(new Response("registered", { status: 409 }));
    vi.stubGlobal("fetch", network);
    await expect(fetchLegacyAttachmentCopySource("space", "source", sourceId)).rejects.toThrow(
      "proof unavailable",
    );
    network.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          kind: "legacy_snapshot_v1",
          asset_id: sourceId,
          space_id: "foreign",
          document_id: "source",
        }),
      ),
    );
    await expect(fetchLegacyAttachmentCopySource("space", "source", sourceId)).rejects.toThrow(
      "association differs",
    );
    expect(mocks.fetch).not.toHaveBeenCalled();
  });
  it("persists adopted original copy when successful guarded use unmounts its own review", async () => {
    const { rows } = storage(),
      id = await prepareAttachmentCopy(selected, target);
    mocks.fetch.mockResolvedValue(result());
    await runAttachmentCopy(id);
    let unmounted = false;
    const verify = vi.fn(() => {
      if (unmounted) throw new Error("Review unmounted");
    });
    const onUse = vi.fn(async (attachment) => {
      expect(attachment.id).toBe(copiedId);
      unmounted = true;
    });
    await adoptReviewedAttachmentCopy(id, target, {
      verifySelection: verify,
      isReferenced: () => false,
      onUse,
    });
    expect(onUse).toHaveBeenCalledOnce();
    expect(rows.get(id)?.state).toBe("adopted");
    expect(await retainedAttachmentCopies(target)).toEqual([]);
    expect(mocks.fetch).toHaveBeenCalledTimes(1);
  });
  it("retains the original acknowledged copy when guarded use rejects a changed draft, and never dispatches under a mismatched target", async () => {
    const { rows } = storage(),
      id = await prepareAttachmentCopy(selected, target);
    mocks.fetch.mockResolvedValue(result());
    await runAttachmentCopy(id);
    await expect(
      adoptReviewedAttachmentCopy(id, target, {
        verifySelection: () => undefined,
        isReferenced: () => false,
        onUse: async () => {
          throw new Error("Original draft changed after insertion await");
        },
      }),
    ).rejects.toThrow("Original draft changed");
    expect(rows.get(id)?.state).toBe("acknowledged");
    expect((await retainedAttachmentCopies(target)).map((x) => x.id)).toEqual([id]);
    await expect(
      adoptReviewedAttachmentCopy(
        id,
        { ...target, draftId: draftB },
        {
          verifySelection: () => undefined,
          isReferenced: () => false,
          onUse: async () => undefined,
        },
      ),
    ).rejects.toThrow("another draft");
    expect(mocks.fetch).toHaveBeenCalledTimes(1);
  });
  it("prevents concurrent dispatch of one durable operation while retaining the pending original", async () => {
    const { rows } = storage(),
      id = await prepareAttachmentCopy(selected, target);
    let release!: (v: ReturnType<typeof result>) => void;
    const called = new Promise<void>((resolve) =>
      mocks.fetch.mockImplementation(() => {
        resolve();
        return new Promise((r) => {
          release = r;
        });
      }),
    );
    const first = runAttachmentCopy(id);
    await called;
    await expect(runAttachmentCopy(id)).rejects.toThrow("already in progress");
    expect(rows.get(id)?.state).toBe("unknown");
    release(result());
    await first;
    expect(mocks.fetch).toHaveBeenCalledTimes(1);
  });
});
