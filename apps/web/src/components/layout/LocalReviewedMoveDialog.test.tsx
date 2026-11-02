import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { LocalVaultSnapshot } from "@/lib/localVault";
import type { LocalReviewedMoveSession } from "@/lib/dataClient";
import { buildLocalMovePreview } from "@/lib/reviewedMoves";
import { prepareLocalReviewedMove, type LocalReviewedMoveDecisions } from "@/lib/localReviewedMove";
import { createLocalMoveSubmission, localMoveMappingCandidateAvailable, localMoveReadiness, updateLocalMoveMenuOwnership, LocalReviewedMoveDialog } from "./LocalReviewedMoveDialog";

const operation = "12345678-1234-4123-8123-123456789abc";
const moved_at = "2026-10-09T12:00:00Z";
const intent = { source_space_id: "source", root: { kind: "document" as const, id: "page" }, destination: { space_id: "target", folder_id: null, parent_id: null } };
function snapshot(): LocalVaultSnapshot {
  return {
    schema_version: 1, spaces: ["source", "target"].map(id => ({ id, name: id, created_at: "old", updated_at: "old" })), folders: [], templates: [], assets: [],
    documents: [{ id: "page", space_id: "source", title: "<script>Page</script>", content: "PRIVATE BODY NEVER RENDER", updated_at: "old", published_at: "old" }],
    drafts: [{ id: "draft", space_id: "source", document_id: "page", owner_id: "local-user", title: "Draft", content: "PRIVATE DRAFT NEVER RENDER", base_updated_at: "old", created_at: "old", updated_at: "old" }],
    property_schemas: ["map", "create", "preserve", "remove"].map(id => ({ id, space_id: "source", name: `${id} property`, property_type: "number", sort_order: 0 })).concat([{ id: "target-property", space_id: "target", name: "Destination", property_type: "number", sort_order: 0 }]),
    property_values: ["map", "create", "preserve", "remove"].map(id => ({ id: `value-${id}`, document_id: "page", property_id: id, value: "42" })),
    tags: [{ id: "tag", space_id: "source", name: "Tag", color: "blue" }], doc_tags: [{ document_id: "page", tag_id: "tag" }],
    workflow_templates: [{ id: "workflow", space_id: "source", name: "Review", stages: [{ id: "stage", name: "Open", sort_order: 0 }, { id: "done", name: "Done", sort_order: 1 }], approvers: [], created_at: "old", updated_at: "old" }],
    document_workflows: [{ id: "instance", document_id: "page", workflow_template_id: "workflow", current_stage_id: "stage", status: "active", created_at: "old", updated_at: "old" }], document_workflow_events: [],
  };
}
function decisions(): LocalReviewedMoveDecisions {
  return {
    properties: [{ source_id: "map", choice: { kind: "map", destination_id: "target-property" } }, { source_id: "create", choice: { kind: "create", destination_id: "fresh-definition" } }, { source_id: "preserve", choice: { kind: "preserve" } }, { source_id: "remove", choice: { kind: "remove" } }],
    workflows: [{ source_id: "workflow", choice: { kind: "preserve" } }], tags: [{ source_id: "tag", choice: { kind: "preserve" } }], relations: [], identifiers: [], maintenance: [], detach_hierarchy: [], preserve_previous_draft_placement: true,
  };
}
function session(source = snapshot()): LocalReviewedMoveSession {
  const preview = buildLocalMovePreview(source, intent, "local-user");
  return {
    preview, verifyCurrent: vi.fn(), relationTargets: vi.fn(() => []),
    validate: vi.fn(choices => { prepareLocalReviewedMove(source, preview, { operation_id: operation, actor_id: "local-user", moved_at, decisions: choices }); }),
    commit: vi.fn(async input => ({ canonical_committed: true as const, record: prepareLocalReviewedMove(source, preview, { ...input, actor_id: "local-user" }).record, newer_local_changes_retained: false })),
  };
}
function identity() { return { operation_id: operation, moved_at }; }

