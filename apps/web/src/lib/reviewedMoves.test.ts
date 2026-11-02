import { describe, expect, it } from "vitest";
import type { LocalVaultSnapshot } from "./localVault";
import {
  resolveLocalMoveIdentifiers,
  buildLocalMovePreview,
  type LocalMovePreviewIntent,
} from "./reviewedMoves";

const intent: LocalMovePreviewIntent = {
  source_space_id: "source",
  root: { kind: "document", id: "page" },
  destination: { space_id: "target", folder_id: null, parent_id: null },
};
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
function fixture(): LocalVaultSnapshot {
  return {
    schema_version: 1,
    templates: [],
    spaces: ["source", "target"].map((id) => ({
      id,
      name: id,
      created_at: "old",
      updated_at: "old",
    })),
    folders: [
      { id: "folder", space_id: "source", name: "Folder" },
      { id: "target-folder", space_id: "target", name: "Target" },
    ],
    documents: [
      {
        id: "page",
        space_id: "source",
        title: "Page",
        content: "body starlane://asset/asset",
        updated_at: "old",
        published_at: "old",
      },
      {
        id: "child",
        space_id: "source",
        parent_id: "page",
        title: "Child",
        content: "child body",
        updated_at: "old",
        published_at: "old",
      },
      {
        id: "outside",
        space_id: "source",
        title: "Outside",
        content: "outside",
        updated_at: "old",
        published_at: "old",
      },
      {
        id: "target-page",
        space_id: "target",
        folder_id: "target-folder",
        title: "Target page",
        content: "target",
        updated_at: "old",
        published_at: "old",
      },
    ],
    drafts: [
      {
        id: "draft",
        document_id: "page",
        space_id: "source",
        owner_id: "local-user",
        title: "Draft",
        content: "draft body",
        base_updated_at: "old",
        created_at: "old",
        updated_at: "old",
      },
    ],
    property_schemas: [
      {
        id: "choice",
        space_id: "source",
        name: "Choice",
        property_type: "select",
        options: ["A", "B"],
        sort_order: 0,
      },
      {
        id: "target-choice",
        space_id: "target",
        name: "Different name",
        property_type: "select",
        options: { options: ["A", "B"] },
        sort_order: 0,
      },
      {
        id: "relation",
        space_id: "source",
        name: "Related",
        property_type: "relation",
        sort_order: 1,
      },
    ],
    property_values: [
      {
        id: "value",
        document_id: "child",
        property_id: "choice",
        value: "A",
        created_at: "old",
        updated_at: "old",
      },
    ],
    tags: [
      { id: "tag", space_id: "source", name: "Tag", color: "blue" },
      { id: "target-tag", space_id: "target", name: "Tag", color: "blue" },
    ],
    doc_tags: [{ document_id: "page", tag_id: "tag" }],
    workflow_templates: ["source", "target"].map((space_id) => ({
      id: `${space_id}-workflow`,
      space_id,
      name: "Workflow",
      created_at: "old",
      updated_at: "old",
      stages: [
        { id: "start", name: "Start", sort_order: 0 },
        { id: "done", name: "Done", sort_order: 1 },
      ],
      approvers: [
        {
          id: `${space_id}-approver`,
          approver_type: "user",
          approver_id: "local-user",
          stage_id: "done",
        },
      ],
    })),
    document_workflows: [
      {
        id: "workflow",
        document_id: "page",
        workflow_template_id: "source-workflow",
        current_stage_id: "done",
        status: "completed",
        created_at: "old",
        updated_at: "old",
      },
    ],
    document_workflow_events: [
      {
        id: "event",
        document_workflow_id: "workflow",
        document_id: "page",
        from_stage_id: "start",
        to_stage_id: "done",
        action: "transition",
        note: "Keep history",
        created_at: "old",
      },
    ],
    assets: [
      {
        id: "asset",
        name: "file.png",
        size: 4,
        url: "asset://file",
        source: "tauri",
        vault_path: "/vault/source/file.png",
        space_id: "source",
        document_id: "page",
        created_at: "old",
      },
    ],
    search_index: [
      {
        document_id: "page",
        space_id: "source",
        title: "Page",
        content: "body",
        search_text: "body",
        updated_at: "old",
      },
    ],
    sync_state: { state: "idle", updated_at: "old", queued_operations: [] },
  };
}

