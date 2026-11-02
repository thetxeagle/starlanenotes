import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { parsePageAccessInspection } from "@/lib/pageAccessInspection";
import { PageAccessReport } from "./PageShareDialog";

function report() {
  return parsePageAccessInspection(
    {
      schema_version: 1,
      space_id: "space",
      page_id: "page",
      checked_at: "2026-10-08T12:00:00Z",
      capabilities: {
        view: true,
        comment: false,
        edit: false,
        move: false,
        manage_lock: false,
        enable_public_link: false,
        disable_public_link: false,
      },
      space_permission_sources: [
        { kind: "global_group_role", permissions: ["view", "edit_content"] },
      ],
      restrictions: { local: "inherited", ancestor_restricted: false },
      public_link: { configured: true, policy_allows_anonymous: true, reason: null },
      selected_private_draft: { scope: "shared_viewer", view: true, edit: false },
      sharing: { people_grants_available: false, guests_available: false },
    },
    "space",
    "page",
  );
}
describe("Share access projection", () => {
  it("uses checked page capabilities rather than expanding inherited permissions", () => {
    const html = renderToStaticMarkup(<PageAccessReport report={report()} />);
    expect(html).toContain("Edit content</dt><dd");
    expect(html).toContain("Not allowed");
    expect(html).toContain("Global group membership");
    expect(html).not.toContain("edit_content");
    expect(html).not.toContain('role="button"');
  });
  it("separates public policy from content availability and selected draft from open draft", () => {
    const html = renderToStaticMarkup(<PageAccessReport report={report()} />);
    expect(html).toContain("published content availability is checked separately");
    expect(html).toContain("does not describe the draft currently open");
    expect(html).toContain("shared with you as a viewer");
    expect(html).toContain("guest invitations are not available yet");
    expect(html).toContain("point-in-time report");
    expect(html).not.toContain("href=");
  });
});
