import { describe, it, expect } from "vitest";
import { buildMoveCommit, createMoveDefinition, hasBlockingMoveReview, blockingMoveReviewReasons } from "./ReviewedMoveDialog";
import type { ServerMovePreview, MoveDefinitionReview } from "@starlane-notes/shared-types";
const metadata: MoveDefinitionReview = {
  kind: "property",
  source_definition: {
    id: "source-property",
    name: "Related page",
    property_type: "relation",
    options: null,
    sort_order: 0,
  },
  assignment_count: 1,
  assignments: [],
  create_allowed: false,
  candidates: [],
  resolution_candidates: [],
  decision_required: true,
  relation_scope_requires_resolution: true,
};
const preview = {
  fingerprint: "exact-fp",
  inventory: {
    pages: [{ id: "source-page", title: "Page" }],
    metadata: [metadata],
    relations: [
      {
        assignment_id: "assignment",
        property_id: "source-property",
        from_document_id: "source-page",
        to_document_id: "outside",
        crosses_subtree: true,
      },
    ],
  },
} as ServerMovePreview;
describe("deliberate reviewed move choices", () => {
  it("requires every definition and incident decision rather than dropping unsupported metadata", () => {
    expect(() => buildMoveCommit(preview, {}, {}, "preserve", true, true)).toThrow(
      "every metadata",
    );
  });
  it("binds outgoing mapped value to same assignment and incident target", () => {
    const r = buildMoveCommit(
      preview,
      {
        "property:source-property": {
          kind: "map",
          destination_id: "target-property",
          values: {},
          stages: {},
        },
      },
      { assignment: { kind: "map", target_id: "target-page" } },
      "disable",
      true,
      true,
    );
    expect(r.decisions[0].choice).toMatchObject({
      values: { assignment: "target-page" },
    });
    expect(r.incident_relations[0].choice).toEqual({
      kind: "map",
      target_id: "target-page",
    });
    expect(r.fingerprint).toBe("exact-fp");
  });
  it("rejects contradictory active-property mapping and incident preservation", () => {
    expect(() =>
      buildMoveCommit(
        preview,
        {
          "property:source-property": {
            kind: "map",
            destination_id: "target-property",
            values: {},
            stages: {},
          },
        },
        { assignment: { kind: "preserve" } },
        "preserve",
        true,
        true,
      ),
    ).toThrow("mapped relation property");
  });
  it("preserve and remove remain distinct explicit inactive-evidence choices", () => {
    for (const kind of ["preserve", "remove"] as const) {
      const c = kind === "remove" ? { kind, confirm: true as const } : { kind };
      expect(
        buildMoveCommit(
          preview,
          { "property:source-property": c },
          { assignment: c },
          "preserve",
          true,
          true,
        ).decisions[0].choice,
      ).toEqual(c);
    }
  });
  it("new definition excludes source identity/scope/timestamps and keeps actual options", () => {
    const m = {
      ...metadata,
      source_definition: {
        id: "old",
        space_id: "source",
        name: "Status",
        property_type: "select",
        options: '["Open","Done"]',
        sort_order: 4,
        created_at: "old",
      },
    };
    const d = createMoveDefinition(m, "Status copy");
    expect(d.options).toEqual(["Open", "Done"]);
    expect(d.id).not.toBe("old");
    expect(d).not.toHaveProperty("space_id");
    expect(d).not.toHaveProperty("created_at");
  });
});