describe("pure local reviewed move inventory", () => {
  it("inventories raw metadata and history without changing inputs or granting commit", () => {
    const snapshot = fixture();
    const before = clone(snapshot);
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(preview.inventory.documents.map((item) => item.id).sort()).toEqual(["child", "page"]);
    expect(preview.inventory.drafts[0].content).toBe("draft body");
    expect(preview.inventory.property_values[0].value).toBe("A");
    expect(preview.inventory.document_workflow_events[0].note).toBe("Keep history");
    expect(preview.mapping_candidates.properties[0].compatible).toBe(true);
    expect(preview.mapping_candidates.workflows[0].compatible).toBe(true);
    expect(preview.commit_available).toBe(false);
    expect(preview.warnings).toContain("asset_path_retained_file_relocation_requires_journal");
    expect(snapshot).toEqual(before);
    preview.inventory.documents[0].title = "Changed returned inventory";
    expect(snapshot).toEqual(before);
  });

  it("produces exact deterministic signatures regardless of collection/object ordering", () => {
    const snapshot = fixture();
    const permuted = clone(snapshot);
    for (const key of Object.keys(permuted) as (keyof LocalVaultSnapshot)[]) {
      const value = permuted[key];
      if (Array.isArray(value)) value.reverse();
    }
    for (const template of permuted.workflow_templates ?? []) {
      template.stages.reverse();
      template.approvers.reverse();
    }
    permuted.documents = permuted.documents.map(
      (doc) => Object.fromEntries(Object.entries(doc).reverse()) as typeof doc,
    );
    const first = buildLocalMovePreview(snapshot, intent);
    const second = buildLocalMovePreview(permuted, intent);
    expect(second).toEqual(first);
    expect(JSON.parse(first.internal_inventory_signature).inventory.property_values[0].value).toBe(
      "A",
    );
  });

  it.each(["private", "archived", "deleted"])("conceals unavailable %s roots", (kind) => {
    const snapshot = fixture();
    const root = snapshot.documents[0];
    if (kind === "private") {
      root.published_at = undefined;
      root.created_by = "foreign";
      snapshot.drafts![0].owner_id = "foreign";
    } else if (kind === "archived") root.archived_at = "old";
    else root.deleted_at = "old";
    expect(() => buildLocalMovePreview(snapshot, intent)).toThrow("Move preview unavailable");
  });

  it("does not expose foreign participating draft content and keeps a nonwaivable blocker", () => {
    const snapshot = fixture();
    snapshot.drafts![0].owner_id = "foreign";
    snapshot.drafts![0].content = "PRIVATE-DRAFT-SECRET";
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(preview.unavailable_participation).toBe(true);
    expect(preview.blockers).toContain("participating_draft_unavailable");
    expect(JSON.stringify(preview)).not.toContain("PRIVATE-DRAFT-SECRET");
    expect(preview.inventory.drafts).toEqual([]);
    expect(() => buildLocalMovePreview(snapshot, intent, "foreign")).not.toThrow();
  });

  it("conceals private descendants and foreign history records", () => {
    const snapshot = fixture();
    snapshot.documents[1].published_at = undefined;
    snapshot.documents[1].created_by = "foreign";
    snapshot.documents[1].content = "PRIVATE-PAGE-SECRET";
    snapshot.document_workflow_events!.push({
      ...snapshot.document_workflow_events![0],
      id: "foreign-event",
      document_id: "outside",
      note: "PRIVATE-HISTORY-SECRET",
    });
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(preview.unavailable_participation).toBe(true);
    expect(preview.blockers).toContain("participating_page_unavailable");
    expect(preview.blockers).toContain("invalid_workflow_history_binding");
    expect(JSON.stringify(preview)).not.toContain("PRIVATE-PAGE-SECRET");
    expect(JSON.stringify(preview)).not.toContain("PRIVATE-HISTORY-SECRET");
  });

  it("takes folder and page transitive closure and blocks escaping folder descendants", () => {
    const snapshot = fixture();
    snapshot.documents[0].folder_id = "folder";
    snapshot.documents[1].folder_id = "other-folder";
    snapshot.folders.push({
      id: "other-folder",
      name: "Other",
      space_id: "source",
    });
    const preview = buildLocalMovePreview(snapshot, {
      ...intent,
      root: { kind: "folder", id: "folder" },
    });
    expect(preview.inventory.documents.map((item) => item.id).sort()).toEqual(["child", "page"]);
    expect(preview.blockers).toContain("page_descendant_outside_folder_closure");
    expect(preview.blockers).toContain("invalid_document_parent_folder");
  });

  it("blocks cyclic source hierarchies and cross-space children", () => {
    const snapshot = fixture();
    snapshot.documents[0].parent_id = "child";
    snapshot.documents.push({
      ...snapshot.documents[1],
      id: "foreign-child",
      space_id: "target",
    });
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(preview.blockers).toContain("cyclic_hierarchy");
    expect(preview.blockers).toContain("cross_space_hierarchy_edge");
  });

  it("checks destination scope, root containment and folder alignment beyond the old depth ceiling", () => {
    const snapshot = fixture();
    expect(
      buildLocalMovePreview(snapshot, {
        ...intent,
        destination: {
          space_id: "target",
          folder_id: "folder",
          parent_id: null,
        },
      }).blockers,
    ).toContain("destination_placement_unavailable");
    expect(
      buildLocalMovePreview(snapshot, {
        ...intent,
        destination: {
          space_id: "source",
          folder_id: null,
          parent_id: "child",
        },
      }).blockers,
    ).toContain("destination_inside_closure");
    expect(
      buildLocalMovePreview(snapshot, {
        ...intent,
        destination: {
          space_id: "target",
          folder_id: null,
          parent_id: "target-page",
        },
      }).blockers,
    ).toContain("destination_parent_folder_mismatch");
    snapshot.documents.push({
      ...snapshot.documents[3],
      id: "target-child",
      parent_id: "target-page",
    });
    expect(
      buildLocalMovePreview(snapshot, {
        ...intent,
        destination: {
          space_id: "target",
          folder_id: "target-folder",
          parent_id: "target-child",
        },
      }).blockers,
    ).not.toContain("destination_depth_exceeded");
  });

  it("blocks locked source pages and destination ancestors without inventing a locker exception", () => {
    const snapshot = fixture();
    snapshot.documents[1].locked = true;
    expect(buildLocalMovePreview(snapshot, intent).blockers).toContain("locked_items");
    snapshot.documents[1].locked = false;
    snapshot.documents[3].locked = true;
    expect(
      buildLocalMovePreview(snapshot, {
        ...intent,
        destination: {
          space_id: "target",
          folder_id: "target-folder",
          parent_id: "target-page",
        },
      }).blockers,
    ).toContain("destination_locked");
  });

  it("accepts page and folder hierarchies beyond three levels", () => {
    const snapshot = fixture();
    snapshot.documents.push({
      ...snapshot.documents[1],
      id: "grandchild",
      parent_id: "child",
    });
    expect(buildLocalMovePreview(snapshot, intent).blockers).not.toContain(
      "destination_depth_exceeded",
    );
    snapshot.documents.push({
      ...snapshot.documents[1],
      id: "too-deep",
      parent_id: "grandchild",
    });
    expect(buildLocalMovePreview(snapshot, intent).blockers).not.toContain(
      "destination_depth_exceeded",
    );
    snapshot.documents = snapshot.documents.filter(row => row.id !== "too-deep");
    snapshot.folders.push(
      {
        id: "folder-child",
        name: "Child",
        space_id: "source",
        parent_id: "folder",
      },
      {
        id: "folder-grandchild",
        name: "Grandchild",
        space_id: "source",
        parent_id: "folder-child",
      },
      {
        id: "folder-too-deep",
        name: "Too deep",
        space_id: "source",
        parent_id: "folder-grandchild",
      },
    );
    expect(
      buildLocalMovePreview(snapshot, {
        ...intent,
        root: { kind: "folder", id: "folder" },
      }).blockers,
    ).not.toContain("destination_depth_exceeded");
  });

  it.each([4, 32, 256])("accepts %i-level folder source and destination with stable raw inventory", depth => {
    const snapshot = fixture();
    snapshot.folders = ["source", "target"].flatMap(space_id => Array.from({ length: depth }, (_, n) => ({ id: `${space_id}-${n}`, name: `Original ${n}`, space_id, parent_id: n ? `${space_id}-${n - 1}` : undefined })));
    const before = JSON.stringify(snapshot);
    const preview = buildLocalMovePreview(snapshot, { source_space_id: "source", root: { kind: "folder", id: "source-0" }, destination: { space_id: "target", folder_id: null, parent_id: `target-${depth - 1}` } });
    expect(preview.blockers).not.toContain("destination_depth_exceeded");
    expect(preview.inventory.folders.map(row => row.id).sort()).toEqual(snapshot.folders.filter(row => row.space_id === "source").map(row => row.id).sort());
    expect(preview.inventory.source_ancestors.folders.every(row => row.space_id === "source")).toBe(true);
    expect(JSON.stringify(snapshot)).toBe(before);
  });

  it.each([4, 32, 256])("retains %i-level source and destination page identity in reviewed inventory", depth => {
    const snapshot = fixture(), source = snapshot.documents[0];
    snapshot.documents = ["source", "target"].flatMap(space_id => Array.from({ length: depth }, (_, n) => ({ ...source, id: `${space_id}-page-${n}`, title: `${space_id} ${n}`, space_id, parent_id: n ? `${space_id}-page-${n - 1}` : undefined })));
    snapshot.sync_state!.queued_operations = [];
    const before = JSON.stringify(snapshot);
    const preview = buildLocalMovePreview(snapshot, { source_space_id: "source", root: { kind: "document", id: "source-page-0" }, destination: { space_id: "target", folder_id: null, parent_id: `target-page-${depth - 1}` } });
    expect(preview.blockers).not.toContain("destination_depth_exceeded");
    expect(preview.inventory.documents).toHaveLength(depth);
    expect(preview.inventory.documents.find(page => page.id === `source-page-${depth - 1}`)?.parent_id).toBe(`source-page-${depth - 2}`);
    expect(JSON.stringify(snapshot)).toBe(before);
  });

  it("rejects name-only property matches and validates options and raw values", () => {
    const snapshot = fixture();
    snapshot.property_schemas![1].options = ["B"];
    let candidate = buildLocalMovePreview(snapshot, intent).mapping_candidates.properties[0];
    expect(candidate.compatible).toBe(false);
    expect(candidate.reasons.join()).toContain("does not match select");
    snapshot.property_schemas![1].property_type = "text";
    candidate = buildLocalMovePreview(snapshot, intent).mapping_candidates.properties[0];
    expect(candidate.reasons).toContain("property_type_mismatch");
    snapshot.property_values![0].value = "bad";
    expect(buildLocalMovePreview(snapshot, intent).blockers).toContain("invalid_property_value");
  });

  it("detects incoming and outgoing relations and incompatible destination target scope", () => {
    const snapshot = fixture();
    snapshot.property_values!.push(
      {
        id: "outgoing",
        document_id: "page",
        property_id: "relation",
        value: "outside",
      },
      {
        id: "incoming",
        document_id: "outside",
        property_id: "relation",
        value: "child",
      },
    );
    snapshot.property_schemas!.push({
      id: "target-relation",
      space_id: "target",
      name: "Related",
      property_type: "relation",
      sort_order: 1,
    });
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(preview.inventory.external_relations.map((item) => item.direction).sort()).toEqual([
      "incoming",
      "outgoing",
    ]);
    expect(preview.blockers).toContain("external_relations_require_resolution");
    expect(
      preview.mapping_candidates.properties.find(
        (item) => item.source_id === "relation" && item.destination_id === "target-relation",
      )?.reasons,
    ).toContain("relation_target_unavailable_in_destination");
  });

  it("requires all workflow history stages and verified approver identities", () => {
    const snapshot = fixture();
    snapshot.workflow_templates![1].stages[0].id = "renamed-start";
    snapshot.workflow_templates![1].approvers[0].approver_type = "role";
    snapshot.workflow_templates![1].approvers[0].approver_id = "admin";
    const candidate = buildLocalMovePreview(snapshot, intent).mapping_candidates.workflows[0];
    expect(candidate.compatible).toBe(false);
    expect(candidate.reasons).toContain("workflow_history_stage_unavailable");
    expect(candidate.reasons).toContain("workflow_stage_identity_mismatch");
    expect(candidate.reasons).toContain("approver_identity_unavailable");
    snapshot.document_workflow_events![0].from_stage_id = "missing";
    expect(buildLocalMovePreview(snapshot, intent).blockers).toContain(
      "workflow_metadata_requires_resolution",
    );
  });

  it("binds external source ancestors and incoming relation definitions in the signature", () => {
    const snapshot = fixture();
    snapshot.documents[0].folder_id = "folder";
    snapshot.documents[1].folder_id = "folder";
    snapshot.property_values!.push({
      id: "incoming",
      document_id: "outside",
      property_id: "relation",
      value: "page",
    });
    const before = buildLocalMovePreview(snapshot, intent);
    expect(before.inventory.source_ancestors.folders[0].id).toBe("folder");
    expect(before.inventory.relation_schemas[0].id).toBe("relation");
    snapshot.folders[0].name = "Changed ancestor";
    expect(buildLocalMovePreview(snapshot, intent).internal_inventory_signature).not.toBe(
      before.internal_inventory_signature,
    );
    const next = buildLocalMovePreview(snapshot, intent).internal_inventory_signature;
    snapshot.property_schemas![2].name = "Changed external relation definition";
    expect(buildLocalMovePreview(snapshot, intent).internal_inventory_signature).not.toBe(next);
  });

  it("reports shared stable assets and source-qualified links without rewriting bytes or paths", () => {
    const snapshot = fixture();
    snapshot.documents[0].content += " /spaces/source/documents/page";
    snapshot.documents[2].content += " starlane://asset/asset";
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(preview.inventory.assets[0].vault_path).toBe("/vault/source/file.png");
    expect(preview.inventory.asset_references).toContainEqual({
      asset_id: "asset",
      document_id: "outside",
    });
    expect(preview.warnings).toContain("shared_asset_reference_retained");
    expect(preview.warnings).toContain("source_space_qualified_content_links_retained");
    snapshot.drafts!.push({
      ...snapshot.drafts![0],
      id: "hidden-draft",
      document_id: "outside",
      owner_id: "foreign",
      content: "PRIVATE starlane://asset/asset",
    });
    const hidden = buildLocalMovePreview(snapshot, intent);
    expect(hidden.unavailable_participation).toBe(true);
    expect(hidden.blockers).toContain("asset_reference_unavailable");
    expect(JSON.stringify(hidden)).not.toContain("hidden-draft");
    expect(JSON.stringify(hidden)).not.toContain("PRIVATE");
  });

  it("blocks folder cycles, foreign ancestry, and malformed property identities", () => {
    const snapshot = fixture();
    snapshot.folders.push({
      id: "folder-child",
      name: "Child",
      space_id: "source",
      parent_id: "folder",
    });
    snapshot.folders[0].parent_id = "folder-child";
    expect(
      buildLocalMovePreview(snapshot, {
        ...intent,
        root: { kind: "folder", id: "folder" },
      }).blockers,
    ).toContain("cyclic_hierarchy");
    snapshot.folders[0].parent_id = "target-folder";
    expect(
      buildLocalMovePreview(snapshot, {
        ...intent,
        root: { kind: "folder", id: "folder" },
      }).blockers,
    ).toContain("invalid_hierarchy_edge");
    snapshot.property_values!.push({
      id: "missing-schema",
      document_id: "page",
      property_id: "missing",
      value: "raw",
    });
    expect(buildLocalMovePreview(snapshot, intent).blockers).toContain("invalid_property_binding");
  });

  it("blocks affected queued writes but ignores unrelated work", () => {
    const snapshot = fixture();
    snapshot.sync_state!.queued_operations.push(
      {
        id: "unrelated",
        entity_type: "document",
        entity_id: "outside",
        action: "update",
        queued_at: "old",
        payload: { title: "unrelated" },
      },
      {
        id: "related",
        entity_type: "workflow_event",
        entity_id: "new-event",
        action: "create",
        queued_at: "old",
        payload: { document_id: "page" },
      },
    );
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(preview.inventory.queued_operations.map((item) => item.id)).toEqual(["related"]);
    expect(preview.blockers).toContain("pending_affected_writes_require_resolution");
  });

  it("keeps private queued payloads out of renderable summaries and nonwaivably blocks participation", () => {
    const snapshot = fixture();
    snapshot.drafts![0].owner_id = "foreign";
    snapshot.sync_state!.queued_operations.push({
      id: "q",
      entity_type: "draft",
      entity_id: "draft",
      action: "update",
      queued_at: "old",
      payload: {
        document: { id: "page" },
        draft_id: "draft",
        content: "PRIVATE-QUEUED-SECRET",
      },
    });
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(preview.inventory.queued_operations).toEqual([
      { id: "q", entity_type: "draft", action: "update", queued_at: "old" },
    ]);
    expect(preview.blockers).toContain("queued_participation_unavailable");
    expect(preview.unavailable_participation).toBe(true);
    expect(JSON.stringify(preview)).not.toContain("PRIVATE-QUEUED-SECRET");
  });

  it.each([
    "document",
    "draft",
    "folder",
    "schema",
    "value",
    "tag",
    "tag_join",
    "workflow",
    "history",
    "template",
    "approver",
    "asset",
    "search",
    "destination",
    "queue",
    "incoming",
  ])("changes exact signature when materially affected %s state changes", (family) => {
    const snapshot = fixture();
    snapshot.documents[0].folder_id = "folder";
    const folderIntent = {
      ...intent,
      root: { kind: "folder" as const, id: "folder" },
    };
    const before = buildLocalMovePreview(snapshot, folderIntent).internal_inventory_signature;
    switch (family) {
      case "document":
        snapshot.documents[0].content += "changed";
        break;
      case "draft":
        snapshot.drafts![0].content += "changed";
        break;
      case "folder":
        snapshot.folders[0].name += "changed";
        break;
      case "schema":
        snapshot.property_schemas![0].options = ["A"];
        break;
      case "value":
        snapshot.property_values![0].value = "B";
        break;
      case "tag":
        snapshot.tags![0].name += "changed";
        break;
      case "tag_join":
        snapshot.doc_tags![0].document_id = "child";
        break;
      case "workflow":
        snapshot.document_workflows![0].status = "active";
        break;
      case "history":
        snapshot.document_workflow_events![0].note += "changed";
        break;
      case "template":
        snapshot.workflow_templates![0].stages[0].name += "changed";
        break;
      case "approver":
        snapshot.workflow_templates![0].approvers[0].approver_id = "foreign";
        break;
      case "asset":
        snapshot.assets[0].vault_path += "changed";
        break;
      case "search":
        snapshot.search_index![0].content += "changed";
        break;
      case "destination":
        snapshot.property_schemas![1].options = ["B"];
        break;
      case "queue":
        snapshot.sync_state!.queued_operations.push({
          id: "q",
          entity_type: "folder",
          entity_id: "folder",
          action: "update",
          queued_at: "now",
        });
        break;
      case "incoming":
        snapshot.property_values!.push({
          id: "incoming",
          document_id: "outside",
          property_id: "relation",
          value: "page",
        });
        break;
    }
    expect(buildLocalMovePreview(snapshot, folderIntent).internal_inventory_signature).not.toBe(
      before,
    );
  });
});

