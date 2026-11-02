import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PageRestrictionControls, restrictionProposalFromInspection, restrictionReviewIsCurrent, restrictionSelectionLabel } from "./PageRestrictionControls";
import type { RestrictionInspection, RestrictionPreview } from "@/lib/pageRestrictionManagement";
function inspection(): RestrictionInspection {
  return { schema_version: 1, space_id: "space", document_id: "page", checked_at: "2026-10-09T12:00:00Z", can_apply: false,
    policy: { mode: "restricted", revision: "current", allowlist: { users: [{ id: "self", available: true }, { id: "old", available: false }], groups: [{ id: "removed-group", available: false }] } },
    eligible_recipients: { users: [{ id: "self", display_name: "Self" }], groups: [] } };
}
function preview(): RestrictionPreview {
  return { schema_version: 1, space_id: "space", document_id: "page", preview_token: "opaque", expires_at: "2026-10-09T12:10:00Z", scope_notice: "All descendants", readable_pages: [] };
}
describe("restriction Share controls", () => {
  it("retains every stored selection for explicit removal instead of intersecting it away", () => {
    expect(restrictionProposalFromInspection(inspection())).toEqual({ mode: "restricted", allowlist: { users: ["self", "old"], groups: ["removed-group"] } });
  });
  it("never carries an old eligible label across catalog refresh or failed inspection", () => {
    const before = inspection();
    before.eligible_recipients.users.push({ id: "old", display_name: "Previously visible label" });
    expect(restrictionSelectionLabel(before, "users", "old")).toBe("Previously visible label");
    expect(restrictionSelectionLabel(inspection(), "users", "old")).toBe("Unavailable person: old");
    expect(restrictionSelectionLabel(null, "users", "old")).toBe("Unavailable person: old");
    expect(restrictionSelectionLabel(inspection(), "groups", "removed-group")).toBe("Unavailable group: removed-group");
  });
  it("does not reinterpret stored IDs or use them to look up global identities", () => {
    const current = inspection(); current.eligible_recipients.users[0].display_name = "";
    expect(restrictionSelectionLabel(current, "users", "self")).toBe("Unnamed person");
    expect(restrictionSelectionLabel(current, "users", "some-foreign-id")).toBe("Unavailable person: some-foreign-id");
  });
  it("invalidates reviewed confirmation on any edited generation or exact expiry boundary", () => {
    const review = { response: preview(), generation: 3 };
    expect(restrictionReviewIsCurrent(review, 3, Date.parse("2026-10-09T12:09:59Z"))).toBe(true);
    expect(restrictionReviewIsCurrent(review, 4, Date.parse("2026-10-09T12:09:59Z"))).toBe(false);
    expect(restrictionReviewIsCurrent(review, 3, Date.parse("2026-10-09T12:10:00Z"))).toBe(false);
  });
  it("renders separate restriction loading without needing an ordinary access report", () => {
    const html = renderToStaticMarkup(<PageRestrictionControls spaceId="space" pageId="page" contextKey="account" accessRefreshVersion={0} assertContext={() => {}} onConfirmed={() => {}} />);
    expect(html).toContain("Page restriction"); expect(html).toContain("Inspecting current restriction");
    expect(html).toContain("Restrictions limit existing access"); expect(html).toContain("Refresh restrictions");
    expect(html).not.toContain("Confirm restriction"); expect(html).not.toContain("Copy page link");
  });
});
