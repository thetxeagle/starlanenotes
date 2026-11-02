import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { reviewedServerMoves, parseServerMovePreview, captureReviewedMoveContext, prepareReviewedMove, dispatchReviewedMove, parseServerMovePreservedEvidence } from "./reviewedServerMoves";
import { api } from "./api";
vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return { ...actual, api: Object.assign(actual.api, { fetch: vi.fn() }) };
});
import { setCurrentLaunchMode } from "./launchMode";
import { setStorageMode, setSyncProvider } from "./storageProviders";
export const previewFixture = {
  preview_id: "preview",
  expires_at: "2099-01-01T00:00:00Z",
  fingerprint: "fp",
  stale: false,
  inventory: { pages: [], folders: [], metadata: [], relations: [] },
  access_consequences: {
    space_audience_gained: 2,
    space_audience_lost: 1,
    public_links: "Intentional links",
    private_drafts: "Separate draft grants",
    permission_model: "existing",
  },
  blockers: [],
  commit_available: false,
  commit_unavailable_reason: "Writer safety prerequisites remain.",
};
const intent = {
  root: { kind: "document" as const, id: "page" },
  destination: { space_id: "destination", folder_id: null, parent_id: null },
};
describe("live reviewed move transport", () => {
  beforeEach(() => {
    vi.clearAllMocks(); const values = new Map<string, string>();
    const storage = { get length() { return values.size; }, key: (i: number) => [...values.keys()][i] ?? null, getItem: (k: string) => values.get(k) ?? null, setItem: (k: string, v: string) => { values.set(k, v); }, removeItem: (k: string) => { values.delete(k); } };
    vi.stubGlobal("localStorage", storage); vi.stubGlobal("window", { localStorage: storage }); vi.stubGlobal("navigator", { onLine: true });
    setCurrentLaunchMode("server"); setStorageMode("server"); setSyncProvider("none");
    api.setToken(`e30.${btoa(JSON.stringify({ sub: "fixture-owner", sid: "fixture-session", token_type: "access" }))}.fixture`);
  });
  afterEach(() => vi.unstubAllGlobals());
  it("retains exact grant choices across unknown response and historical retry", async () => {
    const request = { preview_fingerprint: "original", viewer_grant_decisions: [{ grant_id: "grant", revision: "r1", choice: { kind: "remove", confirm: true } }] };
    const pending = prepareReviewedMove(captureReviewedMoveContext(), "preview", request as never);
    const original = pending.requestJson;
    request.viewer_grant_decisions[0].revision = "newer";
    const fetcher = vi.fn().mockResolvedValueOnce(new Response("unknown", { status: 409 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ committed: true, operation_id: "original-op", historical_receipt: true }), { status: 200 }));
    vi.stubGlobal("fetch", fetcher);
    await expect(dispatchReviewedMove(pending)).rejects.toThrow();
    expect(pending.requestJson).toBe(original);
    expect(await dispatchReviewedMove(pending)).toMatchObject({ operation_id: "original-op", historical_receipt: true });
    for (const call of fetcher.mock.calls) expect(call[1].body).toBe(original);
    expect(api.fetch).not.toHaveBeenCalled();
  });
  it("refuses foreign account retry without dispatching retained grant choices", async () => {
    const pending = prepareReviewedMove(captureReviewedMoveContext(), "preview", { viewer_grant_decisions: [{ grant_id: "grant", revision: "r1", choice: { kind: "preserve" } }] } as never);
    const original = pending.requestJson;
    api.setToken(`e30.${btoa(JSON.stringify({ sub: "foreign", sid: "foreign-session", token_type: "access" }))}.fixture`);
    const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    await expect(dispatchReviewedMove(pending)).rejects.toThrow();
    expect(fetcher).not.toHaveBeenCalled(); expect(pending.requestJson).toBe(original);
  });
  it("uses fresh POST preview and owned DELETE cancellation without cached GET", async () => {
    const fetcher = vi.fn(async (_url: string, init?: RequestInit) => init?.method === "DELETE" ? new Response(null, { status: 204 }) : new Response(JSON.stringify(previewFixture), { status: 200 }));
    vi.stubGlobal("fetch", fetcher);
    expect(await reviewedServerMoves.preview("source", intent)).toEqual(previewFixture);
    await reviewedServerMoves.cancel("preview");
    expect(fetcher).toHaveBeenNthCalledWith(1, "/api/v1/spaces/source/moves/preview", expect.objectContaining({ method: "POST", body: JSON.stringify(intent) }));
    expect(fetcher).toHaveBeenNthCalledWith(2, "/api/v1/moves/previews/preview", expect.objectContaining({ method: "DELETE" }));
    expect(api.fetch).not.toHaveBeenCalled();
  });
  it("does not manufacture committed success from offline or malformed acknowledgements", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 200 })));
    await expect(reviewedServerMoves.commit("preview", {} as never)).rejects.toThrow("unconfirmed");
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ committed: false, operation_id: "operation" }), { status: 200 })));
    await expect(reviewedServerMoves.commit("preview", {} as never)).rejects.toThrow("unconfirmed");
  });
  it("preserves historic operation acknowledgement without reinterpreting later page state", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ committed: true, operation_id: "operation", historical_receipt: true }), { status: 200 })));
    expect(await reviewedServerMoves.commit("preview", {} as never)).toMatchObject({ historical_receipt: true });
  });
  it("rejects malformed source choices but permits deliberately opaque blocked inventories", () => {
    expect(
      parseServerMovePreview({
        ...previewFixture,
        inventory: {},
        blockers: ["subtree_unavailable"],
      }).inventory,
    ).toEqual({});
    expect(() =>
      parseServerMovePreview({
        ...previewFixture,
        inventory: {
          metadata: [{ kind: "property", source_definition: { id: "p" } }],
        },
      }),
    ).toThrow("metadata");
  });
});