describe("identifier move continuity", () => {
  it("preserves unique identifiers and makes archive/trash destination collisions explicit", () => {
    const snapshot = fixture();
    snapshot.documents[0].human_identifier = "Plan-1";
    snapshot.documents.push({
      id: "archived-collision",
      space_id: "target",
      title: "Not rendered",
      content: "",
      human_identifier: "plan-1",
      archived_at: "old",
      updated_at: "old",
    });
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(
      preview.inventory.identifiers.find((p) => p.document_id === "page")?.resolution_required,
    ).toBe(true);
    expect(() => resolveLocalMoveIdentifiers(preview)).toThrow("explicitly");
    expect(
      resolveLocalMoveIdentifiers(preview, [
        { document_id: "page", choice: { kind: "rename", value: "New-1" } },
      ]).find((p) => p.document_id === "page")?.human_identifier,
    ).toBe("New-1");
    expect(
      resolveLocalMoveIdentifiers(preview, [
        { document_id: "page", choice: { kind: "clear" } },
      ]).find((p) => p.document_id === "page")?.human_identifier,
    ).toBeNull();
    expect(() =>
      resolveLocalMoveIdentifiers(preview, [
        { document_id: "page", choice: { kind: "rename", value: "PLAN-1" } },
      ]),
    ).toThrow("conflicts");
    const original = preview.internal_inventory_signature;
    snapshot.documents[snapshot.documents.length - 1].human_identifier = "Other-1";
    expect(buildLocalMovePreview(snapshot, intent).internal_inventory_signature).not.toBe(original);
    expect(snapshot.documents[0].human_identifier).toBe("Plan-1");
  });
  it("rejects unknown decisions and duplicate renamed subtree keys without mutating IDs", () => {
    const preview = buildLocalMovePreview(fixture(), intent);
    expect(() =>
      resolveLocalMoveIdentifiers(preview, [{ document_id: "missing", choice: { kind: "clear" } }]),
    ).toThrow("Invalid");
    expect(() =>
      resolveLocalMoveIdentifiers(preview, [
        { document_id: "page", choice: { kind: "rename", value: "One" } },
        { document_id: "child", choice: { kind: "rename", value: "one" } },
      ]),
    ).toThrow("conflicts");
    expect(preview.commit_available).toBe(false);
  });
});

