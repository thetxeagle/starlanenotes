import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  CalendarPlus,
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  FileText,
  ListFilter,
  Loader2,
  Plus,
  Sparkles,
} from "lucide-react";
import { SearchInput } from "@/components/ui/search-input";
import {
  dataClient,
  type DocumentRecord,
  type FolderRecord,
  type TemplateRecord,
} from "@/lib/dataClient";
import { emitGraphDirty } from "@/lib/graphEvents";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import {
  DAILY_FOLDER_NAME,
  findDailyFolder,
  findDailyTemplate,
  formatDateStamp,
  renderDailyTemplateContent,
  templatePropertiesAsRecord,
} from "@/lib/templates";

interface JournalEntry {
  id: string;
  title: string;
  dateStamp: string;
  date: Date;
  updatedAt?: string;
}

export type JournalFeedMode = "recent" | "week" | "missed";

export interface JournalMissingDay {
  dateStamp: string;
  date: Date;
}

export interface JournalPrompt {
  id: string;
  title: string;
  description: string;
  date: Date;
  dateStamp: string;
  template?: TemplateRecord;
}

export interface JournalCalendarDay {
  dateStamp: string;
  date: Date;
  inMonth: boolean;
  isToday: boolean;
  entry?: JournalEntry;
}

export function getJournalDocumentPath(spaceId: string, documentId: string): string {
  return buildDocumentRoutePath(spaceId, documentId);
}

function startOfDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function addDays(date: Date, days: number) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

function startOfWeek(date: Date) {
  return addDays(startOfDay(date), -date.getDay());
}

function monthLabel(date: Date) {
  return date.toLocaleDateString(undefined, { month: "long", year: "numeric" });
}

export function parseJournalDate(title: string): Date | null {
  const match = title.trim().match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const parsed = new Date(year, month - 1, day);
  if (
    parsed.getFullYear() !== year ||
    parsed.getMonth() !== month - 1 ||
    parsed.getDate() !== day
  ) {
    return null;
  }
  return parsed;
}

export function journalEntriesFromDocuments(documents: DocumentRecord[]): JournalEntry[] {
  return documents
    .flatMap((doc) => {
      const date = parseJournalDate(doc.title);
      if (!date) return [];
      return [{
        id: doc.id,
        title: doc.title,
        dateStamp: formatDateStamp(date),
        date,
        ...(doc.updated_at ? { updatedAt: doc.updated_at } : {}),
      }];
    })
    .sort((a, b) => b.dateStamp.localeCompare(a.dateStamp));
}

export function buildJournalCalendar(
  anchorDate: Date,
  entries: JournalEntry[],
  today = new Date(),
): JournalCalendarDay[] {
  const monthStart = new Date(anchorDate.getFullYear(), anchorDate.getMonth(), 1);
  const gridStart = addDays(monthStart, -monthStart.getDay());
  const entryByDate = new Map(entries.map((entry) => [entry.dateStamp, entry]));
  const todayStamp = formatDateStamp(today);

  return Array.from({ length: 42 }, (_, index) => {
    const date = addDays(gridStart, index);
    const dateStamp = formatDateStamp(date);
    return {
      dateStamp,
      date,
      inMonth: date.getMonth() === anchorDate.getMonth(),
      isToday: dateStamp === todayStamp,
      entry: entryByDate.get(dateStamp),
    };
  });
}

export function calculateJournalStreak(entries: JournalEntry[], today = new Date()): number {
  const stamps = new Set(entries.map((entry) => entry.dateStamp));
  let cursor = startOfDay(today);
  let streak = 0;
  while (stamps.has(formatDateStamp(cursor))) {
    streak += 1;
    cursor = addDays(cursor, -1);
  }
  return streak;
}

export function filterJournalEntries(entries: JournalEntry[], query: string): JournalEntry[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return entries;
  return entries.filter((entry) => {
    const formatted = entry.date.toLocaleDateString(undefined, {
      weekday: "long",
      month: "long",
      day: "numeric",
      year: "numeric",
    });
    return `${entry.title} ${formatted}`.toLowerCase().includes(normalized);
  });
}

export function getRecentJournalEntries(entries: JournalEntry[], limit = 10): JournalEntry[] {
  return entries.slice(0, limit);
}

