import { afterEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import {
  createDraftDiscussionComment,
  createDraftDiscussionAnchor,
  closedDraftDiscussionsRoute,
  deleteDraftDiscussionComment,
  draftDiscussionCommentRoute,
  draftDiscussionRoute,
  draftDiscussionScope,
  getClosedDraftDiscussion,
  isDraftDiscussionAnchorCurrent,
  listDraftDiscussion,
  listClosedDraftDiscussions,
  resolveDraftDiscussionComment,
  updateDraftDiscussionComment,
} from "./draftDiscussion";

const target = {
  spaceId: "space/a",
  documentId: "page b",
  draftId: "draft?c",
  sourceDigest: "sha256:one",
};

describe("draft discussion identity", () => {
  afterEach(() => vi.restoreAllMocks());

  it("binds routes and request scope to the exact selected draft source", () => {
    expect(draftDiscussionRoute(target)).toBe(
      "/spaces/space%2Fa/documents/page%20b/drafts/draft%3Fc/discussion",
    );
    expect(draftDiscussionScope(target)).toContain("draft?c\u0000sha256:one");
    expect(draftDiscussionScope({ ...target, sourceDigest: "sha256:two" })).not.toBe(
      draftDiscussionScope(target),
    );
  });

  it("sends every operation to the exact selected draft and source", async () => {
    const fetch = vi.spyOn(api, "fetch");
    fetch
      .mockResolvedValueOnce({
        discussion_id: null,
        draft_id: target.draftId,
        source_digest: target.sourceDigest,
        state: "active",
        mention_candidates: [],
        comments: [],
      })
      .mockResolvedValueOnce({ id: "comment", discussion_id: "discussion" })
      .mockResolvedValue(undefined);

    await listDraftDiscussion(target);
    await createDraftDiscussionComment(
      target,
      "Review this",
      createDraftDiscussionAnchor("alpha", target.sourceDigest, 0, 5),
    );
    await updateDraftDiscussionComment(target, "comment/id", "Updated");
    await resolveDraftDiscussionComment(target, "comment/id");
    await deleteDraftDiscussionComment(target, "comment/id");

    expect(fetch).toHaveBeenNthCalledWith(
      1,
      `${draftDiscussionRoute(target)}?source_digest=sha256%3Aone`,
    );
    expect(fetch).toHaveBeenNthCalledWith(2, draftDiscussionRoute(target), {
      method: "POST",
      body: {
        source_digest: target.sourceDigest,
        content: "Review this",
        parent_id: null,
        selection_start: 0,
        selection_end: 5,
        selection_text: "alpha",
      },
    });
    expect(fetch).toHaveBeenNthCalledWith(3, draftDiscussionCommentRoute(target, "comment/id"), {
      method: "PUT",
      body: { source_digest: target.sourceDigest, content: "Updated" },
    });
    expect(fetch).toHaveBeenNthCalledWith(
      4,
      `${draftDiscussionCommentRoute(target, "comment/id")}/resolve?source_digest=sha256%3Aone`,
      { method: "POST" },
    );
    expect(fetch).toHaveBeenNthCalledWith(
      5,
      `${draftDiscussionCommentRoute(target, "comment/id")}?source_digest=sha256%3Aone`,
      { method: "DELETE" },
    );
  });

  it("rejects a response for a different draft source", async () => {
    vi.spyOn(api, "fetch").mockResolvedValue({
      discussion_id: null,
      draft_id: target.draftId,
      source_digest: "different",
      state: "active",
      mention_candidates: [],
      comments: [],
    });

    await expect(listDraftDiscussion(target)).rejects.toThrow(
      "does not match the selected draft source",
    );
  });

  it("lists and reads only the selected page's closed custody", async () => {
    const fetch = vi.spyOn(api, "fetch");
    fetch
      .mockResolvedValueOnce({
        schema_version: 1,
        space_id: target.spaceId,
        document_id: target.documentId,
        discussions: [
          {
            id: "discussion/id",
            draft_id: target.draftId,
            publication_operation_id: "publication/id",
            closed_at: "2026-10-22T12:00:00Z",
            comment_count: 2,
            unresolved_count: 0,
          },
        ],
      })
      .mockResolvedValueOnce({
        discussion_id: "discussion/id",
        draft_id: target.draftId,
        source_digest: target.sourceDigest,
        state: "closed",
        mention_candidates: [],
        comments: [],
      });

    const records = await listClosedDraftDiscussions(target.spaceId, target.documentId);
    const discussion = await getClosedDraftDiscussion(
      target.spaceId,
      target.documentId,
      records[0].id,
    );

    expect(fetch).toHaveBeenNthCalledWith(
      1,
      closedDraftDiscussionsRoute(target.spaceId, target.documentId),
    );
    expect(fetch).toHaveBeenNthCalledWith(
      2,
      `${closedDraftDiscussionsRoute(target.spaceId, target.documentId)}/discussion%2Fid`,
    );
    expect(discussion.state).toBe("closed");
  });

  it("rejects closed custody from a different page or discussion", async () => {
    vi.spyOn(api, "fetch")
      .mockResolvedValueOnce({
        schema_version: 1,
        space_id: "other-space",
        document_id: target.documentId,
        discussions: [],
      })
      .mockResolvedValueOnce({
        discussion_id: "other-discussion",
        draft_id: target.draftId,
        source_digest: target.sourceDigest,
        state: "closed",
        mention_candidates: [],
        comments: [],
      });

    await expect(listClosedDraftDiscussions(target.spaceId, target.documentId)).rejects.toThrow(
      "does not match the current page",
    );
    await expect(
      getClosedDraftDiscussion(target.spaceId, target.documentId, "discussion"),
    ).rejects.toThrow("does not match the selected custody record");
  });

  it("retains exact UTF-16 selections without splitting surrogate pairs", () => {
    const source = "A😀BC";
    expect(createDraftDiscussionAnchor(source, "digest", 1, 3)).toEqual({
      source_digest: "digest",
      selection_start: 1,
      selection_end: 3,
      selection_text: "😀",
    });
    expect(createDraftDiscussionAnchor(source, "digest", 2, 3)).toBeNull();
  });

  it("marks anchors outdated instead of rebinding after source drift", () => {
    const anchor = createDraftDiscussionAnchor("alpha beta", "one", 6, 10)!;
    expect(isDraftDiscussionAnchorCurrent(anchor, "alpha beta", "one")).toBe(true);
    expect(isDraftDiscussionAnchorCurrent(anchor, "alpha zeta", "two")).toBe(false);
    expect(isDraftDiscussionAnchorCurrent(anchor, "alpha zeta", "one")).toBe(false);
  });
});