it("rejects malformed identifier projection fields while preserving omitted legacy values", () => {
  expect(() =>
    parseServerMovePreview({
      ...previewFixture,
      inventory: {
        ...previewFixture.inventory,
        pages: [{ id: "page", title: "Page", human_identifier: 42 }],
      },
    }),
  ).toThrow("identifier");
  for (const field of ["identifier_resolution_required", "identifier_edit_allowed"])
    expect(() =>
      parseServerMovePreview({
        ...previewFixture,
        inventory: {
          ...previewFixture.inventory,
          pages: [{ id: "page", title: "Page", [field]: "true" }],
        },
      }),
    ).toThrow("identifier");
  expect(
    parseServerMovePreview({
      ...previewFixture,
      inventory: {
        ...previewFixture.inventory,
        pages: [
          {
            id: "page",
            title: "Page",
            human_identifier: null,
            identifier_resolution_required: false,
            identifier_edit_allowed: true,
          },
        ],
      },
    }).inventory.pages?.[0].human_identifier,
  ).toBeNull();
});

it("validates maintenance XOR pairs and actual catalog target shapes without changing fresh transport", () => {
  const value = {
    ...previewFixture,
    inventory: {
      ...previewFixture.inventory,
      pages: [
        {
          id: "p",
          title: "Page",
          maintenance_user_id: "u",
          maintenance_group_id: null,
          maintenance_resolution_required: false,
          maintenance_edit_allowed: true,
        },
      ],
      maintenance_targets: {
        users: [{ id: "u", name: "Person", active: true }],
        groups: [],
        groups_available: false,
      },
    },
  };
  expect(parseServerMovePreview(value)).toEqual(value);
  expect(() =>
    parseServerMovePreview({
      ...value,
      inventory: {
        ...value.inventory,
        pages: [{ maintenance_user_id: "u", maintenance_group_id: "g" }],
      },
    }),
  ).toThrow("identifier review");
  expect(() =>
    parseServerMovePreview({
      ...value,
      inventory: {
        ...value.inventory,
        maintenance_targets: {
          ...value.inventory.maintenance_targets,
          users: [{ id: "u", name: "Person", active: false }],
        },
      },
    }),
  ).toThrow("maintenance targets");
});

