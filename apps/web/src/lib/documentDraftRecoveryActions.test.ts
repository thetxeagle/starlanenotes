import { describe, expect, it, vi } from "vitest";
import { DraftSaveHeldError, type DraftRecord } from "./dataClient";
import { DraftBirthHeldError } from "./draftBirth";
import {
  createDocumentDraftRecoveryActions,
  type DraftRecoveryIntent,
  type DraftRecoveryTransport,
} from "./documentDraftRecoveryActions";

function fixture() {
  const values = new Map<string, string>();
  const original: DraftRecord = {
    id: "original",
    space_id: "space",
    document_id: "page",
    owner_id: "me",
    owner_name: "Me",
    document_title: "Page",
    title: "Current",
    content: "CURRENT",
    access_role: "owner",
    can_publish: true,
    source_digest: "a".repeat(64),
    base_updated_at: "base",
    created_at: "birth",
    updated_at: "same-time",
  };
  const target = { ...original, id: "copy", source_digest: "b".repeat(64) };
  const intent: DraftRecoveryIntent = {
    scope: {
      storageMode: "server",
      authIdentity: "me",
      serverTarget: "/api/v1",
      spaceId: "space",
      documentId: "page",
      draftId: "original",
    },
    checkpoint: {
      snapshotId: "snapshot",
      capturedAt: "capture",
      baseDraftUpdatedAt: "same-time",
      title: "Recovered",
      content: "RECOVERED",
      synopsis: "",
      pageWidth: "standard",
      customCss: "",
    },
    reviewedDraft: original,
    published: true,
    selection: {
      mode: "server",
      accountId: "me",
      serverBase: "/api/v1",
      storageSelection: "selection",
      origin: null,
    },
    patch: { title: " Recovered ", content: "SERIALIZED RECOVERED", icon: "" },
  };
  const transport: DraftRecoveryTransport = {
    storage: {
      get length() {
        return values.size;
      },
      key: (index: number) => [...values.keys()][index] ?? null,
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => {
        values.set(key, value);
      },
      removeItem: (key) => {
        values.delete(key);
      },
    },
    runExclusive: async (_key, run) => run(),
    verifySelection: vi.fn(),
    createDraft: vi.fn(async (_space, _page, _options, onPrepared) => {
      onPrepared("associated-op");
      return target;
    }),
    retryBirth: vi.fn(async () => target),
    saveDraft: vi.fn(async (_space, _page, id, patch) => ({
      ...target,
      id,
      title: patch.title.trim(),
      content: patch.content,
      icon: patch.icon || undefined,
      source_digest: "c".repeat(64),
    })),
  };
  return {
    values,
    original,
    target,
    intent,
    transport,
    actions: createDocumentDraftRecoveryActions(transport),
  };
}