describe("explicit outgoing relation property mapping", () => {
  it("exposes only resolvable relation-target candidates and still requires complete actual edge decisions", () => {
    const source = snapshot();
    source.tags = []; source.doc_tags = []; source.workflow_templates = []; source.document_workflows = []; source.document_workflow_events = [];
    source.property_schemas = [{ id: "related", space_id: "source", name: "Related", property_type: "relation", sort_order: 0 }, { id: "destination-related", space_id: "target", name: "Destination Related", property_type: "relation", sort_order: 0 }];
    source.property_values = [{ id: "edge", document_id: "page", property_id: "related", value: "outside" }, { id: "incoming", document_id: "outside", property_id: "related", value: "page" }];
    source.documents.push({ id: "outside", space_id: "source", title: "Outside", content: "outside", published_at: "old", updated_at: "old" }, { id: "destination-page", space_id: "target", title: "Destination page", content: "target", published_at: "old", updated_at: "old" });
    const original = session(source); const preview = original.preview;
    const candidate = preview.mapping_candidates.properties[0];
    expect(candidate.compatible).toBe(false); expect(candidate.reasons).toEqual(["relation_target_unavailable_in_destination"]);
    expect(localMoveMappingCandidateAvailable(preview, "properties", candidate)).toBe(true);
    const html = renderToStaticMarkup(<LocalReviewedMoveDialog session={original} title="Page" destinationName="Destination" onClose={() => {}} onCommitted={async () => {}} />);
    expect(html).toContain('aria-label="Property action for Related"');
    const chosen = decisions(); chosen.properties = [{ source_id: "related", choice: { kind: "map", destination_id: "destination-related" } }]; chosen.tags = []; chosen.workflows = [];
    expect(localMoveReadiness(original, chosen, true)).not.toBeNull();
    chosen.relations = [{ value_id: "edge", choice: { kind: "map", document_id: "destination-page" } }, { value_id: "incoming", choice: { kind: "preserve" } }];
    expect(localMoveReadiness(original, chosen, true)).toBeNull();
    const badTarget = { ...chosen, relations: [{ value_id: "edge", choice: { kind: "map" as const, document_id: "outside" } }, chosen.relations[1]] };
    expect(localMoveReadiness(original, badTarget, true)).not.toBeNull();
    expect(prepareLocalReviewedMove(source, preview, { operation_id: operation, actor_id: "local-user", moved_at, decisions: chosen }).snapshot.property_values).toEqual([{ id: "edge", document_id: "page", property_id: "destination-related", value: "destination-page" }]);
    expect(localMoveMappingCandidateAvailable(preview, "properties", { ...candidate, reasons: [...candidate.reasons, "property_type_mismatch"] })).toBe(false);
    expect(localMoveMappingCandidateAvailable(preview, "properties", { ...candidate, reasons: ["invalid relation value"] })).toBe(false);
    expect(localMoveMappingCandidateAvailable({ ...preview, inventory: { ...preview.inventory, external_relations: [] } }, "properties", candidate)).toBe(false);
    expect(localMoveMappingCandidateAvailable(preview, "workflows", candidate)).toBe(false);
    expect(localMoveMappingCandidateAvailable({ ...preview, unavailable_participation: true }, "properties", candidate)).toBe(false);
  });
});