export function getWeeklyJournalEntries(entries: JournalEntry[], today = new Date()): JournalEntry[] {
  const weekStart = startOfWeek(today);
  const weekEnd = addDays(weekStart, 7);
  return entries.filter((entry) => entry.date >= weekStart && entry.date < weekEnd);
}

export function getMissingJournalDays(
  entries: JournalEntry[],
  today = new Date(),
  limit = 7,
): JournalMissingDay[] {
  const stamps = new Set(entries.map((entry) => entry.dateStamp));
  const days: JournalMissingDay[] = [];
  let cursor = startOfDay(today);
  while (days.length < limit) {
    const dateStamp = formatDateStamp(cursor);
    if (!stamps.has(dateStamp)) {
      days.push({ date: cursor, dateStamp });
    }
    cursor = addDays(cursor, -1);
  }
  return days;
}

type JournalPromptCadence = "daily" | "weekly" | "monthly";

interface JournalPromptDefinition {
  cadence: JournalPromptCadence;
  title: string;
  description: string;
  date: Date;
  dateStamp: string;
}

const DEFAULT_JOURNAL_PROMPT_COPY: Record<
  JournalPromptCadence,
  Pick<JournalPromptDefinition, "title" | "description">
> = {
  daily: {
    title: "Daily check-in",
    description: "Capture today's notes before the thread gets cold.",
  },
  weekly: {
    title: "Weekly reset",
    description: "Start the week with priorities, blockers, and follow-ups.",
  },
  monthly: {
    title: "Monthly intention",
    description: "Open a month-start note for themes, goals, and review points.",
  },
};

function normalizePromptCadence(value: unknown): JournalPromptCadence | null {
  if (typeof value !== "string") return null;
  const normalized = value.trim().toLowerCase();
  if (normalized === "daily" || normalized === "weekly" || normalized === "monthly") {
    return normalized;
  }
  return null;
}

export function getJournalPromptTemplateCadence(
  template: TemplateRecord,
): JournalPromptCadence | null {
  const properties = templatePropertiesAsRecord(template.properties);
  const tags = template.tags.map((tag) => tag.trim().toLowerCase());
  const propertyCadence = normalizePromptCadence(
    properties?.journal_prompt_cadence ?? properties?.journalPromptCadence ?? properties?.cadence,
  );
  if (propertyCadence) return propertyCadence;
  const tagCadence = tags.map(normalizePromptCadence).find(Boolean);
  if (tagCadence) return tagCadence;
  if (
    properties?.journal_prompt === true ||
    properties?.journalPrompt === true ||
    tags.includes("journal-prompt")
  ) {
    return "daily";
  }
  return null;
}

function getJournalPromptTemplateCopy(
  template: TemplateRecord | undefined,
  fallback: Pick<JournalPromptDefinition, "title" | "description">,
): Pick<JournalPromptDefinition, "title" | "description"> {
  if (!template) return fallback;
  const properties = templatePropertiesAsRecord(template.properties);
  const title =
    typeof properties?.journal_prompt_title === "string"
      ? properties.journal_prompt_title
      : typeof properties?.journalPromptTitle === "string"
        ? properties.journalPromptTitle
        : template.name;
  const description =
    typeof properties?.journal_prompt_description === "string"
      ? properties.journal_prompt_description
      : typeof properties?.journalPromptDescription === "string"
        ? properties.journalPromptDescription
        : template.description || fallback.description;
  return {
    title: title.trim() || fallback.title,
    description: description.trim() || fallback.description,
  };
}

