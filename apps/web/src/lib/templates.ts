import { isPreferredJournalTemplate, JOURNAL_TEMPLATE_KEY } from "@/lib/journalTemplatePreference";
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

export interface TemplatePresentation {
  icon?: string;
  cover_image?: string;
  custom_css?: string;
  page_width?: "standard" | "wide" | "full";
}

export const TEMPLATE_PRESENTATION_KEY = "__starlane_presentation";
export const TEMPLATE_EXPERIENCE_KEY = "__starlane_template";

export interface RenderedTemplate {
  content: string;
  cursorOffset?: number;
}

export function getTemplateEditorPath(spaceId: string, templateId?: string): string {
  const base = `/spaces/${encodeURIComponent(spaceId)}/templates`;
  return templateId ? `${base}/${encodeURIComponent(templateId)}/edit` : `${base}/new`;
}

export function findDailyTemplate(templates: TemplateRecord[]): TemplateRecord | undefined {
  return templates.find(isPreferredJournalTemplate) ?? templates.find((template) => {
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
  const record = value as Record<string, unknown>;
  const properties = { ...record };
  delete properties[TEMPLATE_PRESENTATION_KEY];
  delete properties[TEMPLATE_EXPERIENCE_KEY];
  delete properties[JOURNAL_TEMPLATE_KEY];
  return Object.keys(properties).length > 0 ? properties : undefined;
}

export function templatePresentationAsRecord(value: unknown): TemplatePresentation {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  const raw = (value as Record<string, unknown>)[TEMPLATE_PRESENTATION_KEY];
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
  const presentation = raw as Record<string, unknown>;
  return {
    icon: typeof presentation.icon === "string" && presentation.icon.trim() ? presentation.icon : undefined,
    cover_image:
      typeof presentation.cover_image === "string" && presentation.cover_image.trim()
        ? presentation.cover_image
        : undefined,
    custom_css:
      typeof presentation.custom_css === "string"
        ? presentation.custom_css
        : undefined,
    page_width:
      presentation.page_width === "standard" ||
      presentation.page_width === "wide" ||
      presentation.page_width === "full"
        ? presentation.page_width
        : undefined,
  };
}

export function withTemplatePresentation(
  properties: Record<string, unknown> | undefined,
  presentation: TemplatePresentation,
): Record<string, unknown> | undefined {
  const storedPresentation = properties?.[TEMPLATE_PRESENTATION_KEY];
  const hadStoredCss = Boolean(storedPresentation && typeof storedPresentation === "object" &&
    !Array.isArray(storedPresentation) &&
    typeof (storedPresentation as Record<string, unknown>).custom_css === "string");
  const hasPresentation = Object.entries(presentation).some(([key, value]) =>
    key === "page_width" ? Boolean(value) : key === "custom_css"
      ? typeof value === "string" && (value.length > 0 || hadStoredCss)
      : Boolean((value as string | undefined)?.trim()),
  );
  const next = { ...(properties ?? {}) };
  if (hasPresentation) {
    next[TEMPLATE_PRESENTATION_KEY] = presentation;
  } else {
    delete next[TEMPLATE_PRESENTATION_KEY];
  }
  return Object.keys(next).length > 0 ? next : undefined;
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