describe("local reviewed maintenance continuity", () => {
  it("binds real account catalog and raw XOR pair in freshness; requires explicit map/clear for unavailable targets", async () => {
    const { resolveLocalMoveMaintenance } = await import("./reviewedMoves");
    const snapshot = fixture() as LocalVaultSnapshot & { maintenance_target_catalogs?: unknown };
    Object.assign(snapshot.documents.find((p) => p.id === "page")!, {
      maintenance_user_id: null,
      maintenance_group_id: "source-team",
    });
    snapshot.maintenance_target_catalogs = [
      {
        server_base: "local",
        account_id: "local-user",
        space_id: "target",
        targets: {
          users: [{ id: "person", name: "Current person", active: true }],
          groups: [{ id: "team", name: "Current team", managed_space_id: "target" }],
          groups_available: true,
        },
      },
    ];
    const preview = buildLocalMovePreview(snapshot, intent);
    expect(
      preview.inventory.maintenance.find((p) => p.document_id === "page")?.resolution_required,
    ).toBe(true);
    expect(() => resolveLocalMoveMaintenance(preview)).toThrow("Map or clear");
    for (const choice of [
      { kind: "clear" } as const,
      { kind: "user", user_id: "person" } as const,
      { kind: "group", group_id: "team" } as const,
    ]) {
      const resolved = resolveLocalMoveMaintenance(preview, [{ document_id: "page", choice }]);
      expect(resolved.find((p) => p.document_id === "page")).toEqual({
        document_id: "page",
        maintenance_user_id: choice.kind === "user" ? "person" : null,
        maintenance_group_id: choice.kind === "group" ? "team" : null,
      });
    }
    expect(() =>
      resolveLocalMoveMaintenance(preview, [
        { document_id: "page", choice: { kind: "user", user_id: "unknown" } },
      ]),
    ).toThrow("catalog person");
    const changed = clone(snapshot);
    Object.assign(changed.documents.find((p) => p.id === "page")!, { maintenance_group_id: null });
    expect(buildLocalMovePreview(changed, intent).internal_inventory_signature).not.toEqual(
      preview.internal_inventory_signature,
    );
    const other = clone(snapshot);
    (other.maintenance_target_catalogs as { account_id: string }[])[0].account_id = "other";
    expect(
      buildLocalMovePreview(other, intent).inventory.destination_maintenance_targets,
    ).toBeNull();
    const cleared = buildLocalMovePreview(changed, intent);
    expect(
      resolveLocalMoveMaintenance(cleared).find((p) => p.document_id === "page")
        ?.maintenance_group_id,
    ).toBeNull();
  });
});