describe("explicit rejected recovery review", () => {
  it("restarts only proven rejected birth after refreshed review and archives original evidence", async () => {
    const f = fixture();
    f.intent.published = false;
    vi.mocked(f.transport.createDraft).mockImplementationOnce(async (_s, _d, _o, prepared) => {
      prepared("rejected-op");
      throw new Error("rejected");
    });
    await expect(f.actions.copy(f.intent)).rejects.toThrow("rejected");
    const previous = f.actions.inspectCopy(f.intent)!;
    f.transport.inspectBirth = vi.fn(() => ({
      operationId: "rejected-op",
      state: "rejected" as const,
      options: previous.options,
    }));
    const reviewed = {
      ...f.intent,
      reviewedDraft: { ...f.original, source_digest: "d".repeat(64) },
    };
    const reloaded = createDocumentDraftRecoveryActions(f.transport);
    expect(reloaded.canRestartCopy(reviewed)).toBe(true);
    await reloaded.restartCopy(reviewed);
    expect(f.transport.createDraft).toHaveBeenLastCalledWith(
      "space",
      "page",
      { force_new: true, source_draft_id: "original", source_digest: "d".repeat(64) },
      expect.any(Function),
    );
    expect(
      [...f.values.entries()]
        .filter(([k]) => k.includes(":archive:"))
        .map(([, v]) => JSON.parse(v)),
    ).toEqual([previous]);
  });
  for (const state of ["unknown", "prepared", "dispatched", "confirmed"] as const)
    it(`never restarts a ${state} birth`, async () => {
      const f = fixture();
      vi.mocked(f.transport.createDraft).mockImplementationOnce(async (_s, _d, _o, prepared) => {
        prepared("held-op");
        throw new Error("rejected-looking message");
      });
      await expect(f.actions.copy(f.intent)).rejects.toThrow();
      f.transport.inspectBirth = () => ({
        operationId: "held-op",
        state,
        options: { force_new: true },
      });
      expect(f.actions.canRestartCopy(f.intent)).toBe(false);
      await expect(f.actions.restartCopy(f.intent)).rejects.toThrow("unknown requests remain held");
      expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
    });
  it("rejects mismatched birth proof options without dispatch", async () => {
    const f = fixture();
    vi.mocked(f.transport.createDraft).mockImplementationOnce(async (_s, _d, _o, prepared) => {
      prepared("held-op");
      throw new Error("rejected");
    });
    await expect(f.actions.copy(f.intent)).rejects.toThrow();
    f.transport.inspectBirth = () => ({ operationId: "held-op", state: "rejected", options: {} });
    await expect(f.actions.restartCopy(f.intent)).rejects.toThrow("exactly proven");
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
  });
  it("retains typed copy rejection on reload and replaces only explicitly reviewed owned target source", async () => {
    const f = fixture();
    vi.mocked(f.transport.saveDraft).mockRejectedValueOnce(
      new DraftSaveHeldError("source_conflict", "changed"),
    );
    await expect(f.actions.copy(f.intent)).rejects.toThrow("changed");
    expect(f.actions.inspectCopy(f.intent)?.state).toBe("save-rejected");
    const previous = f.actions.inspectCopy(f.intent)!;
    f.transport.hasPendingTargetSave = vi.fn(() => false);
    const reloaded = createDocumentDraftRecoveryActions(f.transport);
    const reviewed = { ...f.target, source_digest: "d".repeat(64), content: "REMOTE" };
    await expect(
      reloaded.replaceCopyTarget(f.intent, { ...reviewed, id: "other" }),
    ).rejects.toThrow("unproven");
    await expect(
      reloaded.replaceCopyTarget(f.intent, { ...reviewed, owner_id: "other" }),
    ).rejects.toThrow("unproven");
    await reloaded.replaceCopyTarget(f.intent, reviewed);
    expect(f.transport.saveDraft).toHaveBeenLastCalledWith(
      "space",
      "page",
      "copy",
      f.intent.patch,
      "d".repeat(64),
    );
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
    expect(
      [...f.values.entries()]
        .filter(([k]) => k.includes(":archive:"))
        .map(([, v]) => JSON.parse(v)),
    ).toEqual([previous]);
  });
  it("keeps a review race rejected and a later unknown target save held without blind reissue", async () => {
    const f = fixture();
    const conflict = new DraftSaveHeldError("source_conflict", "changed again");
    vi.mocked(f.transport.saveDraft)
      .mockRejectedValueOnce(conflict)
      .mockRejectedValueOnce(conflict)
      .mockRejectedValueOnce(new DraftSaveHeldError("outcome_unknown", "lost response"));
    await expect(f.actions.copy(f.intent)).rejects.toThrow();
    f.transport.hasPendingTargetSave = () => false;
    const reviewed = { ...f.target, source_digest: "d".repeat(64) };
    await expect(f.actions.replaceCopyTarget(f.intent, reviewed)).rejects.toThrow("changed again");
    expect(f.actions.inspectCopy(f.intent)?.state).toBe("save-rejected");
    await expect(f.actions.replaceCopyTarget(f.intent, reviewed)).rejects.toThrow("lost response");
    expect(f.actions.inspectCopy(f.intent)?.state).toBe("save-held");
    await expect(f.actions.replaceCopyTarget(f.intent, reviewed)).rejects.toThrow(
      "unknown outcomes remain held",
    );
    await expect(f.actions.retryCopy(f.intent)).rejects.toThrow("explicit review");
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(3);
  });
  it("requires target queue inspection and refuses pending target save", async () => {
    const f = fixture();
    vi.mocked(f.transport.saveDraft).mockRejectedValueOnce(
      new DraftSaveHeldError("source_required", "required"),
    );
    await expect(f.actions.copy(f.intent)).rejects.toThrow();
    await expect(f.actions.replaceCopyTarget(f.intent, f.target)).rejects.toThrow(
      "retained target",
    );
    f.transport.hasPendingTargetSave = () => true;
    await expect(f.actions.replaceCopyTarget(f.intent, f.target)).rejects.toThrow(
      "queued or unknown",
    );
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
  });
  it("does not infer source rejection from untyped message text", async () => {
    const f = fixture();
    vi.mocked(f.transport.saveDraft).mockRejectedValueOnce(new Error("source_conflict"));
    await expect(f.actions.copy(f.intent)).rejects.toThrow();
    expect(f.actions.inspectCopy(f.intent)?.state).toBe("save-held");
    await expect(f.actions.replaceCopyTarget(f.intent, f.target)).rejects.toThrow(
      "unknown outcomes remain held",
    );
  });
});