import {
  parseServerMovePreservedIndex,
} from "./reviewedServerMoves";
const indexFixture = {
  schema_version: 1,
  space_id: "space",
  document_id: "page",
  checked_at: "2026-10-08T21:00:00Z",
  operations: [
    { operation_id: "op", created_at: "2026-10-07T20:00:00Z", kind: "immutable_metadata_evidence" },
  ],
  next_cursor: null,
};
const evidenceFixture = {
  space_id: "space",
  document_id: "page",
  operation_id: "op",
  kind: "immutable_metadata_evidence",
  editable: false,
  restoration_available: false,
  entries: [
    {
      family: "property",
      source_id: "prop",
      disposition: "remove",
      original: { value: "<script>raw evidence</script>" },
      expected_active: null,
    },
  ],
};
describe("fresh preserved move inspection", () => {
  beforeEach(() => vi.clearAllMocks());
  it("uses bodyless POST for index/cursor/evidence with exact encoded binding", async () => {
    vi.mocked(api.fetch).mockResolvedValue(indexFixture);
    await reviewedServerMoves.preservedIndex("space", "page", "raw/op");
    expect(api.fetch).toHaveBeenCalledWith(
      "/moves/documents/page/preserved/inspect?cursor=raw%2Fop",
      { method: "POST" },
    );
    vi.mocked(api.fetch).mockResolvedValue(evidenceFixture);
    expect(await reviewedServerMoves.preservedEvidence("space", "page", "op")).toEqual(
      evidenceFixture,
    );
    expect(api.fetch).toHaveBeenLastCalledWith("/moves/commits/op/preserved/page/inspect", {
      method: "POST",
    });
  });
  it("never falls back to cached success after rejection", async () => {
    vi.mocked(api.fetch).mockRejectedValue(new Error("Forbidden"));
    await expect(reviewedServerMoves.preservedIndex("space", "page")).rejects.toThrow("Forbidden");
    await expect(reviewedServerMoves.preservedEvidence("space", "page", "op")).rejects.toThrow(
      "Forbidden",
    );
    expect(api.fetch).toHaveBeenCalledTimes(2);
  });
  it("rejects wrong targets, malformed timestamps and mutation claims", () => {
    for (const delta of [
      { space_id: "other" },
      { document_id: "other" },
      { schema_version: 2 },
      { hidden_count: 2 },
      { checked_at: "yesterday" },
      { next_cursor: "hidden" },
      { operations: [...indexFixture.operations, ...indexFixture.operations] },
    ])
      expect(() =>
        parseServerMovePreservedIndex({ ...indexFixture, ...delta }, "space", "page"),
      ).toThrow();
    for (const delta of [
      { space_id: "other" },
      { document_id: "other" },
      { operation_id: "other" },
      { editable: true },
      { restoration_available: true },
      { entries: [{ ...evidenceFixture.entries[0], family: "body" }] },
      { entries: [{ ...evidenceFixture.entries[0], original: undefined }] },
    ])
      expect(() =>
        parseServerMovePreservedEvidence({ ...evidenceFixture, ...delta }, "space", "page", "op"),
      ).toThrow();
  });
  it("accepts bounded disclosed-cursor pages without hidden totals and empty bound evidence", () => {
    const operations = Array.from({ length: 20 }, (_, i) => ({
      ...indexFixture.operations[0],
      operation_id: `op-${i}`,
    }));
    expect(
      parseServerMovePreservedIndex(
        { ...indexFixture, operations, next_cursor: "op-19" },
        "space",
        "page",
      ).next_cursor,
    ).toBe("op-19");
    expect(() =>
      parseServerMovePreservedIndex(
        { ...indexFixture, operations, next_cursor: "hidden" },
        "space",
        "page",
      ),
    ).toThrow();
    expect(() =>
      parseServerMovePreservedIndex(
        {
          ...indexFixture,
          operations: [...operations, { ...operations[0], operation_id: "extra" }],
          next_cursor: null,
        },
        "space",
        "page",
      ),
    ).toThrow();
    expect(
      parseServerMovePreservedEvidence({ ...evidenceFixture, entries: [] }, "space", "page", "op")
        .entries,
    ).toEqual([]);
  });
});