export function getJournalPrompts(
  entries: JournalEntry[],
  today = new Date(),
  templates: TemplateRecord[] = [],
): JournalPrompt[] {
  const stamps = new Set(entries.map((entry) => entry.dateStamp));
  const todayDate = startOfDay(today);
  const todayStamp = formatDateStamp(todayDate);
  const weekStart = startOfWeek(todayDate);
  const monthStart = new Date(todayDate.getFullYear(), todayDate.getMonth(), 1);
  const hasThisWeek = entries.some((entry) => entry.date >= weekStart && entry.date <= todayDate);
  const hasThisMonth = entries.some((entry) => entry.date >= monthStart && entry.date <= todayDate);
  const templateByCadence = new Map<JournalPromptCadence, TemplateRecord>();
  for (const template of templates) {
    const cadence = getJournalPromptTemplateCadence(template);
    if (cadence && !templateByCadence.has(cadence)) {
      templateByCadence.set(cadence, template);
    }
  }
  const prompts: JournalPrompt[] = [];

  const pushPrompt = (definition: JournalPromptDefinition) => {
    const template = templateByCadence.get(definition.cadence);
    const copy = getJournalPromptTemplateCopy(template, definition);
    prompts.push({
      id: template ? `${definition.cadence}:${template.id}` : definition.cadence,
      title: copy.title,
      description: copy.description,
      date: definition.date,
      dateStamp: definition.dateStamp,
      ...(template ? { template } : {}),
    });
  };

  if (!stamps.has(todayStamp)) {
    pushPrompt({
      cadence: "daily",
      ...DEFAULT_JOURNAL_PROMPT_COPY.daily,
      date: todayDate,
      dateStamp: todayStamp,
    });
  }

  if (!hasThisWeek) {
    pushPrompt({
      cadence: "weekly",
      ...DEFAULT_JOURNAL_PROMPT_COPY.weekly,
      date: weekStart,
      dateStamp: formatDateStamp(weekStart),
    });
  }

  if (todayDate.getDate() <= 7 && !hasThisMonth) {
    pushPrompt({
      cadence: "monthly",
      ...DEFAULT_JOURNAL_PROMPT_COPY.monthly,
      date: monthStart,
      dateStamp: formatDateStamp(monthStart),
    });
  }

  return prompts;
}

export function selectJournalFeedEntries(
  entries: JournalEntry[],
  mode: Exclude<JournalFeedMode, "missed">,
  today = new Date(),
): JournalEntry[] {
  return mode === "week" ? getWeeklyJournalEntries(entries, today) : getRecentJournalEntries(entries);
}