describe("reviewed move identifiers", () => {
  const p = {
    ...preview,
    inventory: {
      pages: [
        {
          id: "source-page",
          title: "Shared title",
          space_id: "source",
          folder_id: null,
          parent_id: null,
          published: true,
          public_link_enabled: false,
          human_identifier: "REF-1",
          identifier_resolution_required: true,
          identifier_edit_allowed: true,
        },
      ],
      metadata: [],
      relations: [],
    },
  } as ServerMovePreview;
  it("requires explicit collision decisions and keeps stable document IDs", () => {
    expect(() => buildMoveCommit(p, {}, {}, "preserve", true, true)).toThrow("Rename or clear");
    const r = buildMoveCommit(p, {}, {}, "preserve", true, true, {
      "source-page": { kind: "rename", value: " Ref-2 " },
    });
    expect(r.identifier_decisions).toEqual([
      {
        document_id: "source-page",
        choice: { kind: "rename", value: "Ref-2" },
      },
    ]);
    expect(
      buildMoveCommit(p, {}, {}, "preserve", true, true, {
        "source-page": { kind: "clear" },
      }).identifier_decisions?.[0].choice.kind,
    ).toBe("clear");
    expect(() =>
      buildMoveCommit(
        {
          ...p,
          inventory: {
            ...p.inventory,
            pages: [{ ...p.inventory.pages![0], identifier_edit_allowed: false }],
          },
        },
        {},
        {},
        "preserve",
        true,
        true,
        { "source-page": { kind: "clear" } },
      ),
    ).toThrow("cannot change");
  });
});

describe("maintenance move choices preserve responsibility without access grants", () => {
  const p = (): ServerMovePreview => ({
    ...preview,
    inventory: {
      pages: [
        {
          id: "p",
          title: "Page",
          space_id: "source",
          folder_id: null,
          parent_id: null,
          published: true,
          public_link_enabled: false,
          maintenance_user_id: null,
          maintenance_group_id: "source-team",
          maintenance_resolution_required: true,
          maintenance_edit_allowed: true,
        },
      ],
      metadata: [],
      relations: [],
      maintenance_targets: {
        users: [{ id: "person", name: "Current person", active: true }],
        groups: [{ id: "team", name: "Destination team", managed_space_id: "target" }],
        groups_available: true,
      },
    },
  });
  it("requires explicit resolution, binds exact selected IDs and omits old empty decisions", () => {
    expect(() => buildMoveCommit(p(), {}, {}, "preserve", true, true)).toThrow("Map or clear");
    for (const choice of [
      { kind: "clear" } as const,
      { kind: "user", user_id: "person" } as const,
      { kind: "group", group_id: "team" } as const,
    ])
      expect(
        buildMoveCommit(p(), {}, {}, "preserve", true, true, {}, { p: choice })
          .maintenance_decisions,
      ).toEqual([{ document_id: "p", choice }]);
    const old = p();
    old.inventory.pages![0].maintenance_resolution_required = false;
    expect(
      buildMoveCommit(old, {}, {}, "preserve", true, true).maintenance_decisions,
    ).toBeUndefined();
    expect(
      buildMoveCommit(old, {}, {}, "preserve", true, true, {}, { p: { kind: "preserve" } })
        .maintenance_decisions,
    ).toEqual([{ document_id: "p", choice: { kind: "preserve" } }]);
  });
  it("cannot submit hidden IDs, unknown pages or a change without original edit capability", () => {
    expect(() =>
      buildMoveCommit(
        p(),
        {},
        {},
        "preserve",
        true,
        true,
        {},
        { p: { kind: "user", user_id: "hidden" } },
      ),
    ).toThrow("current destination");
    expect(() =>
      buildMoveCommit(p(), {}, {}, "preserve", true, true, {}, { unknown: { kind: "clear" } }),
    ).toThrow("unavailable page");
    const denied = p();
    denied.inventory.pages![0].maintenance_edit_allowed = false;
    expect(() =>
      buildMoveCommit(denied, {}, {}, "preserve", true, true, {}, { p: { kind: "clear" } }),
    ).toThrow("cannot change");
  });
});


describe("consumed move readiness for incident relation decisions", () => {
  const crossing = { ...preview, blockers: ["external_relations_require_resolution"] };
  it("complete deliberate crossing relation map admits the existing resolvable marker", () => {
    const commit = buildMoveCommit(crossing, {
      "property:source-property": { kind: "map", destination_id: "target-property", values: {}, stages: {} },
    }, { assignment: { kind: "map", target_id: "target-page" } }, "preserve", true, true);
    expect(commit.incident_relations[0]).toEqual({ assignment_id: "assignment", choice: { kind: "map", target_id: "target-page" } });
    expect(hasBlockingMoveReview(crossing, true)).toBe(false);
    expect(blockingMoveReviewReasons(crossing, true)).toEqual([]);
  });
  it("unresolved crossing choice cannot discharge the marker", () => {
    expect(() => buildMoveCommit(crossing, {}, {}, "preserve", true, true)).toThrow();
    expect(hasBlockingMoveReview(crossing, false)).toBe(true);
    expect(blockingMoveReviewReasons(crossing, false)).toEqual(["external_relations_require_resolution"]);
  });
  it("complete choices cannot waive an unrelated hard blocker", () => {
    expect(hasBlockingMoveReview({ ...crossing, blockers: ["external_relations_require_resolution", "subtree_unavailable"] }, true)).toBe(true);
    expect(blockingMoveReviewReasons({ ...crossing, blockers: ["external_relations_require_resolution", "subtree_unavailable"] }, true)).toEqual(["subtree_unavailable"]);
  });
});

