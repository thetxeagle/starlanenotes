import { describe, expect, it, vi } from "vitest";
import {
  clearDocumentDraftRecovery,
  documentDraftRecoveryMatchesSaved,
  compareDocumentDraftRecoverySource,
  type DocumentDraftRecoverySource,
  discardDocumentDraftRecovery,
  persistDocumentDraftRecovery,
  readDocumentDraftRecovery,
  type DocumentDraftRecoveryContent,
  type DocumentDraftRecoveryScope,
  type StorageLike,
} from "./documentDraftRecovery";

function createStorage(): StorageLike {
  const values = new Map<string, string>();
  return {
    getItem: vi.fn((key) => values.get(key) ?? null),
    setItem: vi.fn((key, value) => values.set(key, value)),
    removeItem: vi.fn((key) => values.delete(key)),
  };
}

const scope: DocumentDraftRecoveryScope = {
  storageMode: "server",
  authIdentity: "user-1",
  serverTarget: "https://notes.example.test/api/v1",
  spaceId: "space-1",
  documentId: "page-1",
  draftId: "draft-1",
};

const content: DocumentDraftRecoveryContent = {
  title: "Launch plan",
  content: "# Updated launch plan\n\nDetails",
  synopsis: "A plan for launch.",
  pageWidth: "wide",
  customCss: ".notice { font-weight: 600; }",
};

