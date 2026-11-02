import { describe, expect, it } from "vitest";
import { getInlineCommentRanges } from "./InlineCommentDecorations";
import type { DocumentComment } from "@/lib/documentComments";

function comment(overrides: Partial<DocumentComment> = {}): DocumentComment {
  return {
    id: "comment-1",
    document_id: "document-1",
    user_id: "user-1",
    user_name: "Eagle",
    content: "Review this.",
    selection_start: 2,
    selection_end: 8,
    selection_text: "Review",
    resolved: false,
    created_at: "2026-07-26T00:00:00Z",
    replies: [],
    ...overrides,
  };
}

describe("inline comment decorations", () => {
  it("shows only unresolved comments with complete text anchors", () => {
    expect(
      getInlineCommentRanges(
        [
          comment(),
          comment({ id: "resolved", resolved: true }),
          comment({ id: "document-comment", selection_start: undefined }),
        ],
        true,
      ),
    ).toEqual([{ id: "comment-1", from: 2, to: 8 }]);
  });

  it("clears every marker when inline comments are hidden", () => {
    expect(getInlineCommentRanges([comment()], false)).toEqual([]);
  });
});