describe("explicit document draft recovery actions", () => {
  it("replaces only the reviewed source and preserves submitted text after remote conflict", async () => {
    const f = fixture();
    vi.mocked(f.transport.saveDraft).mockRejectedValueOnce(new Error("source_conflict"));
    await expect(f.actions.replace(f.intent)).rejects.toThrow("source_conflict");
    expect(f.transport.saveDraft).toHaveBeenCalledWith(
      "space",
      "page",
      "original",
      f.intent.patch,
      "a".repeat(64),
    );
    expect(f.original.content).toBe("CURRENT");
    expect(f.intent.patch.content).toBe("SERIALIZED RECOVERED");
    expect(f.transport.createDraft).not.toHaveBeenCalled();
  });
  for (const holdReason of ["queued", "outcome_unknown"] as const) {
    it(`does not replace while an original save is ${holdReason}`, async () => {
      const f = fixture();
      await expect(f.actions.replace({ ...f.intent, holdReason })).rejects.toThrow(
        "queued or unknown",
      );
      expect(f.transport.saveDraft).not.toHaveBeenCalled();
    });
  }
  it("copies published recovery to a distinct owned target without modifying original evidence", async () => {
    const f = fixture();
    f.values.set("original-queue", "PRESERVE");
    const saved = await f.actions.copy({ ...f.intent, holdReason: "outcome_unknown" });
    expect(f.transport.createDraft).toHaveBeenCalledWith(
      "space",
      "page",
      { force_new: true },
      expect.any(Function),
    );
    expect(f.transport.saveDraft).toHaveBeenCalledWith(
      "space",
      "page",
      "copy",
      f.intent.patch,
      "b".repeat(64),
    );
    expect(f.values.get("original-queue")).toBe("PRESERVE");
    expect(f.original.content).toBe("CURRENT");
    expect(f.actions.inspectCopy(f.intent)?.state).toBe("confirmed");
    f.actions.finishCopy(f.intent, saved);
    expect(f.actions.inspectCopy(f.intent)?.state).toBe("finished");
    await expect(f.actions.copy(f.intent)).resolves.toEqual(saved);
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
  });
  it("forks only the exact editable source for unpublished recovery", async () => {
    const f = fixture();
    await f.actions.copy({ ...f.intent, published: false });
    expect(f.transport.createDraft).toHaveBeenCalledWith(
      "space",
      "page",
      {
        force_new: true,
        source_draft_id: "original",
        source_digest: "a".repeat(64),
      },
      expect.any(Function),
    );
    const denied = fixture();
    await expect(
      denied.actions.copy({
        ...denied.intent,
        published: false,
        reviewedDraft: { ...denied.original, access_role: "viewer" },
      }),
    ).rejects.toThrow("editable");
    expect(denied.transport.createDraft).not.toHaveBeenCalled();
  });
  it("retains original birth operation before context failure and retries exactly once", async () => {
    const f = fixture();
    let current = true;
    f.transport.verifySelection = vi.fn(() => {
      if (!current) throw new Error("context switched");
    });
    vi.mocked(f.transport.createDraft).mockImplementationOnce(
      async (_space, _page, _options, onPrepared) => {
        onPrepared("original-op");
        current = false;
        throw new DraftBirthHeldError("original-op", "unknown birth");
      },
    );
    await expect(f.actions.copy(f.intent)).rejects.toThrow("context switched");
    expect(f.actions.inspectCopy(f.intent)).toMatchObject({
      state: "birth-held",
      operationId: "original-op",
    });
    current = true;
    await f.actions.retryCopy(f.intent);
    expect(f.transport.retryBirth).toHaveBeenCalledWith("space", "page", "original-op");
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
  });
  it("persists acknowledged birth before a context switch and never creates it twice", async () => {
    const f = fixture();
    let current = true;
    f.transport.verifySelection = vi.fn(() => {
      if (!current) throw new Error("context switched");
    });
    vi.mocked(f.transport.createDraft).mockImplementationOnce(async () => {
      current = false;
      return f.target;
    });
    await expect(f.actions.copy(f.intent)).rejects.toThrow("context switched");
    expect(f.actions.inspectCopy(f.intent)).toMatchObject({
      state: "target-confirmed",
      target: f.target,
    });
    expect(f.transport.saveDraft).not.toHaveBeenCalled();
    current = true;
    await f.actions.retryCopy(f.intent);
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
    expect(f.transport.retryBirth).not.toHaveBeenCalled();
  });
  it("holds unknown copy save across controller reopen rather than retrying its bytes", async () => {
    const f = fixture();
    vi.mocked(f.transport.saveDraft).mockRejectedValueOnce(new Error("outcome unknown"));
    await expect(f.actions.copy(f.intent)).rejects.toThrow("outcome unknown");
    const reopened = createDocumentDraftRecoveryActions(f.transport);
    expect(reopened.inspectCopy(f.intent)).toMatchObject({ state: "save-held", target: f.target });
    await expect(reopened.retryCopy(f.intent)).rejects.toThrow("explicit review");
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
  });
  it("keeps exact confirmed save evidence if context switches before activation", async () => {
    const f = fixture();
    let current = true;
    f.transport.verifySelection = vi.fn(() => {
      if (!current) throw new Error("context switched");
    });
    vi.mocked(f.transport.saveDraft).mockImplementationOnce(async (_space, _page, id, patch) => {
      current = false;
      return {
        ...f.target,
        id,
        title: patch.title.trim(),
        content: patch.content,
        icon: undefined,
        source_digest: "c".repeat(64),
      };
    });
    await expect(f.actions.copy(f.intent)).rejects.toThrow("context switched");
    expect(f.actions.inspectCopy(f.intent)?.state).toBe("confirmed");
    current = true;
    await f.actions.retryCopy(f.intent);
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
  });
  it("blocks duplicate clicks and a second controller while birth is pending", async () => {
    const f = fixture();
    let release!: (draft: DraftRecord) => void;
    vi.mocked(f.transport.createDraft).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    const pending = f.actions.copy(f.intent);
    expect(f.actions.busy).toBe(true);
    await expect(f.actions.copy(f.intent)).rejects.toThrow("already pending");
    await expect(createDocumentDraftRecoveryActions(f.transport).copy(f.intent)).rejects.toThrow(
      "explicit review",
    );
    release(f.target);
    await pending;
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
  });
  it("requires durable intent before dispatch and refuses changed input after acknowledged birth", async () => {
    const f = fixture();
    f.transport.storage.setItem = () => {
      throw new Error("quota");
    };
    await expect(f.actions.copy(f.intent)).rejects.toThrow("quota");
    expect(f.transport.createDraft).not.toHaveBeenCalled();
    const other = fixture();
    await other.actions.copy(other.intent);
    await expect(
      other.actions.copy({ ...other.intent, patch: { ...other.intent.patch, content: "NEWER" } }),
    ).rejects.toThrow("differs");
    expect(other.transport.createDraft).toHaveBeenCalledTimes(1);
  });
  for (const malformed of ["wrong-id", "wrong-page", "missing-digest", "wrong-content"] as const) {
    it(`retains a copy save for review after ${malformed} acknowledgement`, async () => {
      const f = fixture();
      vi.mocked(f.transport.saveDraft).mockImplementationOnce(async (_space, _page, id, patch) => ({
        ...f.target,
        id: malformed === "wrong-id" ? "other" : id,
        document_id: malformed === "wrong-page" ? "other" : "page",
        title: patch.title.trim(),
        icon: undefined,
        content: malformed === "wrong-content" ? "OTHER" : patch.content,
        source_digest: malformed === "missing-digest" ? undefined : "c".repeat(64),
      }));
      await expect(f.actions.copy(f.intent)).rejects.toThrow("unproven");
      expect(f.actions.inspectCopy(f.intent)?.state).toBe("save-held");
      await expect(f.actions.retryCopy(f.intent)).rejects.toThrow("explicit review");
      expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
    });
  }
  it("does not adopt a foreign pending birth operation reported before preparation", async () => {
    const f = fixture();
    vi.mocked(f.transport.createDraft).mockRejectedValueOnce(
      new DraftBirthHeldError("FOREIGN", "existing request"),
    );
    await expect(f.actions.copy(f.intent)).rejects.toThrow("existing request");
    expect(f.actions.inspectCopy(f.intent)?.operationId).toBeUndefined();
    await expect(f.actions.retryCopy(f.intent)).rejects.toThrow("explicitly");
    expect(f.transport.retryBirth).not.toHaveBeenCalled();
  });
  it("recovers a browser interruption after prepared identity without a second birth", async () => {
    const f = fixture();
    vi.mocked(f.transport.createDraft).mockImplementationOnce(
      (_space, _page, _options, prepared) => {
        prepared("interrupted-op");
        return new Promise(() => {});
      },
    );
    void f.actions.copy(f.intent);
    expect(f.actions.inspectCopy(f.intent)).toMatchObject({
      state: "birth-pending",
      operationId: "interrupted-op",
    });
    const reopened = createDocumentDraftRecoveryActions(f.transport);
    await reopened.retryCopy(f.intent);
    expect(f.transport.retryBirth).toHaveBeenCalledWith("space", "page", "interrupted-op");
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
  });
  it("refuses uncoordinated browser dispatch and consults retained original writes", async () => {
    const f = fixture();
    delete f.transport.runExclusive;
    vi.stubGlobal("navigator", {});
    await expect(f.actions.copy(f.intent)).rejects.toThrow("Web Locks");
    expect(f.transport.createDraft).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
    const blocked = fixture();
    blocked.transport.hasPendingOriginalSave = () => true;
    await expect(blocked.actions.replace(blocked.intent)).rejects.toThrow("retained original");
    expect(blocked.transport.saveDraft).not.toHaveBeenCalled();
  });
  it("serializes two controllers under one exclusive journal lock and returns one saved copy", async () => {
    const f = fixture();
    let tail: Promise<unknown> = Promise.resolve();
    f.transport.runExclusive = (_key, run) => {
      const result = tail.then(run);
      tail = result.then(
        () => undefined,
        () => undefined,
      );
      return result;
    };
    const other = createDocumentDraftRecoveryActions(f.transport);
    const [a, b] = await Promise.all([f.actions.copy(f.intent), other.copy(f.intent)]);
    expect(a).toEqual(b);
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
  });

  it("inspects retained original birth intent after review changes without adopting new source", async () => {
    const f = fixture();
    await f.actions.copy(f.intent);
    const refreshed = {
      ...f.intent,
      published: false,
      reviewedDraft: { ...f.original, content: "REMOTE NEW", source_digest: "d".repeat(64) },
    };
    expect(f.actions.inspectCopy(refreshed)?.intent).toEqual(f.intent);
    expect(
      f.actions.inspectCopy({
        ...refreshed,
        patch: { ...refreshed.patch, cover_image: "NEW REVIEW COVER" },
      })?.intent,
    ).toEqual(f.intent);
    await expect(f.actions.retryCopy(refreshed)).rejects.toThrow("differs");
    for (const changed of [
      { ...refreshed, checkpoint: { ...refreshed.checkpoint, content: "NEW LOCAL" } },
      { ...refreshed, selection: { ...refreshed.selection, storageSelection: "OTHER" } },
    ])
      expect(() => f.actions.inspectCopy(changed)).toThrow("differs");
    expect(f.transport.createDraft).toHaveBeenCalledTimes(1);
  });
  it("captures click intent before lock wait while newer caller bytes remain separate", async () => {
    const f = fixture();
    let grant!: () => void;
    f.transport.runExclusive = (_key, run) =>
      new Promise<void>((resolve) => {
        grant = resolve;
      }).then(run);
    const pending = f.actions.copy(f.intent);
    f.intent.patch.content = "NEWER CALLER";
    f.intent.checkpoint.content = "NEWER CHECKPOINT";
    grant();
    const saved = await pending;
    expect(saved.content).toBe("SERIALIZED RECOVERED");
    expect(f.intent.patch.content).toBe("NEWER CALLER");
    expect(f.transport.saveDraft).toHaveBeenCalledWith(
      "space",
      "page",
      "copy",
      expect.objectContaining({ content: "SERIALIZED RECOVERED" }),
      "b".repeat(64),
    );
  });
  it("does not claim server source protection for local reviewed replacement", async () => {
    const f = fixture();
    await expect(
      f.actions.replace({
        ...f.intent,
        scope: { ...f.intent.scope, storageMode: "local", serverTarget: "local-workspace" },
        selection: { ...f.intent.selection, mode: "local", serverBase: "local-workspace" },
        reviewedDraft: { ...f.original, source_digest: undefined },
      }),
    ).rejects.toThrow("separate recovery copy");
    expect(f.transport.saveDraft).not.toHaveBeenCalled();
  });
});

