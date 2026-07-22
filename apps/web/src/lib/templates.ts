import type { FolderRecord, TemplateRecord } from "@/lib/dataClient";

const DAILY_TEMPLATE_NAMES = new Set(["_daily", "daily", "daily note"]);
export const DAILY_FOLDER_NAME = "_daily";

export interface TemplateRenderContext {
  date?: Date;
  title?: string;
  space?: string;
  author?: string;
  properties?: Record<string, unknown>;
}

export interface RenderedTemplate {
  content: string;
  cursorOffset?: number;
}

export function findDailyTemplate(templates: TemplateRecord[]): TemplateRecord | undefined {
  return templates.find((template) => {
    const name = template.name.trim().toLowerCase();
    return DAILY_TEMPLATE_NAMES.has(name) || template.tags.some((tag) => tag.toLowerCase() === "daily");
  });
}

export function findDailyFolder<T extends Pick<FolderRecord, "id" | "name">>(
  folders: T[],
): T | undefined {
  return folders.find((folder) => folder.name.trim().toLowerCase() === DAILY_FOLDER_NAME);
}

export function renderDailyTemplateContent(template: TemplateRecord, date: Date): string {
  return renderTemplateContent(template.content, { date }).content;
}

export function renderTemplateContent(
  content: string,
  context: TemplateRenderContext = {},
): RenderedTemplate {
  const date = context.date ?? new Date();
  let cursorOffset: number | undefined;
  let rendered = "";
  let sourceOffset = 0;
  const placeholderPattern = /\{\{\s*([\w.]+)\s*\}\}/g;

  for (const match of content.matchAll(placeholderPattern)) {
    const rawKey = match[1];
    const matchIndex = match.index ?? 0;
    rendered += content.slice(sourceOffset, matchIndex);
    sourceOffset = matchIndex + match[0].length;

    const key = rawKey.toLowerCase();
    if (key === "cursor") {
      cursorOffset = rendered.length;
      continue;
    }

    let value: string | undefined;
    if (key.startsWith("property.")) {
      const propertyName = rawKey.slice("property.".length);
      const propertyValue = context.properties?.[propertyName];
      value = propertyValue == null ? "" : String(propertyValue);
    } else {
      value = renderTemplateValue(key, date, context);
    }

    rendered += value ?? match[0];
  }

  rendered += content.slice(sourceOffset);

  return { content: rendered, cursorOffset };
}

export function templatePropertiesAsRecord(value: unknown): Record<string, unknown> | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  return value as Record<string, unknown>;
}

export function formatDateStamp(date = new Date()): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function renderTemplateValue(
  key: string,
  date: Date,
  context: TemplateRenderContext,
): string | undefined {
  switch (key) {
    case "date":
      return date.toLocaleDateString(undefined, {
        weekday: "long",
        year: "numeric",
        month: "long",
        day: "numeric",
      });
    case "isodate":
      return formatDateStamp(date);
    case "yesterday":
      return formatLocalizedDate(addLocalDays(date, -1));
    case "tomorrow":
      return formatLocalizedDate(addLocalDays(date, 1));
    case "yesterdayisodate":
      return formatDateStamp(addLocalDays(date, -1));
    case "tomorrowisodate":
      return formatDateStamp(addLocalDays(date, 1));
    case "weekday":
      return date.toLocaleDateString(undefined, { weekday: "long" });
    case "month":
      return date.toLocaleDateString(undefined, { month: "long" });
    case "year":
      return String(date.getFullYear());
    case "time":
      return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
    case "datetime":
      return date.toLocaleString();
    case "title":
      return context.title ?? "";
    case "space":
      return context.space ?? "";
    case "author":
      return context.author ?? "";
    default:
      return undefined;
  }
}

function addLocalDays(date: Date, days: number): Date {
  const next = new Date(date);
  next.setDate(next.getDate() + days);
  return next;
}

function formatLocalizedDate(date: Date): string {
  return date.toLocaleDateString(undefined, {
    weekday: "long",
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}