describe("documentDraftRecovery", () => {
  it("persists and reads the full pending draft with its baseline timestamp", () => {
    const storage = createStorage();
    const snapshotId = persistDocumentDraftRecovery(
      scope,
      content,
      "2026-10-05T20:00:00Z",
      storage,
      "2026-10-05T20:01:00Z",
    );

    expect(snapshotId).toBeTruthy();
    expect(readDocumentDraftRecovery(scope, storage)).toEqual({
      ...content,
      snapshotId,
      capturedAt: "2026-10-05T20:01:00Z",
      baseDraftUpdatedAt: "2026-10-05T20:00:00Z",
    });
  });

  it("isolates checkpoints by user, server, space, page, and draft", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(
      scope,
      content,
      "2026-10-05T20:00:00Z",
      storage,
      "2026-10-05T20:01:00Z",
    );

    expect(readDocumentDraftRecovery({ ...scope, authIdentity: "user-2" }, storage)).toBeNull();
    expect(readDocumentDraftRecovery({ ...scope, storageMode: "local" }, storage)).toBeNull();
    expect(
      readDocumentDraftRecovery(
        { ...scope, serverTarget: "https://other.example.test/api/v1" },
        storage,
      ),
    ).toBeNull();
    expect(readDocumentDraftRecovery({ ...scope, draftId: "draft-2" }, storage)).toBeNull();
    expect(readDocumentDraftRecovery({ ...scope, spaceId: "space-2" }, storage)).toBeNull();
    expect(readDocumentDraftRecovery({ ...scope, documentId: "page-2" }, storage)).toBeNull();
  });

  it("does not clear a newer checkpoint when an older save finishes", () => {
    const storage = createStorage();
    const oldSnapshotId = persistDocumentDraftRecovery(
      scope,
      content,
      "2026-10-05T20:00:00Z",
      storage,
      "2026-10-05T20:01:00Z",
    );
    const newerContent = { ...content, content: "Newer edit" };
    const newSnapshotId = persistDocumentDraftRecovery(
      scope,
      newerContent,
      "2026-10-05T20:00:00Z",
      storage,
      "2026-10-05T20:02:00Z",
    );

    expect(clearDocumentDraftRecovery(scope, oldSnapshotId!, "confirmed", storage)).toBe(false);
    expect(readDocumentDraftRecovery(scope, storage)?.snapshotId).toBe(newSnapshotId);
    expect(readDocumentDraftRecovery(scope, storage)?.content).toBe("Newer edit");
  });

  it.each(["confirmed", "queued"] as const)(
    "clears the exact checkpoint after a %s save",
    (outcome) => {
      const storage = createStorage();
      const snapshotId = persistDocumentDraftRecovery(
        scope,
        content,
        "2026-10-05T20:00:00Z",
        storage,
      );

      expect(clearDocumentDraftRecovery(scope, snapshotId!, outcome, storage)).toBe(true);
      expect(readDocumentDraftRecovery(scope, storage)).toBeNull();
    },
  );

  it("leaves the checkpoint intact for another draft or wrong snapshot identity", () => {
    const storage = createStorage();
    const snapshotId = persistDocumentDraftRecovery(scope, content, "base", storage);

    expect(
      clearDocumentDraftRecovery({ ...scope, draftId: "other" }, snapshotId!, "confirmed", storage),
    ).toBe(false);
    expect(clearDocumentDraftRecovery(scope, "stale-snapshot", "queued", storage)).toBe(false);
    expect(readDocumentDraftRecovery(scope, storage)?.snapshotId).toBe(snapshotId);
  });

  it("discards only the exact checkpoint after an explicit user choice", () => {
    const storage = createStorage();
    const oldSnapshotId = persistDocumentDraftRecovery(scope, content, "base", storage);
    const currentSnapshotId = persistDocumentDraftRecovery(
      scope,
      { ...content, content: "Current edit" },
      "base",
      storage,
    );

    expect(discardDocumentDraftRecovery(scope, oldSnapshotId!, storage)).toBe(false);
    expect(readDocumentDraftRecovery(scope, storage)?.snapshotId).toBe(currentSnapshotId);
    expect(discardDocumentDraftRecovery(scope, currentSnapshotId!, storage)).toBe(true);
    expect(readDocumentDraftRecovery(scope, storage)).toBeNull();
  });

  it("returns safe failure values when storage is unavailable or throws", () => {
    const blockedStorage: StorageLike = {
      getItem: () => {
        throw new Error("storage disabled");
      },
      setItem: () => {
        throw new Error("storage full");
      },
      removeItem: () => {
        throw new Error("storage disabled");
      },
    };

    expect(persistDocumentDraftRecovery(scope, content, "base", blockedStorage)).toBeNull();
    expect(readDocumentDraftRecovery(scope, blockedStorage)).toBeNull();
    expect(clearDocumentDraftRecovery(scope, "snapshot", "confirmed", blockedStorage)).toBe(false);
  });

  it("rejects oversized content instead of creating an unsafe recovery record", () => {
    const storage = createStorage();
    const snapshotId = persistDocumentDraftRecovery(
      scope,
      { ...content, content: "x".repeat(2_000_001) },
      "base",
      storage,
    );

    expect(snapshotId).toBeNull();
    expect(storage.setItem).not.toHaveBeenCalled();
  });

  it("rejects a corrupt checkpoint without exposing it as a recovery candidate", () => {
    const storage = createStorage();
    storage.setItem(
      "starlane_document_draft_recovery_v1:server:https%3A%2F%2Fnotes.example.test%2Fapi%2Fv1:user-1:space-1:page-1:draft-1",
      JSON.stringify({ scope, snapshot: { ...content, pageWidth: "huge" } }),
    );

    expect(readDocumentDraftRecovery(scope, storage)).toBeNull();
  });

  it("ignores malformed records without throwing", () => {
    const storage = createStorage();
    storage.setItem(
      "starlane_document_draft_recovery_v1:server:https%3A%2F%2Fnotes.example.test%2Fapi%2Fv1:user-1:space-1:page-1:draft-1",
      "not-json",
    );

    expect(readDocumentDraftRecovery(scope, storage)).toBeNull();
  });
  const originalSource: DocumentDraftRecoverySource = {
    draftId: scope.draftId,
    sourceDigest: "a".repeat(64),
    title: "Original title",
    content: "  Raw original body\n\n[[Exact|id]]  ",
    icon: "🦅",
    coverImage: "https://example.test/cover.png",
  };

  it("persists immutable exact original bytes separately from edited checkpoint content", () => {
    const storage = createStorage();
    const source = { ...originalSource };
    persistDocumentDraftRecovery(scope, content, "same timestamp", storage, "captured", source);
    source.title = "Caller changed title";
    source.content = "Caller changed bytes";
    source.sourceDigest = "b".repeat(64);
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(checkpoint.baseDraftSource).toEqual(originalSource);
    expect(checkpoint.content).toBe(content.content);
    expect(
      compareDocumentDraftRecoverySource(checkpoint, {
        id: scope.draftId,
        source_digest: "a".repeat(64),
      }),
    ).toBe("matching");
  });

  it("detects source drift even when server timestamps are identical", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(
      scope,
      content,
      "2026-10-09T12:00:00Z",
      storage,
      "captured",
      originalSource,
    );
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    const sameTimestampNewDraft = {
      id: scope.draftId,
      source_digest: "b".repeat(64),
      updated_at: checkpoint.baseDraftUpdatedAt,
    };
    expect(compareDocumentDraftRecoverySource(checkpoint, sameTimestampNewDraft)).toBe("changed");
  });

  it("keeps legacy records readable without claiming timestamp-based source equality", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(scope, content, "same timestamp", storage);
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(checkpoint.baseDraftSource).toBeUndefined();
    expect(
      compareDocumentDraftRecoverySource(checkpoint, {
        id: scope.draftId,
        source_digest: "a".repeat(64),
      }),
    ).toBe("legacy-unverified");
  });

  it("cannot claim matching provenance for another selected draft or missing digest", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(scope, content, "base", storage, "captured", originalSource);
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(
      compareDocumentDraftRecoverySource(checkpoint, {
        id: "another-draft",
        source_digest: originalSource.sourceDigest,
      }),
    ).toBe("changed");
    expect(compareDocumentDraftRecoverySource(checkpoint, { id: scope.draftId })).toBe(
      "legacy-unverified",
    );
    discardDocumentDraftRecovery(scope, checkpoint.snapshotId, storage);
    const withoutDigest = { ...originalSource, sourceDigest: undefined };
    persistDocumentDraftRecovery(scope, content, "base", storage, "captured", withoutDigest);
    expect(
      compareDocumentDraftRecoverySource(readDocumentDraftRecovery(scope, storage)!, {
        id: scope.draftId,
        source_digest: originalSource.sourceDigest,
      }),
    ).toBe("legacy-unverified");
  });

  it("preserves the current provenance checkpoint when an older exact-ID save finishes", () => {
    const storage = createStorage();
    const old = persistDocumentDraftRecovery(
      scope,
      content,
      "same timestamp",
      storage,
      "same capture",
      originalSource,
    )!;
    const newerSource = {
      ...originalSource,
      sourceDigest: "b".repeat(64),
      content: "New baseline",
    };
    const current = persistDocumentDraftRecovery(
      scope,
      { ...content, content: "Newest device edit" },
      "same timestamp",
      storage,
      "same capture",
      newerSource,
    )!;
    expect(clearDocumentDraftRecovery(scope, old, "confirmed", storage)).toBe(false);
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(checkpoint.snapshotId).toBe(current);
    expect(checkpoint.baseDraftSource).toEqual(originalSource);
    expect(
      compareDocumentDraftRecoverySource(checkpoint, {
        id: scope.draftId,
        source_digest: newerSource.sourceDigest,
      }),
    ).toBe("changed");
    expect(checkpoint.content).toBe("Newest device edit");
    expect(clearDocumentDraftRecovery(scope, current, "confirmed", storage)).toBe(true);
    persistDocumentDraftRecovery(scope, content, "new base", storage, "new capture", newerSource);
    expect(readDocumentDraftRecovery(scope, storage)?.baseDraftSource).toEqual(newerSource);
  });

  it("rejects malformed, oversized, or foreign source provenance without replacing current bytes", () => {
    const storage = createStorage();
    const current = persistDocumentDraftRecovery(
      scope,
      content,
      "base",
      storage,
      "captured",
      originalSource,
    )!;
    for (const source of [
      { ...originalSource, draftId: "foreign" },
      { ...originalSource, sourceDigest: "invalid digest" },
      { ...originalSource, content: "x".repeat(2_000_001) },
    ]) {
      expect(
        persistDocumentDraftRecovery(scope, content, "base", storage, "captured", source),
      ).toBeNull();
      expect(readDocumentDraftRecovery(scope, storage)?.snapshotId).toBe(current);
    }
  });
  it("preserves pending icon/cover choices, including explicit removal, and legacy absence", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(
      scope,
      { ...content, icon: "Pending icon", coverImage: "Pending cover" },
      "base",
      storage,
      "captured",
      originalSource,
    );
    let checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(checkpoint.icon).toBe("Pending icon");
    expect(checkpoint.coverImage).toBe("Pending cover");
    expect(checkpoint.baseDraftSource?.icon).toBe(originalSource.icon);
    persistDocumentDraftRecovery(
      scope,
      { ...content, icon: null, coverImage: null },
      "new timestamp",
      storage,
      "captured",
      { ...originalSource, sourceDigest: "b".repeat(64) },
    );
    checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(checkpoint.icon).toBeNull();
    expect(checkpoint.coverImage).toBeNull();
    expect(checkpoint.baseDraftSource).toEqual(originalSource);
    expect(checkpoint.baseDraftUpdatedAt).toBe("base");
    discardDocumentDraftRecovery(scope, checkpoint.snapshotId, storage);
    persistDocumentDraftRecovery(scope, content, "base", storage);
    checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(checkpoint).not.toHaveProperty("icon");
    expect(checkpoint).not.toHaveProperty("coverImage");
  });

  it("does not retroactively assign a new source digest to outstanding legacy edits", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(scope, content, "old base", storage);
    persistDocumentDraftRecovery(
      scope,
      { ...content, content: "Additional edit" },
      "new base",
      storage,
      "new capture",
      originalSource,
    );
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(checkpoint.baseDraftSource).toBeUndefined();
    expect(checkpoint.baseDraftUpdatedAt).toBe("old base");
    expect(
      compareDocumentDraftRecoverySource(checkpoint, {
        id: scope.draftId,
        source_digest: originalSource.sourceDigest,
      }),
    ).toBe("legacy-unverified");
  });

  it("validates presentation choices and their storage budget without replacing good edits", () => {
    const storage = createStorage();
    const current = persistDocumentDraftRecovery(scope, content, "base", storage)!;
    for (const invalid of [
      { ...content, icon: 123 },
      { ...content, coverImage: "x".repeat(2_000_001) },
    ]) {
      expect(
        persistDocumentDraftRecovery(
          scope,
          invalid as DocumentDraftRecoveryContent,
          "base",
          storage,
        ),
      ).toBeNull();
      expect(readDocumentDraftRecovery(scope, storage)?.snapshotId).toBe(current);
    }
  });
  it("does not clear matching title/body while checkpoint presentation is unconfirmed", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(
      scope,
      { ...content, icon: "New icon", coverImage: "New cover" },
      "base",
      storage,
    );
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    const prepared = { title: "Launch plan", content: "Canonical serialized body" };
    const saved = { ...prepared, icon: "Old icon", cover_image: "Old cover" };
    expect(documentDraftRecoveryMatchesSaved(checkpoint, saved, prepared)).toBe(false);
    expect(
      documentDraftRecoveryMatchesSaved(checkpoint, { ...saved, icon: "New icon" }, prepared),
    ).toBe(false);
    expect(
      documentDraftRecoveryMatchesSaved(
        checkpoint,
        { ...saved, cover_image: "New cover" },
        prepared,
      ),
    ).toBe(false);
    expect(
      documentDraftRecoveryMatchesSaved(
        checkpoint,
        { ...saved, icon: "New icon", cover_image: "New cover" },
        prepared,
      ),
    ).toBe(true);
  });

  it("confirms explicit icon/cover removal only when the saved draft confirms removal", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(
      scope,
      { ...content, icon: null, coverImage: "  " },
      "base",
      storage,
    );
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    const prepared = { title: content.title, content: "Serialized checkpoint" };
    expect(
      documentDraftRecoveryMatchesSaved(
        checkpoint,
        { ...prepared, icon: "Existing", cover_image: null },
        prepared,
      ),
    ).toBe(false);
    expect(
      documentDraftRecoveryMatchesSaved(
        checkpoint,
        { ...prepared, icon: null, cover_image: "Existing" },
        prepared,
      ),
    ).toBe(false);
    expect(
      documentDraftRecoveryMatchesSaved(
        checkpoint,
        { ...prepared, icon: null, cover_image: null },
        prepared,
      ),
    ).toBe(true);
    expect(documentDraftRecoveryMatchesSaved(checkpoint, prepared, prepared)).toBe(true);
  });

  it("does not claim pending nonempty presentation bytes were normalized away", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(
      scope,
      { ...content, icon: " icon ", coverImage: " cover " },
      "base",
      storage,
    );
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    const prepared = { title: content.title, content: "Serialized checkpoint" };
    expect(
      documentDraftRecoveryMatchesSaved(
        checkpoint,
        { ...prepared, icon: "icon", cover_image: "cover" },
        prepared,
      ),
    ).toBe(false);
    expect(
      documentDraftRecoveryMatchesSaved(
        checkpoint,
        { ...prepared, icon: " icon ", cover_image: " cover " },
        prepared,
      ),
    ).toBe(true);
  });

  it("allows absent legacy presentation fields but still requires exact prepared title/body", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(scope, content, "base", storage);
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    const prepared = { title: "Normalized checkpoint title", content: "Canonical serialized body" };
    const saved = { ...prepared, icon: "Current icon", cover_image: "Current cover" };
    expect(documentDraftRecoveryMatchesSaved(checkpoint, saved, prepared)).toBe(true);
    expect(
      documentDraftRecoveryMatchesSaved(checkpoint, { ...saved, title: "Other" }, prepared),
    ).toBe(false);
    expect(
      documentDraftRecoveryMatchesSaved(checkpoint, { ...saved, content: "Other" }, prepared),
    ).toBe(false);
  });
  const rawCssBlock = '<!-- STARLANE:custom-css -->\r\n \0@import "https://example.invalid/original.css";\r\n.notice { color: blue; }\r\n<!-- /starlane:custom-css -->';

  it("round-trips exact original CSS spans separately from edited content and original draft evidence", () => {
    const storage = createStorage();
    const cssProvenance = { schemaVersion: 1, kind: "single", rawBlock: rawCssBlock } as const;
    persistDocumentDraftRecovery(scope, { ...content, cssProvenance }, "base", storage, "captured", originalSource);
    const first = readDocumentDraftRecovery(scope, storage)!;
    expect(first.cssProvenance).toEqual(cssProvenance);
    expect(first.baseDraftSource).toEqual(originalSource);
    persistDocumentDraftRecovery(scope, { ...content, content: "Edited Markdown", cssProvenance }, "later", storage, "later", { ...originalSource, content: "Changed source", sourceDigest: "b".repeat(64) });
    const latest = readDocumentDraftRecovery(scope, storage)!;
    expect(latest.cssProvenance).toEqual(cssProvenance);
    expect(latest.content).toBe("Edited Markdown");
    expect(latest.baseDraftUpdatedAt).toBe("base");
    expect(latest.baseDraftSource).toEqual(originalSource);
    expect(clearDocumentDraftRecovery(scope, first.snapshotId, "confirmed", storage)).toBe(false);
    expect(readDocumentDraftRecovery(scope, storage)?.snapshotId).toBe(latest.snapshotId);
  });

  it("distinguishes an empty retained block, explicit absence and a legacy missing field", () => {
    for (const cssProvenance of [
      { schemaVersion: 1, kind: "single", rawBlock: "<!-- starlane:custom-css --><!-- /starlane:custom-css -->" },
      { schemaVersion: 1, kind: "absent" },
      undefined,
    ] as const) {
      const storage = createStorage();
      expect(persistDocumentDraftRecovery(scope, { ...content, ...(cssProvenance === undefined ? {} : { cssProvenance }) }, "base", storage)).not.toBeNull();
      expect(readDocumentDraftRecovery(scope, storage)?.cssProvenance).toEqual(cssProvenance);
      if (cssProvenance === undefined) expect(readDocumentDraftRecovery(scope, storage)).not.toHaveProperty("cssProvenance");
    }
  });

  it("retains ambiguous whole original source and edited body without deriving a replacement", () => {
    const storage = createStorage();
    const original = "Original header\r\n<!-- starlane:custom-css -->\0unclosed CSS\r\nOriginal body";
    const cssProvenance = { schemaVersion: 1, kind: "ambiguous", originalSource: original } as const;
    const local = { ...content, content: "Separate edited body", customCss: "", cssProvenance };
    expect(persistDocumentDraftRecovery(scope, local, "base", storage)).not.toBeNull();
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(checkpoint.content).toBe("Separate edited body");
    expect(checkpoint.customCss).toBe("");
    expect(checkpoint.cssProvenance).toEqual(cssProvenance);
    expect(checkpoint.baseDraftSource).toBeUndefined();
    expect(compareDocumentDraftRecoverySource(checkpoint, { id: scope.draftId, source_digest: "a".repeat(64) })).toBe("legacy-unverified");
  });

  it("does not drop retained CSS when a legacy caller writes a newer checkpoint without the optional field", () => {
    const storage = createStorage();
    const cssProvenance = { schemaVersion: 1, kind: "single", rawBlock: rawCssBlock } as const;
    persistDocumentDraftRecovery(scope, { ...content, cssProvenance }, "base", storage);
    expect(persistDocumentDraftRecovery(scope, { ...content, title: "New title" }, "later", storage)).not.toBeNull();
    expect(readDocumentDraftRecovery(scope, storage)?.cssProvenance).toEqual(cssProvenance);
    expect(readDocumentDraftRecovery(scope, storage)?.title).toBe("New title");
  });

  it("refuses malformed or unknown provenance before replacing the previous checkpoint", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(scope, content, "base", storage);
    const key = vi.mocked(storage.setItem).mock.calls[0][0];
    const original = storage.getItem(key);
    const invalid = [null, false, [], { schemaVersion: 2, kind: "absent" }, { schemaVersion: 1, kind: "future" }, { schemaVersion: 1, kind: "single", rawBlock: 123 }, { schemaVersion: 1, kind: "single", rawBlock: "invented marker-free body" }, { schemaVersion: 1, kind: "absent", rawBlock: rawCssBlock }, { schemaVersion: 1, kind: "ambiguous", originalSource: "ordinary marker-free source" }, { schemaVersion: 1, kind: "ambiguous", originalSource: null }];
    for (const cssProvenance of invalid) {
      expect(persistDocumentDraftRecovery(scope, { ...content, cssProvenance } as unknown as DocumentDraftRecoveryContent, "later", storage)).toBeNull();
      expect(storage.getItem(key)).toBe(original);
    }
  });

  it("keeps corrupt original bytes during reads, clears, discards and subsequent checkpoint writes", () => {
    const storage = createStorage();
    const id = persistDocumentDraftRecovery(scope, content, "base", storage)!;
    const key = vi.mocked(storage.setItem).mock.calls[0][0];
    const valid = JSON.parse(storage.getItem(key)!);
    valid.snapshot.cssProvenance = { schemaVersion: 99, kind: "single", rawBlock: rawCssBlock };
    const corrupt = JSON.stringify(valid);
    storage.setItem(key, corrupt);
    expect(readDocumentDraftRecovery(scope, storage)).toBeNull();
    expect(clearDocumentDraftRecovery(scope, id, "confirmed", storage)).toBe(false);
    expect(discardDocumentDraftRecovery(scope, id, storage)).toBe(false);
    expect(persistDocumentDraftRecovery(scope, content, "later", storage)).toBeNull();
    expect(storage.getItem(key)).toBe(corrupt);
    expect(storage.removeItem).not.toHaveBeenCalled();
    expect(documentDraftRecoveryMatchesSaved(valid.snapshot, { title: content.title, content: content.content }, { title: content.title, content: content.content })).toBe(false);
  });

  it("counts raw provenance against the combined checkpoint budget and preserves a prior record on refusal", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(scope, content, "base", storage);
    const key = vi.mocked(storage.setItem).mock.calls[0][0];
    const original = storage.getItem(key);
    const cssProvenance = { schemaVersion: 1, kind: "single", rawBlock: `<!-- starlane:custom-css -->${"x".repeat(1_000_000)}<!-- /starlane:custom-css -->` } as const;
    expect(persistDocumentDraftRecovery(scope, { ...content, content: "y".repeat(1_000_000), cssProvenance }, "later", storage)).toBeNull();
    expect(storage.getItem(key)).toBe(original);
    const ambiguous = { schemaVersion: 1, kind: "ambiguous", originalSource: `<!-- starlane:custom-css -->${"x".repeat(2_000_000)}` } as const;
    expect(persistDocumentDraftRecovery(scope, { ...content, cssProvenance: ambiguous }, "later", storage)).toBeNull();
    expect(storage.getItem(key)).toBe(original);
  });

  it("counts inherited provenance before writing an otherwise bounded newer body", () => {
    const storage = createStorage();
    const cssProvenance = { schemaVersion: 1, kind: "single", rawBlock: `<!-- starlane:custom-css -->${"x".repeat(1_000_000)}<!-- /starlane:custom-css -->` } as const;
    expect(persistDocumentDraftRecovery(scope, { ...content, cssProvenance }, "base", storage)).not.toBeNull();
    const key = vi.mocked(storage.setItem).mock.calls[0][0];
    const original = storage.getItem(key);
    expect(persistDocumentDraftRecovery(scope, { ...content, content: "y".repeat(1_000_000) }, "later", storage)).toBeNull();
    expect(storage.getItem(key)).toBe(original);
  });

  it("leaves the original CSS checkpoint intact when storage rejects its replacement", () => {
    const storage = createStorage();
    const cssProvenance = { schemaVersion: 1, kind: "single", rawBlock: rawCssBlock } as const;
    persistDocumentDraftRecovery(scope, { ...content, cssProvenance }, "base", storage);
    const key = vi.mocked(storage.setItem).mock.calls[0][0];
    const original = storage.getItem(key);
    vi.mocked(storage.setItem).mockImplementationOnce(() => { throw new Error("Quota exceeded"); });
    expect(persistDocumentDraftRecovery(scope, { ...content, title: "New title", cssProvenance }, "later", storage)).toBeNull();
    expect(storage.getItem(key)).toBe(original);
    expect(readDocumentDraftRecovery(scope, storage)?.cssProvenance).toEqual(cssProvenance);
  });

  it("does not fabricate CSS provenance from sanitized CSS or original draft text for legacy records", () => {
    const storage = createStorage();
    persistDocumentDraftRecovery(scope, content, "base", storage, "captured", { ...originalSource, content: `Original body\n${rawCssBlock}` });
    const checkpoint = readDocumentDraftRecovery(scope, storage)!;
    expect(checkpoint.cssProvenance).toBeUndefined();
    expect(checkpoint.baseDraftSource?.content).toBe(`Original body\n${rawCssBlock}`);
    expect(checkpoint.customCss).toBe(content.customCss);
  });

});
