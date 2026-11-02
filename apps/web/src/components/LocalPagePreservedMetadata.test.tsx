import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { VaultDocument, VaultPropertySchema, VaultDocumentWorkflow, VaultDocumentWorkflowEvent, VaultWorkflowTemplate } from "@/lib/localVault";
import type { LocalReviewedMoveRecord } from "@/lib/localReviewedMove";
import { dataClient } from "@/lib/dataClient";
import { setStorageMode, setSyncProvider } from "@/lib/storageProviders";
import { LocalPagePreservedMetadata, LocalPreservedMetadataEvidence, projectLocalMoveMetadata, readLocalPreservedMetadata } from "./LocalPagePreservedMetadata";
const session = vi.hoisted(() => ({ token: "original", refresh: "refresh" }));
vi.mock("@/lib/api", () => ({ api: { getToken: () => session.token, getRefreshToken: () => session.refresh }, getApiBase: () => "http://fictional.invalid/api/v1", isTauri: () => false }));
vi.mock("@/lib/dataClient", () => ({ dataClient: { inspectLocalReviewedMoveRecords: vi.fn() } }));
const record = {
  schema_version: 1, id: "op", provider: "none", actor_id: "local-system", moved_at: "2026-10-09T12:00:00Z",
  intent: { source_space_id: "source", destination: { space_id: "destination" } },
  decisions: { properties: [{ source_id: "property", choice: { kind: "preserve" } }], tags: [{ source_id: "tag", choice: { kind: "remove" } }], workflows: [], relations: [], identifiers: [], maintenance: [], detach_hierarchy: [], preserve_previous_draft_placement: true },
  before: {
    documents: [{ id: "p", space_id: "source", title: "PRIVATE LABEL", content: "PAGE BODY", acl: "ACL SECRET", access_token: "TOKEN SECRET" }],
    drafts: [{ id: "draft", content: "DRAFT BODY", title: "DRAFT TITLE" }],
    search_index: [{ content: "SEARCH BODY" }],
    property_values: [{ id: "value", document_id: "p", property_id: "property", value: "<script>alert('x')</script>", payload: "PAYLOAD SECRET" }],
  }, after: { documents: [{ id: "p", space_id: "destination", content: "NEW PAGE BODY" }] },
  preserved: [{ collection: "property_schemas", records: [{ id: "property", name: "Fact", options: { nested: { token: "NESTED TOKEN", secret: "NESTED SECRET", label: "Visible option" } }, body: "HIDDEN PROPERTY BODY" }] }],
  removed: [{ collection: "tags", records: [{ id: "tag", name: "Old tag" }] }],
} as unknown as LocalReviewedMoveRecord;
const target = { spaceId: "destination", documentId: "p", accountId: "account-a" };
beforeEach(() => {
  const entries = new Map<string, string>();
  const storage = { getItem: (key: string) => entries.get(key) ?? null, setItem: (key: string, value: string) => { entries.set(key, value); }, removeItem: (key: string) => { entries.delete(key); }, clear: () => entries.clear(), key: () => null, get length() { return entries.size; } };
  vi.stubGlobal("window", { localStorage: storage }); vi.stubGlobal("localStorage", storage);
  setStorageMode("local"); setSyncProvider("none"); session.token = "original"; session.refresh = "refresh";
  vi.mocked(dataClient.inspectLocalReviewedMoveRecords).mockReset().mockResolvedValue([record]);
});
describe("Local preserved metadata projection", () => {
  it("shows explicit dispositions and scoped metadata as escaped text without bodies, access or token payloads", () => {
    const html = renderToStaticMarkup(<LocalPreservedMetadataEvidence record={record} />);
    expect(html).toContain("Property property: preserve"); expect(html).toContain("Tag tag: remove");
    expect(html).toContain("Before move"); expect(html).toContain("Expected after move"); expect(html).toContain("Preserved inactive"); expect(html).toContain("Removed from active metadata");
    expect(html).toContain("&lt;script&gt;"); expect(html).not.toContain("<script>"); expect(html).toContain("Visible option");
    for (const hidden of ["PRIVATE LABEL", "PAGE BODY", "DRAFT BODY", "DRAFT TITLE", "SEARCH BODY", "ACL SECRET", "TOKEN SECRET", "PAYLOAD SECRET", "NESTED TOKEN", "NESTED SECRET", "HIDDEN PROPERTY BODY"]) expect(html).not.toContain(hidden);
    expect(html).not.toContain("href="); expect(html).not.toContain("<button"); expect(html).toContain("restoration is unavailable");
  });
  it("projects actual typed identifier, relation sort order, workflow reference and authored action without changing evidence", () => {
    const document: VaultDocument = { id: "p", space_id: "source", title: "HIDDEN TITLE", content: "HIDDEN BODY", human_identifier: "PAGE-42", updated_at: "2026-10-09T12:00:00Z" };
    const schema: VaultPropertySchema = { id: "relation", space_id: "source", name: "Related", property_type: "relation", sort_order: 7 };
    const workflow: VaultDocumentWorkflow = { id: "workflow", document_id: "p", workflow_template_id: "template", current_stage_id: "review", status: "active", created_by: "author-id", created_at: "2026-10-09T11:00:00Z", updated_at: "2026-10-09T12:00:00Z" };
    const event: VaultDocumentWorkflowEvent = { id: "event", document_workflow_id: "workflow", document_id: "p", from_stage_id: "draft", to_stage_id: "review", action: "transition", created_by: "event-author", note: "PRIVATE NOTE", created_at: "2026-10-09T12:00:00Z" };
    const template: VaultWorkflowTemplate = { id: "template", space_id: "source", name: "Review", stages: [{ id: "review", name: "Review", sort_order: 2 }], approvers: [{ id: "approver", approver_type: "user", approver_id: "reviewer-id", stage_id: "review" }], created_at: "2026-10-09T11:00:00Z", updated_at: "2026-10-09T12:00:00Z" };
    const evidence: LocalReviewedMoveRecord = { ...record, before: { documents: [document], property_schemas: [schema], property_values: [{ id: "edge", document_id: "p", property_id: "relation", value: "target-id" }], workflow_templates: [template], document_workflows: [workflow], document_workflow_events: [event] } };
    const original = JSON.stringify(evidence);
    expect(projectLocalMoveMetadata("documents", [document])).toEqual([{ id: "p", space_id: "source", human_identifier: "PAGE-42" }]);
    expect(projectLocalMoveMetadata("property_schemas", [schema])).toEqual([schema]);
    expect(projectLocalMoveMetadata("document_workflow_events", [event])).toEqual([{ id: "event", document_id: "p", document_workflow_id: "workflow", from_stage_id: "draft", to_stage_id: "review", action: "transition", created_by: "event-author", created_at: event.created_at }]);
    const html = renderToStaticMarkup(<LocalPreservedMetadataEvidence record={evidence} />);
    for (const visible of ["PAGE-42", "sort_order", "document_workflow_id", "transition", "event-author", "target-id", "reviewer-id"]) expect(html).toContain(visible);
    for (const hidden of ["HIDDEN TITLE", "HIDDEN BODY", "PRIVATE NOTE"]) expect(html).not.toContain(hidden);
    expect(JSON.stringify(evidence)).toBe(original);
  });
  it.each([
    {},
    { ...record.decisions, properties: undefined },
    { ...record.decisions, properties: [null] },
    { ...record.decisions, properties: [{ source_id: "property" }] },
    { ...record.decisions, relations: [{ value_id: "edge", choice: { kind: "map" } }] },
  ])("retains unsupported decision history and suppresses both accepted reads and direct renders", async decisions => {
    // This is admitted by the portable envelope (object decisions), not a new import rule.
    const imported = { ...record, decisions } as unknown as LocalReviewedMoveRecord;
    const original = JSON.stringify(imported);
    vi.mocked(dataClient.inspectLocalReviewedMoveRecords).mockResolvedValueOnce([imported]);
    await expect(readLocalPreservedMetadata(target, () => target, () => true)).rejects.toThrow("Preserved metadata is unavailable");
    const html = renderToStaticMarkup(<LocalPreservedMetadataEvidence record={imported} />);
    expect(html).toContain("Preserved metadata is unavailable");
    expect(html).not.toContain("Before move"); expect(html).not.toContain("Visible option"); expect(html).not.toContain("<button");
    expect(JSON.stringify(imported)).toBe(original);
  });
  it("does not disclose unknown collections or nested credentials", () => {
    expect(projectLocalMoveMetadata("drafts", [{ content: "hidden" }])).toEqual([]);
    expect(projectLocalMoveMetadata("unknown", [{ content: "hidden" }])).toEqual([]);
    expect(projectLocalMoveMetadata("property_schemas", [{ id: "p", options: [{ password: "secret", label: "kept" }] }])).toEqual([{ id: "p", options: [{ label: "kept" }] }]);
  });
  it("starts collapsed with no inspection request or restoration controls", () => {
    const html = renderToStaticMarkup(<LocalPagePreservedMetadata spaceId="destination" documentId="p" />);
    expect(html).toContain('aria-expanded="false"'); expect(html).not.toContain("Move operation"); expect(html).not.toContain("Restore"); expect(dataClient.inspectLocalReviewedMoveRecords).not.toHaveBeenCalled();
  });
  it("keeps a valid empty record read-only without inventing active values", () => {
    const html = renderToStaticMarkup(<LocalPreservedMetadataEvidence record={{ ...record, before: {}, after: {}, preserved: [], removed: [] }} />);
    expect(html).toContain("No displayable metadata"); expect(html).not.toContain("<button");
  });
});
describe("Actual widget reader context boundary", () => {
  it("uses the exact target reader and leaves raw retained history untouched", async () => {
    const before = JSON.stringify(record); const result = await readLocalPreservedMetadata(target, () => target, () => true);
    expect(dataClient.inspectLocalReviewedMoveRecords).toHaveBeenCalledWith("destination", "p", expect.any(Function));
    expect(result.records).toEqual([record]); result.assertCurrent(); expect(JSON.stringify(record)).toBe(before);
  });
  it.each(["account", "page", "space", "selection", "provider", "token"])("suppresses a delayed response after original %s changes", async kind => {
    let selected = { ...target }; let selectionCurrent = true; let release!: (rows: LocalReviewedMoveRecord[]) => void;
    vi.mocked(dataClient.inspectLocalReviewedMoveRecords).mockImplementationOnce(() => new Promise(resolve => { release = resolve; }));
    const pending = readLocalPreservedMetadata(target, () => selected, () => selectionCurrent); const rejected = expect(pending).rejects.toThrow("original workspace");
    if (kind === "account") selected = { ...selected, accountId: "account-b" };
    if (kind === "page") selected = { ...selected, documentId: "other" };
    if (kind === "space") selected = { ...selected, spaceId: "other" };
    if (kind === "selection") selectionCurrent = false;
    if (kind === "provider") setSyncProvider("google-drive");
    if (kind === "token") session.token = "changed";
    release([record]); await rejected;
  });
  it("rejects invalidated selection before dispatch", async () => {
    await expect(readLocalPreservedMetadata(target, () => target, () => false)).rejects.toThrow("original workspace"); expect(dataClient.inspectLocalReviewedMoveRecords).not.toHaveBeenCalled();
  });
  it("rechecks context when a failed awaited reader resolves and never displays raw failure content", async () => {
    let selected = { ...target }; let reject!: (reason: unknown) => void;
    vi.mocked(dataClient.inspectLocalReviewedMoveRecords).mockImplementationOnce(() => new Promise((_, no) => { reject = no; }));
    const pending = readLocalPreservedMetadata(target, () => selected, () => true); const failed = expect(pending).rejects.toThrow("original workspace"); selected = { ...selected, accountId: "b" }; reject(new Error("TOKEN PRIVATE ERROR")); await failed;
  });
  it("provides generic failure and clears acceptance rather than using cached history", async () => {
    vi.mocked(dataClient.inspectLocalReviewedMoveRecords).mockRejectedValueOnce(new Error("secret inaccessible draft"));
    await expect(readLocalPreservedMetadata(target, () => target, () => true)).rejects.toThrow("Preserved metadata is unavailable"); expect(dataClient.inspectLocalReviewedMoveRecords).toHaveBeenCalledTimes(1);
  });
  it("rejects ambiguous operation identity instead of selecting arbitrary evidence", async () => {
    vi.mocked(dataClient.inspectLocalReviewedMoveRecords).mockResolvedValueOnce([record, { ...record }]);
    await expect(readLocalPreservedMetadata(target, () => target, () => true)).rejects.toThrow("Preserved metadata is unavailable");
  });
});
