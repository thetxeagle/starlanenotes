import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ApiError } from "@/lib/api";
import {
  appendDraftDiscussionMention,
  DraftDiscussionPanel,
  draftDiscussionErrorMessage,
} from "./DraftDiscussionPanel";

const target = {
  spaceId: "space-a",
  documentId: "page-a",
  draftId: "draft-a",
  sourceDigest: "a".repeat(64),
};

describe("DraftDiscussionPanel", () => {
  it("renders an exact-draft composer with shared controls", () => {
    const html = renderToStaticMarkup(
      <DraftDiscussionPanel
        target={target}
        currentUserId="user-a"
        pendingAnchor={{
          source_digest: target.sourceDigest,
          selection_start: 0,
          selection_end: 5,
          selection_text: "alpha",
        }}
      />,
    );

    expect(html).toContain("Draft discussion");
    expect(html).toContain("Commenting on “alpha”");
    expect(html).toContain('aria-label="New draft comment"');
    expect(html).toContain("Mention");
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
  });

  it("states that rejected mutations retain locally entered text", () => {
    expect(draftDiscussionErrorMessage(new ApiError("conflict", 409))).toContain(
      "Your text is still here",
    );
    expect(draftDiscussionErrorMessage(new ApiError("forbidden", 403))).toContain(
      "Your text is still here",
    );
    expect(draftDiscussionErrorMessage(new Error("offline"))).toContain("Your text is still here");
  });

  it("appends portable exact-user mentions without allowing display text to break syntax", () => {
    expect(
      appendDraftDiscussionMention("Review", {
        display_name: "Pat ](outsider)\nSmith",
        user_id: "user-123",
      }),
    ).toBe("Review @[Pat outsider Smith](user-123) ");
  });
});