describe("local move dialog consumed original submission", () => {
  it("cancels before dispatch without validating, creating an operation or calling a writer", () => {
    const original = session(); const id = vi.fn(identity); const request = createLocalMoveSubmission(original, () => true, id);
    request.cancel(); expect(original.commit).not.toHaveBeenCalled(); expect(original.validate).not.toHaveBeenCalled(); expect(id).not.toHaveBeenCalled();
  });
  it("incomplete or hard-blocked choices cannot dispatch or allocate operation identity", async () => {
    const original = session(); const id = vi.fn(identity); const request = createLocalMoveSubmission(original, () => true, id);
    const incomplete = decisions(); incomplete.properties = [];
    await expect(request.submit(incomplete)).rejects.toThrow(); expect(original.commit).not.toHaveBeenCalled(); expect(id).not.toHaveBeenCalled(); expect(request.locked()).toBe(false);
    const blockedSource = snapshot(); blockedSource.documents[0].locked = true;
    const blocked = session(blockedSource);
    expect(localMoveReadiness(blocked, decisions(), true)).not.toBeNull();
  });
  it("actual dry-run requires whole confirmation and accepts map/create/preserve/remove without writes", () => {
    const original = session(); const before = JSON.stringify(original.preview);
    expect(localMoveReadiness(original, decisions(), false)).toContain("complete subtree");
    expect(localMoveReadiness(original, decisions(), true)).toBeNull();
    expect(JSON.stringify(original.preview)).toBe(before); expect(original.commit).not.toHaveBeenCalled();
    const result = prepareLocalReviewedMove(snapshot(), original.preview, { ...identity(), actor_id: "local-user", decisions: decisions() });
    expect(result.snapshot.property_values?.find(row => row.id === "value-map")?.property_id).toBe("target-property");
    expect(result.snapshot.property_values?.find(row => row.id === "value-create")?.property_id).toBe("fresh-definition");
    expect(result.record.preserved.some(row => row.collection === "property_values")).toBe(true); expect(result.record.removed.some(row => row.collection === "property_values")).toBe(true);
  });
  it("duplicate submission while the first awaits invokes the writer exactly once", async () => {
    const original = session(); let release!: () => void; const hold = new Promise<void>(resolve => { release = resolve; }); const actual = original.commit;
    original.commit = vi.fn(async input => { await hold; return actual(input); });
    const id = vi.fn(identity); const request = createLocalMoveSubmission(original, () => true, id);
    const first = request.submit(decisions()); await expect(request.submit(decisions())).resolves.toBeUndefined();
    expect(original.commit).toHaveBeenCalledTimes(1); release(); await first; expect(id).toHaveBeenCalledTimes(1);
  });
  it("unknown outcome retains exact original operation and decisions despite edits, then confirmed refresh never redispatches", async () => {
    const original = session(); const actual = original.commit; const calls: Parameters<typeof actual>[0][] = []; let attempt = 0;
    original.commit = vi.fn(async input => { calls.push(JSON.parse(JSON.stringify(input))); input.decisions.tags = []; if (++attempt === 1) throw new Error("Unconfirmed commit"); return actual(calls[1]); });
    const id = vi.fn(identity); const request = createLocalMoveSubmission(original, () => true, id); const mutable = decisions();
    await expect(request.submit(mutable)).rejects.toThrow("Unconfirmed"); expect(() => request.cancel()).toThrow("cannot undo");
    mutable.properties = []; await request.submit(mutable); await request.submit(mutable);
    expect(calls[1]).toEqual(calls[0]); expect(original.commit).toHaveBeenCalledTimes(2); expect(id).toHaveBeenCalledTimes(1); expect(request.confirmed()).toBe(true);
  });
  it("changed original selection prevents dispatch and late change cannot acknowledge a held result", async () => {
    const original = session(); let current = false; const request = createLocalMoveSubmission(original, () => current, identity);
    await expect(request.submit(decisions())).rejects.toThrow("selection changed"); expect(original.commit).not.toHaveBeenCalled();
    current = true; let release!: () => void; const hold = new Promise<void>(resolve => { release = resolve; }); const actual = original.commit;
    original.commit = vi.fn(async input => { await hold; return actual(input); });
    const first = request.submit(decisions()); current = false; release();
    await expect(first).rejects.toThrow("selection changed"); expect(request.confirmed()).toBe(false); expect(request.locked()).toBe(true);
  });
  it.each([false, true])("actual submission accepts sorted native ACK only with complete original decisions, tamper=%s", async tamper => {
    const original = session(); const actual = original.commit;
    const sorted = (value: unknown): unknown => Array.isArray(value) ? value.map(sorted) : value && typeof value === "object" ? Object.fromEntries(Object.keys(value).sort().map(key => [key, sorted((value as Record<string, unknown>)[key])])) : value;
    original.commit = vi.fn(async input => {
      const result = await actual(input);
      const record = sorted(result.record) as typeof result.record;
      if (tamper) Object.assign(record.decisions, { unexpected_native_field: "changed intent" });
      return { ...result, record };
    });
    const id = vi.fn(identity); const submission = createLocalMoveSubmission(original, () => true, id);
    if (tamper) {
      await expect(submission.submit(decisions())).rejects.toThrow("unconfirmed");
      expect(submission.locked()).toBe(true); expect(submission.confirmed()).toBe(false);
    } else {
      const result = await submission.submit(decisions());
      expect(result?.canonical_committed).toBe(true); expect(submission.confirmed()).toBe(true);
      expect(await submission.submit(decisions())).toEqual(result);
      expect(original.commit).toHaveBeenCalledTimes(1);
    }
    expect(id).toHaveBeenCalledTimes(1);
  });
  it("malformed acknowledgment cannot clear the original request", async () => {
    const original = session(); const actual = original.commit;
    original.commit = vi.fn(async input => { const result = await actual(input); return { ...result, record: { ...result.record, id: "foreign-operation" } }; });
    const request = createLocalMoveSubmission(original, () => true, identity);
    await expect(request.submit(decisions())).rejects.toThrow("unconfirmed"); expect(request.locked()).toBe(true); expect(request.confirmed()).toBe(false);
  });
});

