import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { DocumentRecord, TemplateRecord } from "@/lib/dataClient";
import { withJournalTemplatePreference } from "@/lib/journalTemplatePreference";
import {
  buildJournalCalendar,
  canCreateJournalEntry,
  calculateJournalStreak,
  claimJournalMutation,
  filterJournalEntries,
  getJournalDocumentPath,
  getJournalDataForSpace,
  getJournalPrompts,
  getJournalPromptTemplateCadence,
  getMissingJournalDays,
  getWeeklyJournalEntries,
  JournalFeedModeSelector,
  journalEntriesFromDocuments,
  parseJournalDate,
  releaseJournalMutation,
  selectJournalFeedEntries,
  settleCurrentJournalRequest,
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
  it("uses the explicit daily template while preserving the weekly prompt template", () => {
    const selected = template({ id: "selected", name: "Morning recipe", properties: withJournalTemplatePreference({}, true) });
    const weekly = template({ id: "weekly", name: "Weekly reflection", tags: ["journal-prompt", "weekly"] });
    const legacy = template({ id: "legacy", name: "_daily" });
    const prompts = getJournalPrompts([], new Date(2026, 9, 7), [legacy, weekly, selected]);
    expect(prompts.find((item) => item.id.startsWith("daily:"))?.template?.id).toBe("selected");
    expect(prompts.find((item) => item.id.startsWith("weekly:"))?.template?.id).toBe("weekly");
  });

  it("announces new entries so the Files tree can refresh its document and daily folder list", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/JournalPage.tsx"), "utf8");

    expect(source).toContain('emitGraphDirty(requestScope, "document-created")');
  });

  it("announces journal failures", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/JournalPage.tsx"), "utf8");

    expect(source).toContain('className="text-danger mt-3 text-sm" role="alert"');
  });

  it("announces Journal loading as a polite status", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/JournalPage.tsx"), "utf8");

    expect(source).toContain('role="status"');
    expect(source).toContain('aria-live="polite"');
    expect(source).toContain("Loading journal…");
  });

  it("ignores a superseded journal request", async () => {
    let resolveOld!: (value: string) => void;
    const oldRequest = new Promise<string>((resolve) => {
      resolveOld = resolve;
    });
    let currentRequestId = 1;
    let visible = "new space";

    const settling = settleCurrentJournalRequest(
      oldRequest,
      () => currentRequestId === 1,
      (value) => {
        visible = value;
      },
      () => {
        visible = "error";
      },
      () => undefined,
    );
    currentRequestId = 2;
    resolveOld("old space");
    await settling;

    expect(visible).toBe("new space");
  });

  it("hides the previous space's entries immediately while the next Journal loads", () => {
    const previousSpaceEntries = journalEntriesFromDocuments([doc("old-entry", "2026-07-05")]);

    expect(getJournalDataForSpace("space-a", "space-a", previousSpaceEntries)).toEqual(
      previousSpaceEntries,
    );
    expect(getJournalDataForSpace("space-b", "space-a", previousSpaceEntries)).toEqual([]);
    expect(getJournalDataForSpace("space-b", null, previousSpaceEntries)).toEqual([]);
  });

  it("blocks Journal creation while the current load is pending or failed", () => {
    expect(canCreateJournalEntry("space-a", "space-a", false, "")).toBe(true);
    expect(canCreateJournalEntry("space-a", "space-a", true, "")).toBe(false);
    expect(canCreateJournalEntry("space-a", "space-a", false, "load failed")).toBe(false);
    expect(canCreateJournalEntry("space-b", "space-a", false, "")).toBe(false);
  });

  it("ignores entry creation completion after the active space changes", async () => {
    let resolveCreation!: (value: string) => void;
    const creation = new Promise<string>((resolve) => {
      resolveCreation = resolve;
    });
    let activeScope = "space-a";
    const requestScope = activeScope;
    let destination = "current space";
    let busy = true;

    const settling = settleCurrentJournalRequest(
      creation,
      () => requestScope === activeScope,
      (documentId) => {
        destination = documentId;
      },
      () => {
        destination = "error";
      },
      () => {
        busy = false;
      },
    );
    activeScope = "space-b";
    resolveCreation("old-space-document");
    await settling;

    expect(destination).toBe("current space");
    expect(busy).toBe(true);
  });

  it("binds journal creation state and navigation to the active space", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/JournalPage.tsx"), "utf8");

    expect(source).toContain("requestScope === activeJournalScopeRef.current");
    expect(source).toContain("busyEntry?.scope === journalScope");
    expect(source).toContain("if (isCurrent()) navigate(getJournalDocumentPath(requestScope, created.id))");
  });

  it("serializes Journal creation before React renders disabled controls", () => {
    const lock = { scope: "space-a", dateStamp: "" };

    expect(claimJournalMutation(lock, "space-a", "2026-10-03")).toBe(true);
    expect(claimJournalMutation(lock, "space-a", "2026-10-03")).toBe(false);
    expect(claimJournalMutation(lock, "space-a", "2026-10-02")).toBe(false);
    expect(releaseJournalMutation(lock, "space-a", "2026-10-02")).toBe(false);
    expect(releaseJournalMutation(lock, "space-a", "2026-10-03")).toBe(true);
  });

  it("starts with an independent Journal creation lock after a space change", () => {
    const lock = { scope: "space-a", dateStamp: "2026-10-03" };

    expect(claimJournalMutation(lock, "space-b", "2026-10-03")).toBe(true);
    expect(lock).toEqual({ scope: "space-b", dateStamp: "2026-10-03" });
  });

  it("exposes the active Journal creation date as busy", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/JournalPage.tsx"), "utf8");

    expect(source.match(/aria-busy=\{busyDate ===/g)).toHaveLength(4);
    expect(source).toContain("Creating journal entry for ${todayStamp}");
    expect(source).toContain('"Creating Today…"');
  });

  it("announces the default Recent feed mode as selected", () => {
    const html = renderToStaticMarkup(
      <JournalFeedModeSelector mode="recent" onChange={() => undefined} />,
    );

    expect(html).toContain('role="group"');
    expect(html).toContain('aria-label="Journal feed mode"');
    expect(html).toContain('<button type="button" aria-pressed="true"');
    expect(html.match(/aria-pressed="true"/g)).toHaveLength(1);
    expect(html).toContain("Recent");
  });

  it("moves the announced selected state when the feed mode changes", () => {
    const html = renderToStaticMarkup(
      <JournalFeedModeSelector mode="week" onChange={() => undefined} />,
    );

    expect(html.match(/aria-pressed="true"/g)).toHaveLength(1);
    expect(html).toMatch(/aria-pressed="false"[^>]*>Recent</);
    expect(html).toMatch(/aria-pressed="true"[^>]*>This week</);
    expect(html).toMatch(/aria-pressed="false"[^>]*>Missed</);
  });

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
