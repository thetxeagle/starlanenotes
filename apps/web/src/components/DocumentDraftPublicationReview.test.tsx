import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DocumentDraftPublicationReview } from "./DocumentDraftPublicationReview";
const publication = {
  title: "Exact saved title",
  content: "<script>run()</script>\nSaved body",
  icon: "🌙",
  cover_image: "/saved-cover",
};
const callbacks = { onConfirm() {}, onCancel() {} };
describe("exact saved publication review", () => {
  it("shows saved source without executing it or implying a new audience grant", () => {
    const html = renderToStaticMarkup(
      <DocumentDraftPublicationReview
        publication={publication}
        pagePublished
        busy={false}
        {...callbacks}
      />,
    );
    expect(html).toContain("Exact saved title");
    expect(html).toContain("&lt;script&gt;run()&lt;/script&gt;");
    expect(html).not.toContain("<script>");
    expect(html).toContain("/saved-cover");
    expect(html).toContain("Publishing does not grant access");
    expect(html).toContain("Later draft or page changes will be rejected");
  });
  it("keeps the source readable while blocking repeat confirmation of an uncertain request", () => {
    const html = renderToStaticMarkup(
      <DocumentDraftPublicationReview
        publication={publication}
        pagePublished={false}
        busy={false}
        confirmBlocked="Retry the original publication after closing this review."
        cancelLabel="Close review"
        {...callbacks}
      />,
    );
    expect(html).toContain("Publish the first page revision");
    expect(html).toContain("Close review");
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>Publish reviewed draft<\/button>/);
    expect(html).toContain("Retry the original publication");
  });
});