describe("durable reviewed replacement evidence", () => {
  it("retains both exact sources before dispatch and the own acknowledgement across restart", async () => {
    const f = fixture();
    f.intent.reviewedDraft.content =
      "REMOTE\r\n<!-- starlane:custom-css -->\n\0@import blocked;\n<!-- /starlane:custom-css -->\n";
    Object.assign(f.intent.reviewedDraft, {
      unknown_metadata: { exact: "  RAW\r\n" },
      icon: "remote-icon",
    });
    f.intent.checkpoint.content = "LOCAL EDIT\r\n";
    f.intent.patch.content = "LOCAL SERIALIZED\r\n\0";
    vi.mocked(f.transport.saveDraft).mockImplementation(async (_space, _page, id, patch) => {
      expect(f.actions.inspectReplacement(f.intent)).toMatchObject({
        state: "save-pending",
        intent: f.intent,
      });
      return {
        ...f.original,
        id,
        title: patch.title.trim(),
        content: patch.content,
        icon: undefined,
        source_digest: "c".repeat(64),
      };
    });
    const saved = await f.actions.replace(f.intent);
    const reopened = createDocumentDraftRecoveryActions(f.transport);
    expect(reopened.inspectReplacement(f.intent)).toEqual({
      version: 1,
      kind: "replacement",
      intent: JSON.parse(JSON.stringify(f.intent)),
      state: "confirmed",
      saved: JSON.parse(JSON.stringify(saved)),
    });
    await expect(reopened.replace(f.intent)).resolves.toEqual(saved);
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
    expect(f.actions.inspectCopy(f.intent)).toBeNull();
  });
  it("keeps an unknown replacement and original queue while a separate copy remains possible", async () => {
    const f = fixture();
    f.values.set("original-queue", "EXACT REQUEST");
    vi.mocked(f.transport.saveDraft).mockRejectedValueOnce(
      new DraftSaveHeldError("outcome_unknown", "unknown"),
    );
    await expect(f.actions.replace(f.intent)).rejects.toThrow("unknown");
    const reopened = createDocumentDraftRecoveryActions(f.transport);
    expect(reopened.inspectReplacement(f.intent)).toMatchObject({
      state: "save-held",
      intent: f.intent,
    });
    await expect(reopened.replace(f.intent)).rejects.toThrow("no second request");
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
    await reopened.copy(f.intent);
    expect(reopened.inspectCopy(f.intent)?.state).toBe("confirmed");
    expect(reopened.inspectReplacement(f.intent)?.state).toBe("save-held");
    expect(f.values.get("original-queue")).toBe("EXACT REQUEST");
  });
  it("archives a rejected review before dispatching a newly reviewed source", async () => {
    const f = fixture();
    vi.mocked(f.transport.saveDraft).mockRejectedValueOnce(
      new DraftSaveHeldError("source_conflict", "changed"),
    );
    await expect(f.actions.replace(f.intent)).rejects.toThrow("changed");
    const previous = f.actions.inspectReplacement(f.intent);
    await expect(f.actions.replace(f.intent)).rejects.toThrow("no second request");
    const reviewed = {
      ...f.intent,
      reviewedDraft: { ...f.original, content: "NEW REMOTE\r\n", source_digest: "d".repeat(64) },
    };
    await f.actions.replace(reviewed);
    const record = f.actions.inspectReplacement(reviewed)!;
    expect(record.state).toBe("confirmed");
    expect(record.previousArchiveKey).toBeTruthy();
    expect(f.actions.inspectReplacement(reviewed, record.previousArchiveKey)).toEqual(previous);
    expect(f.transport.saveDraft).toHaveBeenLastCalledWith(
      "space",
      "page",
      "original",
      reviewed.patch,
      "d".repeat(64),
    );
  });
  for (const failure of ["quota", "readback"] as const) {
    it("does not dispatch when pre-request evidence fails: " + failure, async () => {
      const f = fixture();
      f.transport.storage.setItem = () => {
        if (failure === "quota") throw new Error("quota");
      };
      await expect(f.actions.replace(f.intent)).rejects.toThrow();
      expect(f.transport.saveDraft).not.toHaveBeenCalled();
    });
  }
  it("preserves pending sources when confirmed-result storage fails", async () => {
    const f = fixture();
    const write = f.transport.storage.setItem;
    let calls = 0;
    f.transport.storage.setItem = (key, raw) => {
      if (++calls === 3) throw new Error("post-ACK quota");
      write(key, raw);
    };
    await expect(f.actions.replace(f.intent)).rejects.toThrow("post-ACK quota");
    const reopened = createDocumentDraftRecoveryActions(f.transport);
    expect(reopened.inspectReplacement(f.intent)).toMatchObject({
      state: "save-pending",
      intent: f.intent,
    });
    await expect(reopened.replace(f.intent)).rejects.toThrow("no second request");
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
  });
  it("does not replace old evidence when rejected-review archival fails", async () => {
    const f = fixture();
    vi.mocked(f.transport.saveDraft).mockRejectedValueOnce(
      new DraftSaveHeldError("source_conflict", "changed"),
    );
    await expect(f.actions.replace(f.intent)).rejects.toThrow();
    const before = new Map(f.values);
    const write = f.transport.storage.setItem;
    f.transport.storage.setItem = (key, raw) => {
      if (key.includes(":archive:")) throw new Error("archive quota");
      write(key, raw);
    };
    await expect(
      f.actions.replace({
        ...f.intent,
        reviewedDraft: { ...f.original, source_digest: "e".repeat(64) },
      }),
    ).rejects.toThrow("archive quota");
    expect(f.values).toEqual(before);
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
  });
  it("refuses corrupt and forged confirmed provenance without another dispatch", async () => {
    for (const corrupt of ["json", "owner", "state"]) {
      const f = fixture();
      await f.actions.replace(f.intent);
      vi.mocked(f.transport.saveDraft).mockClear();
      const [key, raw] = [...f.values].find(([key]) => key.endsWith(":replacement"))!;
      const record = JSON.parse(raw);
      if (corrupt === "owner") record.saved.owner_id = "foreign";
      if (corrupt === "state") record.state = "invented";
      f.values.set(key, corrupt === "json" ? "{" : JSON.stringify(record));
      expect(() => f.actions.inspectReplacement(f.intent)).toThrow();
      await expect(f.actions.replace(f.intent)).rejects.toThrow();
      expect(f.transport.saveDraft).not.toHaveBeenCalled();
    }
  });
  it("refuses oversized and unserializable proposals without dispatch", async () => {
    const f = fixture();
    await expect(
      f.actions.replace({
        ...f.intent,
        reviewedDraft: { ...f.original, content: "X".repeat(32 * 1024 * 1024) },
      }),
    ).rejects.toThrow("bound");
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    await expect(
      f.actions.replace({
        ...f.intent,
        reviewedDraft: Object.assign({ ...f.original }, { unknown_metadata: circular }),
      }),
    ).rejects.toThrow();
    expect(f.transport.saveDraft).not.toHaveBeenCalled();
    expect(f.values.size).toBe(0);
  });
  it("retains original-context acknowledgement when selection changes during the request", async () => {
    const f = fixture();
    let switched = false;
    vi.mocked(f.transport.verifySelection).mockImplementation(() => {
      if (switched) throw new Error("context switched");
    });
    vi.mocked(f.transport.saveDraft).mockImplementation(async (_space, _page, id, patch) => {
      switched = true;
      return {
        ...f.original,
        id,
        title: patch.title.trim(),
        content: patch.content,
        icon: undefined,
        source_digest: "c".repeat(64),
      };
    });
    await expect(f.actions.replace(f.intent)).rejects.toThrow("context switched");
    switched = false;
    expect(f.actions.inspectReplacement(f.intent)).toMatchObject({
      state: "confirmed",
      intent: f.intent,
    });
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
  });
  it("does not redispatch another controller while a replacement is pending", async () => {
    const f = fixture();
    let resolve!: (draft: DraftRecord) => void;
    vi.mocked(f.transport.saveDraft).mockImplementation(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    const pending = f.actions.replace(f.intent);
    await expect(createDocumentDraftRecoveryActions(f.transport).replace(f.intent)).rejects.toThrow(
      "no second request",
    );
    resolve({
      ...f.original,
      title: f.intent.patch.title.trim(),
      content: f.intent.patch.content,
      icon: undefined,
      source_digest: "c".repeat(64),
    });
    await pending;
    expect(f.transport.saveDraft).toHaveBeenCalledTimes(1);
  });
});

describe("replacement history discovery", () => {
  it("discovers exact confirmed remote/local evidence after the original checkpoint is gone", async () => {
    const f = fixture();
    await f.actions.replace(f.intent);
    f.values.delete("original-checkpoint");
    const records = createDocumentDraftRecoveryActions(f.transport).listReplacements(
      f.intent.scope,
      f.intent.selection,
    );
    expect(records).toEqual([f.actions.inspectReplacement(f.intent)]);
    expect(records[0].intent.reviewedDraft.content).toBe("CURRENT");
    expect(records[0].intent.patch.content).toBe("SERIALIZED RECOVERED");
    expect(JSON.parse(JSON.stringify(records[0])).saved.content).toBe("SERIALIZED RECOVERED");
  });
  it("excludes other account/server/draft and session histories without changing evidence", async () => {
    const f = fixture();
    await f.actions.replace(f.intent);
    const [key, raw] = [...f.values][0];
    for (const field of ["authIdentity", "serverTarget", "draftId"] as const) {
      const foreign = JSON.parse(raw);
      foreign.intent.scope[field] = "OTHER";
      f.values.set(
        key
          .replace(":snapshot:replacement", ":" + field + ":replacement")
          .replace(":me:", ":OTHER:"),
        JSON.stringify(foreign),
      );
    }
    const session = JSON.parse(raw);
    session.intent.selection.storageSelection = "OLD SESSION";
    session.intent.checkpoint.snapshotId = "older-session";
    f.values.set(
      key.replace(":snapshot:replacement", ":older-session:replacement"),
      JSON.stringify(session),
    );
    const before = new Map(f.values);
    expect(f.actions.listReplacements(f.intent.scope, f.intent.selection)).toHaveLength(1);
    expect(f.values).toEqual(before);
  });
  it("fails closed on corrupt matching history rather than claiming none exists", async () => {
    const f = fixture();
    await f.actions.replace(f.intent);
    f.values.set([...f.values.keys()][0], "{");
    expect(() => f.actions.listReplacements(f.intent.scope, f.intent.selection)).toThrow("corrupt");
  });
  it("requires bounded enumerable storage only for replacement, preserving legacy copy capability", async () => {
    const f = fixture();
    delete f.transport.storage.key;
    await expect(f.actions.replace(f.intent)).rejects.toThrow("enumerable");
    expect(f.transport.saveDraft).not.toHaveBeenCalled();
    await f.actions.copy(f.intent);
    expect(f.actions.inspectCopy(f.intent)?.state).toBe("confirmed");
    const unreadable = fixture();
    unreadable.transport.storage.key = () => {
      throw new Error("enumeration denied");
    };
    await expect(unreadable.actions.replace(unreadable.intent)).rejects.toThrow(
      "enumeration denied",
    );
    expect(unreadable.transport.saveDraft).not.toHaveBeenCalled();
    const huge = fixture();
    Object.defineProperty(huge.transport.storage, "length", { get: () => 10001 });
    expect(() => huge.actions.listReplacements(huge.intent.scope, huge.intent.selection)).toThrow(
      "bounded",
    );
    await expect(huge.actions.replace(huge.intent)).rejects.toThrow("bounded");
    expect(huge.transport.saveDraft).not.toHaveBeenCalled();
  });
});