describe("complete deliberate page viewer grant decisions", () => {
  const grant = {grant_id:"grant-a",document_id:"page",revision:"original-revision",role:"viewer" as const,scope:"page" as const,recipient:{available:true,display_name:"Viewer"},preserve_allowed:true,remove_allowed:true};
  const review: ServerMovePreview = {...preview,fingerprint:"original-fingerprint",inventory:{pages:[{id:"page",title:"Page",space_id:"source",folder_id:null,parent_id:null,published:true,public_link_enabled:false}],metadata:[],relations:[],viewer_grants:[grant]}};
  const build = (choices: Parameters<typeof buildMoveCommit>[8]) => buildMoveCommit(review,{}, {},"preserve",true,true,{}, {},choices);
  it("does not make omission an implicit preserved audience", () => {
    expect(() => build({})).toThrow("every page viewer grant");
  });
  it("copies exact accepted grant selector/revision and distinct explicit dispositions", () => {
    for (const choice of [{kind:"preserve" as const},{kind:"remove" as const,confirm:true as const}]) {
      const result = build({"grant-a":choice});
      expect(result.viewer_grant_decisions).toEqual([{grant_id:"grant-a",revision:"original-revision",choice}]);
      expect(result.fingerprint).toBe("original-fingerprint");
      expect(result.viewer_grant_decisions?.[0].choice).not.toBe(choice);
    }
  });
  it("rejects extra grant choices and missing newer current grant instead of rebasing", () => {
    expect(() => build({"foreign-grant":{kind:"preserve"}})).toThrow("unavailable grant");
    expect(() => buildMoveCommit({...review,inventory:{...review.inventory,viewer_grants:[grant,{...grant,grant_id:"grant-b"}]}},{},{},"preserve",true,true,{},{},{"grant-a":{kind:"preserve"}})).toThrow("every page viewer grant");
  });
  it("does not infer preserve/remove authority from move readiness or audience confirmation", () => {
    for (const kind of ["preserve","remove"] as const) {
      const choice = kind === "remove" ? {kind,confirm:true as const} : {kind};
      expect(() => buildMoveCommit({...review,commit_available:true,inventory:{...review.inventory,viewer_grants:[{...grant,preserve_allowed:false,remove_allowed:false}]}},{},{},"preserve",true,true,{},{},{"grant-a":choice})).toThrow(kind === "preserve" ? "cannot be preserved" : "cannot remove");
    }
  });
  it("refuses unconfirmed removal even if a malformed caller bypasses the choice type", () => {
    expect(() => build({"grant-a":{kind:"remove",confirm:false} as never})).toThrow("explicit confirmed");
  });
  it("keeps old no-grant request bytes without an added empty decision field", () => {
    const ordinary = buildMoveCommit({...review,inventory:{...review.inventory,viewer_grants:[]}},{},{},"preserve",true,true);
    expect(ordinary).not.toHaveProperty("viewer_grant_decisions");
  });
  it("permits explicit removal of an unavailable stored recipient when only remove is authorized", () => {
    const inaccessible = {...review,inventory:{...review.inventory,viewer_grants:[{...grant,recipient:{available:false,display_name:null},preserve_allowed:false}]}};
    expect(buildMoveCommit(inaccessible,{},{},"preserve",true,true,{},{},{"grant-a":{kind:"remove",confirm:true}}).viewer_grant_decisions?.[0].choice).toEqual({kind:"remove",confirm:true});
  });
});