const viewerGrant = {
  grant_id: "grant-original", document_id: "page", revision: "revision-original",
  role: "viewer" as const, scope: "page" as const,
  recipient: {available:true,display_name:"Viewer"}, preserve_allowed:true, remove_allowed:true,
};
const grantedPreview = {...previewFixture, inventory:{...previewFixture.inventory,pages:[{id:"page",title:"Published page"}],viewer_grants:[viewerGrant]}};
describe("strict current viewer grants in a server move preview", () => {
  it("accepts exact permitted projection and legacy no-grant preview without inventing grants", () => {
    expect(parseServerMovePreview(grantedPreview).inventory.viewer_grants).toEqual([viewerGrant]);
    expect(parseServerMovePreview(previewFixture).inventory.viewer_grants).toBeUndefined();
  });
  it.each([
    {revision:""}, {role:"editor"}, {scope:"subtree"}, {preserve_allowed:"true"},
    {document_id:"hidden-page"}, {user_id:"hidden-user"},
    {recipient:{available:false,display_name:"Hidden former name"}},
    {recipient:{available:true,display_name:"Name",email:"private@example.test"}},
  ])("rejects malformed or secret-bearing grant projection %j", changed => {
    expect(() => parseServerMovePreview({...grantedPreview, inventory:{...grantedPreview.inventory,viewer_grants:[{...viewerGrant,...changed}]}})).toThrow("viewer grants");
  });
  it("rejects duplicate grants rather than collapsing two selector witnesses", () => {
    expect(() => parseServerMovePreview({...grantedPreview,inventory:{...grantedPreview.inventory,viewer_grants:[viewerGrant,viewerGrant]}})).toThrow("viewer grants");
  });
  it("requires unavailable recipient labels to stay null without blocking an authorized remove choice", () => {
    expect(parseServerMovePreview({...grantedPreview,inventory:{...grantedPreview.inventory,viewer_grants:[{...viewerGrant,recipient:{available:false,display_name:null},preserve_allowed:false}]}}).inventory.viewer_grants?.[0].recipient.display_name).toBeNull();
  });
});

const projectedGrant = { grant_id: "grant", document_id: "page", space_id: "source", revision: "r1", role: "viewer", scope: "page", recipient: { available: false, display_name: null } };
const grantEvidence = { space_id: "destination", document_id: "page", operation_id: "operation", kind: "immutable_metadata_evidence", editable: false, restoration_available: false, entries: [{ family: "page_viewer_grant", source_id: "grant", disposition: "remove", original: projectedGrant, expected_active: null }] };
describe("projected historical viewer grant admission", () => {
  it("accepts removed null and preserved destination tuple without treating history as current authority", () => {
    expect(parseServerMovePreservedEvidence(grantEvidence, "destination", "page", "operation")).toEqual(grantEvidence);
    const preserved = { ...grantEvidence, entries: [{ ...grantEvidence.entries[0], disposition: "preserve", expected_active: { ...projectedGrant, space_id: "destination", revision: "r2" } }] };
    expect(parseServerMovePreservedEvidence(preserved, "destination", "page", "operation")).toEqual(preserved);
    expect(parseServerMovePreservedEvidence({ ...grantEvidence, entries: [] }, "destination", "page", "operation").entries).toEqual([]);
  });
  it.each([
    { ...projectedGrant, user_id: "sealed" },
    { ...projectedGrant, document_id: "foreign" },
    { ...projectedGrant, grant_id: "foreign" },
    { ...projectedGrant, recipient: { available: false, display_name: "Old private name" } },
    { ...projectedGrant, recipient: { available: true, display_name: "Visible", email: "secret" } },
    { ...projectedGrant, role: "editor" },
  ])("rejects malformed or sealed raw original tuple %j", (original) => {
    expect(() => parseServerMovePreservedEvidence({ ...grantEvidence, entries: [{ ...grantEvidence.entries[0], original }] }, "destination", "page", "operation")).toThrow("unavailable");
  });
  it("rejects missing preserve evidence or non-null removed evidence", () => {
    for (const entry of [{ ...grantEvidence.entries[0], disposition: "preserve" }, { ...grantEvidence.entries[0], expected_active: projectedGrant }])
      expect(() => parseServerMovePreservedEvidence({ ...grantEvidence, entries: [entry] }, "destination", "page", "operation")).toThrow("unavailable");
  });
});