import { prepareLocalReviewedMove, type LocalReviewedMoveDecisions } from "./localReviewedMove";
const moveDecisions = (): LocalReviewedMoveDecisions => ({
  properties: [{ source_id: "choice", choice: { kind: "map", destination_id: "target-choice" } }],
  workflows: [{ source_id: "source-workflow", choice: { kind: "map", destination_id: "target-workflow" } }],
  tags: [{ source_id: "tag", choice: { kind: "map", destination_id: "target-tag" } }],
  relations: [], identifiers: [], maintenance: [], detach_hierarchy: [], preserve_previous_draft_placement: true,
});
const moveInput = (decisions = moveDecisions()) => ({ operation_id: "12345678-1234-1234-1234-123456789abc", actor_id: "local-user", moved_at: "2026-10-09T12:00:00Z", decisions });
describe("standalone reviewed subtree transform preparation", () => {
  it("maps the complete subtree while retaining IDs, exact bodies, drafts, events, asset paths and raw evidence without mutation", () => {
    const source = fixture(); const before = clone(source); const preview = buildLocalMovePreview(source, intent);
    const { snapshot, record } = prepareLocalReviewedMove(source, preview, moveInput());
    expect(source).toEqual(before); expect(preview.commit_available).toBe(false);
    expect(snapshot.documents.filter(row => ["page", "child"].includes(row.id)).map(row => row.space_id)).toEqual(["target", "target"]);
    expect(snapshot.documents.find(row => row.id === "page")!.content).toBe(before.documents[0].content);
    expect(snapshot.drafts![0]).toEqual({ ...before.drafts![0], space_id: "target" });
    expect(snapshot.property_values![0]).toEqual({ ...before.property_values![0], property_id: "target-choice" });
    expect(snapshot.document_workflows![0].workflow_template_id).toBe("target-workflow");
    expect(snapshot.document_workflow_events).toEqual(before.document_workflow_events); expect(snapshot.assets).toEqual(before.assets.map(row => ({ ...row, space_id: "target" })));
    expect(record.before.assets).toEqual(before.assets);
    expect(record.before.documents).toEqual(before.documents.filter(row => ["page", "child"].includes(row.id)));
    expect(snapshot.reviewed_move_records).toEqual([record]);
  });
  it("moves document-owned asset ownership and retains unrelated source/global assets and exact file references", () => {
    const source = fixture();
    source.assets.push({ ...source.assets[0], id: "outside-owned", document_id: "outside", vault_path: "/vault/source/outside.png" });
    source.assets.push({ ...source.assets[0], id: "source-reusable", document_id: undefined, vault_path: "/vault/source/reusable.png" });
    source.assets.push({ ...source.assets[0], id: "global", document_id: undefined, space_id: undefined, vault_path: "/vault/global.png" });
    const before = clone(source); const result = prepareLocalReviewedMove(source, buildLocalMovePreview(source, intent), moveInput());
    expect(result.snapshot.assets.find(row => row.id === "asset")).toEqual({ ...before.assets[0], space_id: "target" });
    expect(result.snapshot.assets.filter(row => row.id !== "asset")).toEqual(before.assets.filter(row => row.id !== "asset"));
    expect(result.snapshot.assets.filter(row => row.space_id === "target" && row.document_id === "page")).toEqual([{ ...before.assets[0], space_id: "target" }]);
    expect(result.record.before.assets).toContainEqual(before.assets[0]);
    expect(result.record.after.assets).toContainEqual({ ...before.assets[0], space_id: "target" });
    expect(source).toEqual(before);
  });
  it("retains explicitly preserved and removed metadata raw in separate recovery evidence", () => {
    const source = fixture(); const choices = moveDecisions(); choices.properties[0].choice = { kind: "preserve" }; choices.workflows[0].choice = { kind: "remove" };
    const result = prepareLocalReviewedMove(source, buildLocalMovePreview(source, intent), moveInput(choices));
    expect(result.snapshot.property_values).toEqual([]); expect(result.snapshot.document_workflows).toEqual([]); expect(result.snapshot.document_workflow_events).toEqual([]);
    expect(result.record.preserved.find(row => row.collection === "property_values")!.records).toEqual(source.property_values);
    expect(result.record.removed.find(row => row.collection === "document_workflow_events")!.records).toEqual(source.document_workflow_events);
    expect(result.snapshot.property_schemas).toEqual(source.property_schemas); expect(result.snapshot.workflow_templates).toEqual(source.workflow_templates);
  });
  it("creates fresh compatible definitions without changing originals", () => {
    const source = fixture(); const choices = moveDecisions(); choices.properties[0].choice = { kind: "create", destination_id: "new-choice" }; choices.tags[0].choice = { kind: "create", destination_id: "new-tag" }; choices.workflows[0].choice = { kind: "create", destination_id: "new-workflow" };
    const result = prepareLocalReviewedMove(source, buildLocalMovePreview(source, intent), moveInput(choices));
    expect(result.snapshot.property_schemas!.find(row => row.id === "new-choice")).toEqual({ ...source.property_schemas![0], id: "new-choice", space_id: "target" });
    expect(result.snapshot.property_values![0].property_id).toBe("new-choice"); expect(result.snapshot.doc_tags![0].tag_id).toBe("new-tag"); expect(result.snapshot.document_workflows![0].workflow_template_id).toBe("new-workflow");
  });
  it("rejects stale source, missing choices and incompatible options without modifying canonical input", () => {
    const source = fixture(); const preview = buildLocalMovePreview(source, intent); const edited = clone(source); edited.documents[0].content = "newer B";
    expect(() => prepareLocalReviewedMove(edited, preview, moveInput())).toThrow("source changed"); expect(edited.documents[0].content).toBe("newer B");
    const choices = moveDecisions(); choices.properties = []; expect(() => prepareLocalReviewedMove(source, preview, moveInput(choices))).toThrow("every metadata");
    const invalid = clone(source); invalid.property_schemas!.find(row => row.id === "target-choice")!.options = ["C"];
    expect(() => prepareLocalReviewedMove(invalid, buildLocalMovePreview(invalid, intent), moveInput())).toThrow();
    expect(source).toEqual(fixture());
  });
  it("handles both incident directions explicitly and retains exact source rows in evidence", () => {
    const source = fixture(); source.property_values!.push({ id: "outgoing", document_id: "page", property_id: "relation", value: "outside" }, { id: "incoming", document_id: "outside", property_id: "relation", value: "page" });
    const choices = moveDecisions(); choices.properties.push({ source_id: "relation", choice: { kind: "create", destination_id: "new-relation" } });
    choices.relations = [{ value_id: "outgoing", choice: { kind: "map", document_id: "target-page" } }, { value_id: "incoming", choice: { kind: "preserve" } }];
    const result = prepareLocalReviewedMove(source, buildLocalMovePreview(source, intent), moveInput(choices));
    expect(result.snapshot.property_values!.find(row => row.id === "outgoing")).toEqual({ id: "outgoing", document_id: "page", property_id: "new-relation", value: "target-page" });
    expect(result.snapshot.property_values!.some(row => row.id === "incoming")).toBe(false);
    expect(result.record.preserved.find(row => row.collection === "property_values")!.records).toEqual([source.property_values!.find(row => row.id === "incoming")]);
    expect(result.record.before.property_values).toContainEqual(source.property_values!.find(row => row.id === "incoming"));
    const incomplete = moveDecisions(); incomplete.properties = choices.properties; expect(() => prepareLocalReviewedMove(source, buildLocalMovePreview(source, intent), moveInput(incomplete))).toThrow("every incoming");
  });
  it("does not capture unrelated private draft rows into the move record", () => {
    const source = fixture(); source.drafts!.push({ ...source.drafts![0], id: "foreign", document_id: "outside", owner_id: "foreign", content: "private bytes" });
    const result = prepareLocalReviewedMove(source, buildLocalMovePreview(source, intent), moveInput());
    expect(JSON.stringify(result.record)).not.toContain("private bytes"); expect(result.snapshot.drafts!.find(row => row.id === "foreign")!.content).toBe("private bytes");
  });
});

describe("malformed page source traversal resources", () => {
  it("blocks a large visible source cycle with bounded reads and no source mutation", () => {
    const snapshot = fixture(), count = 3000;
    let reads = 0;
    snapshot.documents = Array.from({ length: count }, (_, n) => ({
      id: `cycle-${n}`, space_id: "source", title: `Retained ${n}`, content: `Original body ${n}\r\n`, published_at: "old", updated_at: "old",
      get parent_id() { reads++; return `cycle-${(n + 1) % count}`; },
    }));
    snapshot.drafts = []; snapshot.sync_state!.queued_operations = [];
    const before = JSON.stringify(snapshot);
    reads = 0;
    const preview = buildLocalMovePreview(snapshot, { source_space_id: "source", root: { kind: "document", id: "cycle-0" }, destination: { space_id: "target", folder_id: null, parent_id: null } });
    const proofReads = reads;
    expect(preview.blockers).toContain("cyclic_hierarchy");
    expect(preview.inventory.documents).toHaveLength(count);
    expect(proofReads).toBeLessThan(count * 100);
    expect(JSON.stringify(snapshot)).toBe(before);
  });
});