describe("local move dialog presentation", () => {
  it("uses named shared dialog controls, exposes deliberate choices and never renders body/signature/evidence JSON", () => {
    const original = session(); const html = renderToStaticMarkup(<LocalReviewedMoveDialog session={original} title="<script>Root</script>" destinationName="Target" onClose={vi.fn()} onCommitted={vi.fn()} />);
    expect(html).toContain('role="dialog"'); expect(html).toContain('aria-labelledby="local-move-title"'); expect(html).toContain('aria-label="Property action for map property"'); expect(html).toContain('aria-label="Workflow action for Review"'); expect(html).toContain('aria-label="Tag action for Tag"'); expect(html).toContain("Choose a disposition"); expect(html).toContain('data-starlane-select-control="true"'); expect(html).toContain("complete subtree");
    expect(html).toContain("&lt;script&gt;"); expect(html).not.toContain("PRIVATE BODY"); expect(html).not.toContain("PRIVATE DRAFT"); expect(html).not.toContain("internal_inventory_signature"); expect(html).not.toContain("<script>"); expect(original.commit).not.toHaveBeenCalled();
  });
  it("projects both relation directions only from session targets and exposes hierarchy/draft placement warnings", () => {
    const original = session(); original.preview.inventory.external_relations = [
      { direction: "incoming", value: { id: "incoming", document_id: "outside", property_id: "relation", value: "page" } },
      { direction: "outgoing", value: { id: "outgoing", document_id: "page", property_id: "relation", value: "outside" } },
    ];
    original.preview.inventory.external_hierarchy = [{ kind: "document", id: "page", boundary: "parent_outside_closure" }]; original.preview.warnings.push("draft_previous_placement_requires_review");
    const html = renderToStaticMarkup(<LocalReviewedMoveDialog session={original} title="Root" destinationName="Target" onClose={vi.fn()} onCommitted={vi.fn()} />);
    expect(original.relationTargets).toHaveBeenCalledWith("incoming"); expect(original.relationTargets).toHaveBeenCalledWith("outgoing"); expect(html).toContain("Incoming relationship"); expect(html).toContain("Outgoing relationship"); expect(html).toContain("No proven readable mapping target"); expect(html).toContain("previous draft placement"); expect(html).toContain("Every descendant remains");
  });
  it("shows generic identifier conflicts and catalog-free maintenance without exposing destination IDs or hidden target labels", () => {
    const original = session(); original.preview.inventory.identifiers[0].resolution_required = true;
    original.preview.inventory.destination_identifier_rows = [{ id: "HIDDEN_DESTINATION", human_identifier: "HIDDEN_ID" }];
    original.preview.inventory.maintenance[0].maintenance_user_id = "HIDDEN_PERSON"; original.preview.inventory.maintenance[0].resolution_required = true;
    const html = renderToStaticMarkup(<LocalReviewedMoveDialog session={original} title="Root" destinationName="Target" onClose={vi.fn()} onCommitted={vi.fn()} />);
    expect(html).toContain("Conflicts in the destination"); expect(html).toContain("Assigned person"); expect(html).not.toContain("HIDDEN_DESTINATION"); expect(html).not.toContain("HIDDEN_ID"); expect(html).not.toContain("HIDDEN_PERSON"); expect(html).not.toContain("Assign person");
  });
});


describe("local move consumed nested-menu ownership", () => {
  it("keeps dialog Escape owned until every keyed menu closes, including disabled or removed menu teardown", () => {
    let open: ReadonlySet<string> = new Set();
    open = updateLocalMoveMenuOwnership(open, "properties:source:action", true);
    open = updateLocalMoveMenuOwnership(open, "relation:edge:target", true);
    expect(open.size === 0).toBe(false);
    // First Escape closes the active inner menu; a second still-open menu retains ownership.
    open = updateLocalMoveMenuOwnership(open, "relation:edge:target", false);
    expect([...open]).toEqual(["properties:source:action"]);
    expect(open.size === 0).toBe(false);
    // MenuSelect invokes this same false callback on disabled/unmounted conditional menus.
    open = updateLocalMoveMenuOwnership(open, "properties:source:action", false);
    expect(open.size === 0).toBe(true);
    open = updateLocalMoveMenuOwnership(open, "relation:edge:target", false);
    expect(open.size === 0).toBe(true);
  });
});
