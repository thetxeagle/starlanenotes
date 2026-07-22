import { describe, expect, it } from "vitest";
import type { DocumentRecord, TemplateRecord } from "@/lib/dataClient";
import {
  buildJournalCalendar,
  calculateJournalStreak,
  filterJournalEntries,
  getJournalDocumentPath,
  getJournalPrompts,
  getJournalPromptTemplateCadence,
  getMissingJournalDays,
  getWeeklyJournalEntries,
  journalEntriesFromDocuments,
  parseJournalDate,
  selectJournalFeedEntries,
} from "@/pages/JournalPage";

function doc(id: string, title: string, updated_at = "2026-07-05T12:00:00.000Z"): DocumentRecord {
  return { id, title, updated_at };
}

function template(input: Partial<TemplateRecord> & Pick<TemplateRecord, "id" | "name">): TemplateRecord {
  return {
    space_id: "space-1",
    description: "",
    content: "",
    properties: {},
    tags: [],
    created_at: "2026-07-05T12:00:00.000Z",
    updated_at: "2026-07-05T12:00:00.000Z",
    ...input,
  };
}

describe("JournalPage helpers", () => {
  it("builds encoded document routes for journal entries", () => {
    expect(getJournalDocumentPath("space-1", "doc-1")).toBe("/spaces/space-1/docs/doc-1");
    expect(getJournalDocumentPath("space/one", "doc/one")).toBe(
      "/spaces/space%2Fone/docs/doc%2Fone",
    );
  });

  it("parses only valid daily-note titles", () => {
    expect(parseJournalDate("2026-07-05")?.getFullYear()).toBe(2026);
    expect(parseJournalDate("2026-02-31")).toBeNull();
    expect(parseJournalDate("Daily 2026-07-05")).toBeNull();
  });

  it("extracts and sorts journal entries from documents", () => {
    const entries = journalEntriesFromDocuments([
      doc("older", "2026-07-03"),
      doc("skip", "Project Plan"),
      doc("newer", "2026-07-05"),
    ]);

    expect(entries.map((entry) => entry.id)).toEqual(["newer", "older"]);
    expect(entries[0].dateStamp).toBe("2026-07-05");
  });

  it("builds a six-week calendar grid with entry and today markers", () => {
    const entries = journalEntriesFromDocuments([doc("today", "2026-07-05")]);
    const days = buildJournalCalendar(new Date(2026, 6, 1), entries, new Date(2026, 6, 5));
    const today = days.find((day) => day.dateStamp === "2026-07-05");

    expect(days).toHaveLength(42);
    expect(days[0].dateStamp).toBe("2026-06-28");
    expect(today?.isToday).toBe(true);
    expect(today?.entry?.id).toBe("today");
  });

  it("calculates current streaks from today backward", () => {
    const entries = journalEntriesFromDocuments([
      doc("today", "2026-07-05"),
      doc("yesterday", "2026-07-04"),
      doc("gap", "2026-07-02"),
    ]);

    expect(calculateJournalStreak(entries, new Date(2026, 6, 5))).toBe(2);
  });

  it("filters entries by title and formatted date text", () => {
    const entries = journalEntriesFromDocuments([doc("a", "2026-07-05"), doc("b", "2026-06-01")]);

    expect(filterJournalEntries(entries, "july").map((entry) => entry.id)).toEqual(["a"]);
    expect(filterJournalEntries(entries, "2026-06-01").map((entry) => entry.id)).toEqual(["b"]);
  });

  it("selects recent and current-week feed entries", () => {
    const entries = journalEntriesFromDocuments([
      doc("sun", "2026-07-05"),
      doc("mon", "2026-07-06"),
      doc("previous", "2026-06-30"),
    ]);

    expect(selectJournalFeedEntries(entries, "recent").map((entry) => entry.id)).toEqual([
      "mon",
      "sun",
      "previous",
    ]);
    expect(getWeeklyJournalEntries(entries, new Date(2026, 6, 6)).map((entry) => entry.id)).toEqual([
      "mon",
      "sun",
    ]);
  });

  it("finds missing journal days from today backward", () => {
    const entries = journalEntriesFromDocuments([
      doc("today", "2026-07-05"),
      doc("two-days-ago", "2026-07-03"),
    ]);

    expect(getMissingJournalDays(entries, new Date(2026, 6, 5), 3).map((day) => day.dateStamp)).toEqual([
      "2026-07-04",
      "2026-07-02",
      "2026-07-01",
    ]);
  });

  it("creates recurring prompts only when their cadence is due", () => {
    const emptyPrompts = getJournalPrompts([], new Date(2026, 6, 5));
    expect(emptyPrompts.map((prompt) => prompt.id)).toEqual(["daily", "weekly", "monthly"]);

    expect(getJournalPrompts([], new Date(2026, 6, 8)).map((prompt) => prompt.id)).toEqual([
      "daily",
      "weekly",
    ]);

    const entries = journalEntriesFromDocuments([
      doc("today", "2026-07-05"),
      doc("week", "2026-07-01"),
    ]);
    expect(getJournalPrompts(entries, new Date(2026, 6, 5))).toEqual([]);
  });

  it("detects prompt template cadences from properties and tags", () => {
    expect(
      getJournalPromptTemplateCadence(
        template({
          id: "weekly-template",
          name: "Weekly Reflection",
          properties: { journal_prompt_cadence: "weekly" },
        }),
      ),
    ).toBe("weekly");
    expect(
      getJournalPromptTemplateCadence(
        template({ id: "monthly-template", name: "Monthly", tags: ["journal-prompt", "monthly"] }),
      ),
    ).toBe("monthly");
    expect(
      getJournalPromptTemplateCadence(
        template({
          id: "camel-weekly",
          name: "Camel Weekly",
          properties: { journalPromptCadence: "weekly" },
        }),
      ),
    ).toBe("weekly");
    expect(
      getJournalPromptTemplateCadence(
        template({ id: "daily-boolean", name: "Daily Boolean", properties: { journalPrompt: true } }),
      ),
    ).toBe("daily");
    expect(getJournalPromptTemplateCadence(template({ id: "plain", name: "Plain" }))).toBeNull();
  });

  it("uses prompt templates to customize cards and created-note content", () => {
    const prompts = getJournalPrompts([], new Date(2026, 6, 5), [
      template({
        id: "weekly-template",
        name: "Commander Intent",
        description: "Set weekly direction.",
        content: "# Week of {{date}}",
        properties: {
          journal_prompt_cadence: "weekly",
          journal_prompt_title: "Weekly commander's intent",
          journal_prompt_description: "Pick objectives and blockers.",
        },
      }),
    ]);
    const weekly = prompts.find((prompt) => prompt.id === "weekly:weekly-template");

    expect(weekly).toMatchObject({
      id: "weekly:weekly-template",
      title: "Weekly commander's intent",
      description: "Pick objectives and blockers.",
    });
    expect(weekly?.template?.content).toBe("# Week of {{date}}");
  });
});
