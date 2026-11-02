import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ClosedDraftDiscussionHistory } from "./ClosedDraftDiscussionHistory";

describe("closed draft discussion history", () => {
  it("presents owner custody as read-only inspection", () => {
    const markup = renderToStaticMarkup(
      <ClosedDraftDiscussionHistory
        records={[{
          id: "discussion",
          draft_id: "draft",
          publication_operation_id: "publication",
          closed_at: "2026-10-22T12:00:00Z",
          comment_count: 2,
          unresolved_count: 1,
        }]}
        loading={false}
        error={false}
        onRetry={() => {}}
        onOpen={() => {}}
      />,
    );

    expect(markup).toContain("Read-only owner records closed by publication");
    expect(markup).toContain("2 comments");
    expect(markup).toContain("1 unresolved");
    expect(markup).toContain("Inspect discussion");
    expect(markup).not.toMatch(/Edit|Reply|Reopen|Resolve/);
  });

  it("owns loading, empty, and retryable error states", () => {
    const render = (loading: boolean, error: boolean) => renderToStaticMarkup(
      <ClosedDraftDiscussionHistory records={[]} loading={loading} error={error} onRetry={() => {}} onOpen={() => {}} />,
    );
    expect(render(true, false)).toContain("Loading retained discussions");
    expect(render(false, false)).toContain("No retained draft discussions for this page");
    expect(render(false, true)).toContain("Retry retained discussions");
  });
});
