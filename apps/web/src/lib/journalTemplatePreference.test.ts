import { describe, expect, it } from "vitest";
import { findDailyTemplate, templatePropertiesAsRecord } from "./templates";
import { withJournalTemplatePreference } from "./journalTemplatePreference";
import type { TemplateRecord } from "./dataClient";
const template = (id: string, name: string, properties?: unknown): TemplateRecord => ({
  id,
  name,
  properties,
  space_id: "s",
  content: "",
  tags: [],
  created_at: "",
  updated_at: "",
});
describe("explicit Journal template selection", () => {
  it("prefers a selected custom template and restores the legacy fallback when cleared", () => {
    const legacy = template("legacy", "_daily");
    const custom = template(
      "custom",
      "Morning recipe",
      withJournalTemplatePreference({ Chef: "Eagle" }, true),
    );
    expect(findDailyTemplate([legacy, custom])?.id).toBe("custom");
    expect(templatePropertiesAsRecord(custom.properties)).toEqual({ Chef: "Eagle" });
    expect(
      findDailyTemplate([
        legacy,
        { ...custom, properties: withJournalTemplatePreference(custom.properties, false) },
      ])?.id,
    ).toBe("legacy");
    expect(findDailyTemplate([legacy])?.id).toBe("legacy");
  });
});
