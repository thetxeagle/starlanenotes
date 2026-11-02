import { describe, expect, it } from "vitest";
import {
  humanIdentifierKey,
  normalizeDocumentIdentifiers,
  normalizeHumanIdentifier,
  parseDocumentIdentifierIntent,
} from "./humanIdentifier";
describe("human identifiers", () => {
  it("normalizes display and comparison without treating identifiers as machine identity", () => {
    expect(normalizeHumanIdentifier("  Eng-0042  ")).toBe("Eng-0042");
    expect(humanIdentifierKey("  Eng-0042  ")).toBe("ENG-0042");
    expect(normalizeHumanIdentifier(" \n")).toBeNull();
    expect(normalizeHumanIdentifier(null)).toBeNull();
    for (const value of [undefined, 42, {}, [], "-ENG", "ENG 42", "é", "x".repeat(65)])
      expect(() => normalizeHumanIdentifier(value)).toThrow();
    expect(normalizeHumanIdentifier("x".repeat(64))).toHaveLength(64);
  });
  it("reserves case-insensitive references across active/archive/trash but scopes them to the space", () => {
    const one = {
      id: "stable-one",
      space_id: "space",
      human_identifier: "ENG-1",
      archived_at: "old",
    };
    const two = {
      id: "stable-two",
      space_id: "space",
      human_identifier: " eng-1 ",
      deleted_at: "old",
    };
    expect(() => normalizeDocumentIdentifiers([one, two])).toThrow(/reserved/);
    expect(normalizeDocumentIdentifiers([one, { ...two, space_id: "other" }])).toEqual([
      one,
      { ...two, space_id: "other", human_identifier: "eng-1" },
    ]);
    expect(one.id).toBe("stable-one");
  });
  it("retains missing legacy fields and original input while normalizing explicit clears", () => {
    const docs = [
      { id: "legacy", space_id: "space" },
      { id: "clear", space_id: "space", human_identifier: " " },
    ];
    const before = JSON.stringify(docs);
    const normalized = normalizeDocumentIdentifiers(docs);
    expect(normalized[0]).not.toHaveProperty("human_identifier");
    expect(normalized[1].human_identifier).toBeNull();
    expect(JSON.stringify(docs)).toBe(before);
  });
  it("rejects unrelated dedicated intent fields instead of silently acknowledging them", () => {
    const intent = { id: "page", space_id: "space", human_identifier: "ENG-2", source: { page_updated_at: "2026-10-08T00:00:00Z", published_at: null, human_identifier: "ENG-1" } };
    expect(parseDocumentIdentifierIntent(intent)).toEqual(intent);
    for (const key of ["human_identifier_key", "content", "owner_id", "updated_at"])
      expect(() => parseDocumentIdentifierIntent({ ...intent, [key]: "unacknowledged intent" })).toThrow(/unrelated/);
    for (const key of ["human_identifier_key", "content", "owner_id", "draft_id"])
      expect(() => parseDocumentIdentifierIntent({ ...intent, source: { ...intent.source, [key]: "unacknowledged source" } })).toThrow(/unrelated/);
    for (const key of ["human_identifier", "source"]) {
      const missing: Record<string, unknown> = { ...intent };
      delete missing[key];
      expect(() => parseDocumentIdentifierIntent(missing)).toThrow();
    }
  });

});