export function JournalPage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [folders, setFolders] = useState<FolderRecord[]>([]);
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [anchorDate, setAnchorDate] = useState(() => new Date());
  const [query, setQuery] = useState("");
  const [feedMode, setFeedMode] = useState<JournalFeedMode>("recent");
  const [loading, setLoading] = useState(false);
  const [busyDate, setBusyDate] = useState("");
  const [error, setError] = useState("");

  const loadJournal = useCallback(async () => {
    if (!spaceId) return;
    setLoading(true);
    setError("");
    try {
      const [nextFolders, nextTemplates] = await Promise.all([
        dataClient.listFolders(spaceId),
        dataClient.listTemplates(spaceId).catch(() => []),
      ]);
      const dailyFolder = findDailyFolder(nextFolders);
      const nextDocuments = dailyFolder
        ? await dataClient.listDocuments(spaceId, { folder_id: dailyFolder.id })
        : await dataClient.listDocuments(spaceId);
      setFolders(nextFolders);
      setTemplates(nextTemplates);
      setDocuments(nextDocuments);
    } catch {
      setError("Journal entries could not be loaded.");
    } finally {
      setLoading(false);
    }
  }, [spaceId]);

  useEffect(() => {
    void loadJournal();
  }, [loadJournal]);

  const entries = useMemo(() => journalEntriesFromDocuments(documents), [documents]);
  const filteredEntries = useMemo(() => filterJournalEntries(entries, query), [entries, query]);
  const feedEntries = useMemo(
    () =>
      feedMode === "missed"
        ? []
        : selectJournalFeedEntries(filteredEntries, feedMode),
    [feedMode, filteredEntries],
  );
  const missingDays = useMemo(() => getMissingJournalDays(entries), [entries]);
  const journalPrompts = useMemo(() => getJournalPrompts(entries, new Date(), templates), [entries, templates]);
  const calendarDays = useMemo(() => buildJournalCalendar(anchorDate, entries), [anchorDate, entries]);
  const currentMonthCount = useMemo(
    () =>
      entries.filter(
        (entry) =>
          entry.date.getFullYear() === anchorDate.getFullYear() &&
          entry.date.getMonth() === anchorDate.getMonth(),
      ).length,
    [anchorDate, entries],
  );
  const streak = useMemo(() => calculateJournalStreak(entries), [entries]);
  const todayStamp = formatDateStamp(new Date());
  const todayEntry = entries.find((entry) => entry.dateStamp === todayStamp);

  const openOrCreateEntry = async (date: Date, promptTemplate?: TemplateRecord) => {
    if (!spaceId) return;
    const dateStamp = formatDateStamp(date);
    const existing = entries.find((entry) => entry.dateStamp === dateStamp);
    if (existing) {
      navigate(getJournalDocumentPath(spaceId, existing.id));
      return;
    }

    setBusyDate(dateStamp);
    setError("");
    try {
      const dailyFolder =
        findDailyFolder(folders) ?? (await dataClient.createFolder(spaceId, DAILY_FOLDER_NAME));
      const dailyTemplate = promptTemplate ?? findDailyTemplate(templates);
      const created = await dataClient.createDocument(spaceId, {
        title: dateStamp,
        folder_id: dailyFolder.id,
        content: dailyTemplate ? renderDailyTemplateContent(dailyTemplate, date) : undefined,
      });
      emitGraphDirty(spaceId, "document-created");
      await loadJournal();
      navigate(getJournalDocumentPath(spaceId, created.id));
    } catch {
      setError("Journal entry could not be created.");
    } finally {
      setBusyDate("");
    }
  };

  const moveMonth = (offset: number) => {
    setAnchorDate((date) => new Date(date.getFullYear(), date.getMonth() + offset, 1));
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-bg">
      <header className="border-border flex flex-wrap items-center gap-3 border-b px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="text-text flex items-center gap-2 text-lg font-semibold">
            <CalendarDays size={18} />
            Journal
          </div>
          <p className="text-text-muted mt-1 text-sm">
            Daily notes from the {DAILY_FOLDER_NAME} folder.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void openOrCreateEntry(new Date())}
          disabled={busyDate === todayStamp}
          className="rounded-button bg-primary text-primary-foreground hover:opacity-90 flex items-center gap-2 px-3 py-2 text-sm transition-opacity duration-100 disabled:opacity-60"
        >
          {busyDate === todayStamp ? <Loader2 size={15} className="animate-spin" /> : <Plus size={15} />}
          {todayEntry ? "Open Today" : "New Today"}
        </button>
      </header>

      <main className="min-h-0 flex-1 overflow-auto p-4">
        <div className="mx-auto grid max-w-7xl gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
          <section className="min-w-0">
            <div className="border-border bg-surface mb-4 rounded-lg border">
              <div className="border-border flex items-center justify-between border-b p-3">
                <button
                  type="button"
                  onClick={() => moveMonth(-1)}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 items-center justify-center"
                  aria-label="Previous month"
                >
                  <ChevronLeft size={16} />
                </button>
                <div className="text-center">
                  <h2 className="text-text text-base font-semibold">{monthLabel(anchorDate)}</h2>
                  <p className="text-text-muted text-xs">
                    {currentMonthCount} entries this month · {streak} day streak
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => moveMonth(1)}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 items-center justify-center"
                  aria-label="Next month"
                >
                  <ChevronRight size={16} />
                </button>
              </div>

              {journalPrompts.length > 0 && (
                <div className="border-border grid gap-2 border-b p-3 md:grid-cols-3">
                  {journalPrompts.map((prompt) => (
                    <button
                      key={prompt.id}
                      type="button"
                      onClick={() => void openOrCreateEntry(prompt.date, prompt.template)}
                      disabled={busyDate === prompt.dateStamp}
                      className="rounded-button border-border hover:bg-surface-raised flex min-h-[88px] items-start gap-2 border bg-surface p-3 text-left transition-colors duration-100 disabled:opacity-60"
                    >
                      {busyDate === prompt.dateStamp ? (
                        <Loader2 size={15} className="text-primary mt-0.5 shrink-0 animate-spin" />
                      ) : (
                        <Sparkles size={15} className="text-primary mt-0.5 shrink-0" />
                      )}
                      <span className="min-w-0">
                        <span className="text-text block text-sm font-semibold">{prompt.title}</span>
                        <span className="text-text-muted mt-1 block text-xs leading-5">
                          {prompt.description}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              )}

              <div className="grid grid-cols-7 border-b border-border text-center text-xs font-medium text-text-muted">
                {["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].map((day) => (
                  <div key={day} className="px-2 py-2">
                    {day}
                  </div>
                ))}
              </div>

              <div className="grid grid-cols-7">
                {calendarDays.map((day) => (
                  <button
                    key={day.dateStamp}
                    type="button"
                    onClick={() => void openOrCreateEntry(day.date)}
                    disabled={busyDate === day.dateStamp}
                    className={`border-border min-h-[88px] border-b border-r p-2 text-left transition-colors duration-100 last:border-r-0 hover:bg-surface-raised ${
                      day.inMonth ? "text-text" : "text-text-muted opacity-60"
                    } ${day.isToday ? "bg-primary/10" : ""}`}
                  >
                    <span className="flex items-center justify-between gap-2">
                      <span className="text-sm font-medium">{day.date.getDate()}</span>
                      {busyDate === day.dateStamp ? (
                        <Loader2 size={13} className="animate-spin" />
                      ) : day.entry ? (
                        <FileText size={13} className="text-primary" />
                      ) : null}
                    </span>
                    <span className="mt-2 block min-h-5 text-xs text-text-muted">
                      {day.entry ? "Journal entry" : "Create note"}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          </section>

          <aside className="min-w-0">
            <div className="border-border bg-surface rounded-lg border">
              <div className="border-border space-y-3 border-b p-3">
                <SearchInput
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Search journal"
                  aria-label="Search journal"
                  className="bg-surface"
                />
                <div className="flex items-center gap-2">
                  <ListFilter size={15} className="text-text-muted shrink-0" />
                  <div className="border-border bg-surface flex min-w-0 flex-1 rounded-button border p-1">
                    {[
                      ["recent", "Recent"],
                      ["week", "This week"],
                      ["missed", "Missed"],
                    ].map(([mode, label]) => (
                      <button
                        key={mode}
                        type="button"
                        onClick={() => setFeedMode(mode as JournalFeedMode)}
                        className={`rounded-button min-w-0 flex-1 px-2 py-1.5 text-xs font-medium transition-colors duration-100 ${
                          feedMode === mode
                            ? "bg-primary text-primary-foreground"
                            : "text-text-muted hover:bg-surface-raised hover:text-text"
                        }`}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
              <div className="max-h-[620px] overflow-auto p-2">
                {loading ? (
                  <div className="text-text-muted flex items-center gap-2 px-2 py-6 text-sm">
                    <Loader2 size={15} className="animate-spin" />
                    Loading journal…
                  </div>
                ) : feedMode === "missed" ? (
                  missingDays.length > 0 ? (
                    missingDays.map((day) => (
                      <button
                        key={day.dateStamp}
                        type="button"
                        onClick={() => void openOrCreateEntry(day.date)}
                        disabled={busyDate === day.dateStamp}
                        className="rounded-button hover:bg-surface-raised flex w-full items-start gap-2 px-2 py-2 text-left transition-colors duration-100 disabled:opacity-60"
                      >
                        {busyDate === day.dateStamp ? (
                          <Loader2 size={14} className="text-primary mt-0.5 shrink-0 animate-spin" />
                        ) : (
                          <CalendarPlus size={14} className="text-primary mt-0.5 shrink-0" />
                        )}
                        <span className="min-w-0">
                          <span className="text-text block truncate text-sm font-medium">
                            {day.dateStamp}
                          </span>
                          <span className="text-text-muted block text-xs">
                            {day.date.toLocaleDateString(undefined, {
                              weekday: "long",
                              month: "short",
                              day: "numeric",
                            })}
                          </span>
                        </span>
                      </button>
                    ))
                  ) : (
                    <div className="text-text-muted px-2 py-6 text-sm">
                      No missed journal days in the current lookback.
                    </div>
                  )
                ) : feedEntries.length > 0 ? (
                  feedEntries.map((entry) => (
                    <button
                      key={entry.id}
                      type="button"
                      onClick={() => {
                        if (spaceId) navigate(getJournalDocumentPath(spaceId, entry.id));
                      }}
                      className="rounded-button hover:bg-surface-raised flex w-full items-start gap-2 px-2 py-2 text-left transition-colors duration-100"
                    >
                      <FileText size={14} className="text-primary mt-0.5 shrink-0" />
                      <span className="min-w-0">
                        <span className="text-text block truncate text-sm font-medium">
                          {entry.title}
                        </span>
                        <span className="text-text-muted block text-xs">
                          {entry.date.toLocaleDateString(undefined, {
                            weekday: "long",
                            month: "short",
                            day: "numeric",
                          })}
                        </span>
                      </span>
                    </button>
                  ))
                ) : (
                  <div className="text-text-muted px-2 py-6 text-sm">
                    {query.trim()
                      ? "No matching journal entries."
                      : feedMode === "week"
                        ? "No journal entries this week."
                        : "No daily notes yet."}
                  </div>
                )}
              </div>
            </div>
            {error && <div className="text-danger mt-3 text-sm">{error}</div>}
          </aside>
        </div>
      </main>
    </div>
  );
}
