import { describe, expect, it } from "vitest";
import {
  selectDocumentDraft,
  shouldChooseAccessibleDraft,
  type DocumentDraftSelectionCandidate,
} from "./documentDraftSelection";

interface Draft extends DocumentDraftSelectionCandidate {
  owner_name: string;
}

const drafts: Draft[] = [
  { id: "owned-old", access_role: "owner", updated_at: "2026-10-01T09:00:00Z", owner_name: "Me" },
  {
    id: "shared-new",
    access_role: "editor",
    updated_at: "2026-10-05T12:00:00Z",
    owner_name: "Alex",
  },
  { id: "owned-new", access_role: "owner", updated_at: "2026-10-04T12:00:00Z", owner_name: "Me" },
  {
    id: "shared-view",
    access_role: "viewer",
    updated_at: "2026-10-05T11:00:00Z",
    owner_name: "Sam",
  },
];

describe("selectDocumentDraft", () => {
  it("selects the requested accessible draft, including a shared editor draft", () => {
    expect(selectDocumentDraft(drafts, "shared-new")).toBe(drafts[1]);
  });

  it("selects the requested accessible viewer draft", () => {
    expect(selectDocumentDraft(drafts, "shared-view")).toBe(drafts[3]);
  });

  it("selects the newest owned draft regardless of input order or newer shared drafts", () => {
    expect(selectDocumentDraft(drafts)).toBe(drafts[2]);
  });

  it("breaks equal update-time ties by ascending stable ID", () => {
    const tied: Draft[] = [
      { id: "draft-b", access_role: "owner", updated_at: "2026-10-05T12:00:00Z", owner_name: "Me" },
      { id: "draft-a", access_role: "owner", updated_at: "2026-10-05T12:00:00Z", owner_name: "Me" },
    ];

    expect(selectDocumentDraft(tied)).toBe(tied[1]);
    expect(tied.map((draft) => draft.id)).toEqual(["draft-b", "draft-a"]);
  });

  it("reports an unavailable requested draft instead of falling back to another draft", () => {
    expect(selectDocumentDraft(drafts, "missing-draft")).toBeUndefined();
  });

  it("does not automatically select a shared editor or viewer draft", () => {
    expect(selectDocumentDraft(drafts.slice(1, 2))).toBeUndefined();
    expect(selectDocumentDraft([drafts[3]])).toBeUndefined();
  });

  it("reports no owned draft for an empty list", () => {
    expect(selectDocumentDraft([])).toBeUndefined();
  });

  it("treats a null request as automatic selection", () => {
    expect(selectDocumentDraft(drafts, null)).toBe(drafts[2]);
  });

  it("requires an explicit choice when only shared drafts are accessible", () => {
    expect(shouldChooseAccessibleDraft(drafts.slice(1, 2))).toBe(true);
    expect(shouldChooseAccessibleDraft([drafts[3]])).toBe(true);
    expect(shouldChooseAccessibleDraft(drafts)).toBe(false);
    expect(shouldChooseAccessibleDraft([])).toBe(false);
  });
});
