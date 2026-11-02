import type { TemplateRecord } from "@/lib/dataClient";
export const JOURNAL_TEMPLATE_KEY = "__starlane_journal";
export function isPreferredJournalTemplate(template: Pick<TemplateRecord, "properties">): boolean {
  const properties = template.properties;
  if (!properties || typeof properties !== "object" || Array.isArray(properties)) return false;
  const value = (properties as Record<string, unknown>)[JOURNAL_TEMPLATE_KEY];
  return Boolean(
    value && typeof value === "object" && (value as Record<string, unknown>).daily === true,
  );
}
export function withJournalTemplatePreference(
  properties: unknown,
  preferred: boolean,
): Record<string, unknown> {
  const next =
    properties && typeof properties === "object" && !Array.isArray(properties)
      ? { ...(properties as Record<string, unknown>) }
      : {};
  if (preferred) next[JOURNAL_TEMPLATE_KEY] = { daily: true };
  else delete next[JOURNAL_TEMPLATE_KEY];
  return next;
}
