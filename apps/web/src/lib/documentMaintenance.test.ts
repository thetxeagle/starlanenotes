import { describe, expect, it } from "vitest";
import {
  maintenanceFields,
  normalizeDocumentMaintenance,
  normalizeDocumentMaintenanceFields,
  parseDocumentMaintenanceIntent,
  normalizeDocumentMaintenanceTargets,
  normalizeDocumentMaintenanceCatalogs,
} from "./documentMaintenance";
describe("maintenance responsibility metadata", () => {
  it("keeps exact identity and legacy omissions without inventing ownership or granting access", () => {
    expect(normalizeDocumentMaintenanceFields({ title: "Legacy" })).toEqual({ title: "Legacy" });
    expect(normalizeDocumentMaintenanceFields({ maintenance_user_id: "Person-A" })).toEqual({
      maintenance_user_id: "Person-A",
    });
    expect(maintenanceFields({ kind: "group", group_id: "Team-A" })).toEqual({
      maintenance_user_id: null,
      maintenance_group_id: "Team-A",
    });
    for (const fields of [
      { maintenance_user_id: "person", maintenance_group_id: "team" },
      { maintenance_user_id: 42 },
      { maintenance_group_id: "" },
    ])
      expect(() => normalizeDocumentMaintenanceFields(fields)).toThrow();
    for (const value of [
      { kind: "user", user_id: null },
      { kind: "unassigned", owner_id: "hidden" },
      { kind: "group", group_id: "" },
      { kind: "user", user_id: "person", role: "owner" },
    ])
      expect(() => normalizeDocumentMaintenance(value)).toThrow();
  });
  it("requires both original assignment IDs even when timestamps have not changed", () => {
    const value = {
      id: "page",
      space_id: "space",
      maintenance: { kind: "unassigned" },
      source: {
        page_updated_at: "2026-10-08T00:00:00Z",
        published_at: null,
        maintenance_user_id: "Person-A",
        maintenance_group_id: null,
      },
    };
    expect(parseDocumentMaintenanceIntent(value)).toEqual(value);
    for (const key of ["maintenance_user_id", "maintenance_group_id", "published_at"]) {
      const source: Record<string, unknown> = { ...value.source };
      delete source[key];
      expect(() => parseDocumentMaintenanceIntent({ ...value, source })).toThrow();
    }
    for (const extra of ["content", "owner_id", "human_identifier", "metadata_intent"])
      expect(() =>
        parseDocumentMaintenanceIntent({ ...value, [extra]: "unacknowledged" }),
      ).toThrow();
    expect(() =>
      parseDocumentMaintenanceIntent({ ...value, source: { ...value.source, draft_id: "wrong" } }),
    ).toThrow();
  });
  it("validates a data-only catalog and exact provenance without inventing teams", () => {
    const targets = {
      users: [{ id: "person", name: "Person", active: true }],
      groups: [],
      groups_available: false,
    };
    expect(normalizeDocumentMaintenanceTargets(targets)).toEqual(targets);
    expect(() =>
      normalizeDocumentMaintenanceTargets({
        ...targets,
        groups: [{ id: "fake", name: "Fake", managed_space_id: null }],
      }),
    ).toThrow();
    expect(() =>
      normalizeDocumentMaintenanceTargets({
        ...targets,
        users: [{ id: "person", name: "Person", active: false }],
      }),
    ).toThrow();
    const record = {
      server_base: "https://notes/api/v1",
      account_id: "account",
      space_id: "space",
      targets,
    };
    expect(normalizeDocumentMaintenanceCatalogs([record])).toEqual([record]);
    expect(() => normalizeDocumentMaintenanceCatalogs([record, record])).toThrow(/ambiguous/);
  });
});
