import { describe, expect, it } from "vitest";
import type { TemplateRecord } from "@/lib/dataClient";
import {
  findDailyTemplate,
  findDailyFolder,
  formatDateStamp,
  renderDailyTemplateContent,
  renderTemplateContent,
  templatePropertiesAsRecord,
} from "./templates";

function makeTemplate(overrides: Partial<TemplateRecord>): TemplateRecord {
  return {
    id: "template-1",
    space_id: "local-space",
    name: "Template",
    content: "",
    tags: [],
    created_at: "2026-07-02T00:00:00.000Z",
    updated_at: "2026-07-02T00:00:00.000Z",
    ...overrides,
  };
}

describe("template helpers", () => {
  it("finds daily templates by reserved name or tag", () => {
    expect(
      findDailyTemplate([
        makeTemplate({ id: "regular", name: "Project Brief" }),
        makeTemplate({ id: "daily", name: "_daily" }),
      ])?.id,
    ).toBe("daily");

    expect(
      findDailyTemplate([
        makeTemplate({ id: "regular", name: "Project Brief" }),
        makeTemplate({ id: "tagged", name: "Journal", tags: ["daily"] }),
      ])?.id,
    ).toBe("tagged");
  });

  it("finds the reserved daily folder", () => {
    expect(
      findDailyFolder([
        { id: "root", name: "Notes" },
        { id: "daily", name: "_daily" },
      ])?.id,
    ).toBe("daily");
  });

  it("renders daily template placeholders", () => {
    const date = new Date(2026, 6, 2);
    const content = renderDailyTemplateContent(
      makeTemplate({ content: "# {{date}}\n\nISO: {{isoDate}}" }),
      date,
    );

    expect(content).toContain("Thursday, July 2, 2026");
    expect(content).toContain("ISO: 2026-07-02");
    expect(formatDateStamp(date)).toBe("2026-07-02");
  });

  it("renders document context, property, and cursor placeholders", () => {
    const date = new Date(2026, 6, 2, 9, 15);
    const rendered = renderTemplateContent(
      "# {{title}}\nSpace: {{space}}\nAuthor: {{author}}\nWhen: {{weekday}}, {{month}} {{year}}\nStatus: {{property.status}}\n{{cursor}}Body",
      {
        date,
        title: "Project Plan",
        space: "Personal Space",
        author: "Local User",
        properties: templatePropertiesAsRecord({ status: "Draft" }),
      },
    );

    expect(rendered.content).toContain("# Project Plan");
    expect(rendered.content).toContain("Space: Personal Space");
    expect(rendered.content).toContain("Author: Local User");
    expect(rendered.content).toContain("When: Thursday, July 2026");
    expect(rendered.content).toContain("Status: Draft");
    expect(rendered.content).toContain("\nBody");
    expect(rendered.cursorOffset).toBe(rendered.content.indexOf("Body"));
  });

  it("keeps unknown placeholders and blanks null or missing property values", () => {
    const rendered = renderTemplateContent(
      "Known: {{property.status}}\nNull: {{property.owner}}\nMissing: {{property.priority}}\nUnknown: {{workspace.name}}",
      {
        properties: templatePropertiesAsRecord({
          status: "Ready",
          owner: null,
        }),
      },
    );

    expect(rendered.content).toContain("Known: Ready");
    expect(rendered.content).toContain("Null: ");
    expect(rendered.content).toContain("Missing: ");
    expect(rendered.content).toContain("Unknown: {{workspace.name}}");
  });

  it("renders relative local-date placeholders across month boundaries", () => {
    const rendered = renderTemplateContent(
      "Yesterday: {{yesterdayIsoDate}}; Today: {{isoDate}}; Tomorrow: {{tomorrowIsoDate}}",
      { date: new Date(2026, 0, 31, 9, 15) },
    );

    expect(rendered.content).toBe("Yesterday: 2026-01-30; Today: 2026-01-31; Tomorrow: 2026-02-01");
  });
});
