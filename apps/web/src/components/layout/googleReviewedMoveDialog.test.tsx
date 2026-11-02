import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { LocalReviewedMoveDialog, createLocalMoveSubmission } from "./LocalReviewedMoveDialog";
import { buildLocalMovePreview } from "@/lib/reviewedMoves";
import { prepareGoogleReviewedMove, type LocalReviewedMoveDecisions } from "@/lib/localReviewedMove";
import type { LocalVaultSnapshot } from "@/lib/localVault";
import type { LocalReviewedMoveSession } from "@/lib/dataClient";
const snapshot: LocalVaultSnapshot = { schema_version: 1, spaces: ["source", "destination"].map(id => ({ id, name: id, created_at: "old", updated_at: "old" })), folders: [], documents: [{ id: "page", space_id: "source", title: "Page", content: "exact body", published_at: "old", updated_at: "old" }], assets: [], templates: [] };
const decisions: LocalReviewedMoveDecisions = { properties: [], workflows: [], tags: [], relations: [], identifiers: [], maintenance: [], detach_hierarchy: [], preserve_previous_draft_placement: true };
const identity = { operation_id: "12345678-1234-1234-1234-123456789abc", moved_at: "2026-10-09T12:00:00Z" };
function session(): LocalReviewedMoveSession {
  const preview = buildLocalMovePreview(snapshot, { source_space_id: "source", root: { kind: "document", id: "page" }, destination: { space_id: "destination", folder_id: null, parent_id: null } }, "local-user");
  const prepared = prepareGoogleReviewedMove(snapshot, preview, { ...identity, actor_id: "local-user", decisions }, { schema_version: 1, provider: "google-drive", account_permission_id: "A", root_id: "root" });
  return { preview, provider: "google-drive", provider_status: "pending_provider", verifyCurrent: vi.fn(), validate: vi.fn(), relationTargets: vi.fn(() => []), commit: vi.fn(async () => ({ canonical_committed: true as const, record: prepared.record, newer_local_changes_retained: false, provider_status: "pending_provider" as const })) };
}
describe("existing Google move dialog consumer status", () => {
  it("admits the same original schema2 Google operation only as locally committed and pending provider", async () => { const original = session(); const submission = createLocalMoveSubmission(original, () => true, () => identity); expect((await submission.submit(decisions))!.provider_status).toBe("pending_provider"); expect(submission.confirmed()).toBe(true); });
  it("does not admit a remote-applied-looking or provider-none acknowledgment for a Google operation", async () => { const original = session(); const result = await original.commit({ ...identity, decisions }); vi.mocked(original.commit).mockResolvedValue({ ...result, provider_status: undefined }); await expect(createLocalMoveSubmission(original, () => true, () => identity).submit(decisions)).rejects.toThrow("unconfirmed"); });
  it("explains local commit, pending publication and held manual sync before dispatch using existing controls", () => { const html = renderToStaticMarkup(<LocalReviewedMoveDialog session={session()} title="Page" destinationName="Destination" onClose={vi.fn()} onCommitted={vi.fn()} />); expect(html).toContain("Review Google workspace move"); expect(html).toContain("Remote publication and received application are unavailable"); expect(html).toContain("manual sync holds"); expect(html).not.toContain("Move the complete subtree in this standalone vault"); });
});
