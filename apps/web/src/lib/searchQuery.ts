import type { SearchFilters } from "@/lib/dataClient";

export interface ParsedSearchQuery {
  query: string;
  filters: SearchFilters;
  starredOnly: boolean;
}

export function formatSearchQueryInput(
  query: string,
  filters: SearchFilters = {},
  starredOnly = false,
): string {
  const parts = [query.trim()];
  if (filters.maintenance_user?.trim())
    parts.push(`maintenance-user:${formatTermValue(filters.maintenance_user)}`);
  if (filters.maintenance_group?.trim())
    parts.push(`maintenance-group:${formatTermValue(filters.maintenance_group)}`);
  if (filters.identifier?.trim()) parts.push(`identifier:${formatTermValue(filters.identifier)}`);
  if (filters.title?.trim()) parts.push(`title:${formatTermValue(filters.title)}`);
  if (filters.tag?.trim()) parts.push(`tag:${formatTermValue(filters.tag)}`);
  if (filters.space_id?.trim()) parts.push(`space:${formatTermValue(filters.space_id)}`);
  if (filters.folder?.trim()) parts.push(`folder:${formatTermValue(filters.folder)}`);
  if (filters.user?.trim()) parts.push(`user:${formatTermValue(filters.user)}`);
  if (filters.property_name?.trim()) {
    const property = filters.property_value?.trim()
      ? `${filters.property_name.trim()}=${filters.property_value.trim()}`
      : filters.property_name.trim();
    parts.push(`property:${formatTermValue(property)}`);
  }
  appendPresenceFilter(parts, "attachment", filters.has_attachment);
  appendPresenceFilter(parts, "cover", filters.has_cover);
  appendPresenceFilter(parts, "workflow", filters.has_workflow);
  appendPresenceFilter(parts, "tag", filters.has_tag);
  appendPresenceFilter(parts, "property", filters.has_property);
  if (filters.is_locked === "true") parts.push("is:locked");
  if (filters.is_locked === "false") parts.push("is:unlocked");
  if (filters.is_draft === "true") parts.push("is:draft");
  if (filters.created_on?.trim()) parts.push(`created:${formatTermValue(filters.created_on)}`);
  if (filters.updated_on?.trim()) parts.push(`updated:${formatTermValue(filters.updated_on)}`);
  if (starredOnly) parts.push("is:starred");
  return parts.filter(Boolean).join(" ");
}

export function parseSearchQueryInput(input: string): ParsedSearchQuery {
  const tokens = input.match(/\S+:"[^"]*"|"[^"]+"|\S+/g) ?? [];
  const queryParts: string[] = [];
  const filters: SearchFilters = {};
  let starredOnly = false;

  for (const token of tokens) {
    const normalized = stripQuotes(token);
    const lower = normalized.toLowerCase();
    if (lower === "and") continue;
    if (lower === "is:starred" || lower === "starred:true") {
      starredOnly = true;
      continue;
    }
    if (lower === "is:locked" || lower === "locked:true") {
      filters.is_locked = "true";
      continue;
    }
    if (lower === "is:unlocked" || lower === "locked:false") {
      filters.is_locked = "false";
      continue;
    }
    if (lower === "is:draft" || lower === "draft:true") {
      filters.is_draft = "true";
      continue;
    }
    const presenceFilter = parsePresenceFilter(lower);
    if (presenceFilter) {
      filters[presenceFilter.key] = presenceFilter.value;
      continue;
    }
    if (lower.startsWith("tag:")) {
      filters.tag = stripQuotes(normalized.slice(4).trim());
      continue;
    }
    if (lower.startsWith("maintenance-user:")) {
      filters.maintenance_user = stripQuotes(normalized.slice(17).trim());
      continue;
    }
    if (lower.startsWith("maintenance-group:")) {
      filters.maintenance_group = stripQuotes(normalized.slice(18).trim());
      continue;
    }
    if (lower.startsWith("identifier:")) {
      filters.identifier = stripQuotes(normalized.slice(11).trim());
      continue;
    }
    if (lower.startsWith("title:")) {
      filters.title = stripQuotes(normalized.slice(6).trim());
      continue;
    }
    if (lower.startsWith("space:")) {
      filters.space_id = stripQuotes(normalized.slice(6).trim());
      continue;
    }
    if (lower.startsWith("folder:")) {
      filters.folder = stripQuotes(normalized.slice(7).trim());
      continue;
    }
    if (lower.startsWith("folder_id:")) {
      filters.folder = stripQuotes(normalized.slice(10).trim());
      continue;
    }
    if (lower.startsWith("user:") || lower.startsWith("author:")) {
      const separator = normalized.indexOf(":");
      filters.user = stripQuotes(normalized.slice(separator + 1).trim());
      continue;
    }
    if (lower.startsWith("property:")) {
      const property = stripQuotes(normalized.slice(9).trim());
      const [name, value] = splitPropertyFilter(property);
      if (name) filters.property_name = name;
      if (value) filters.property_value = value;
      continue;
    }
    if (lower.startsWith("property.")) {
      const property = stripQuotes(normalized.slice(9).trim());
      const [name, value] = splitPropertyFilter(property);
      if (name) filters.property_name = name;
      if (value) filters.property_value = value;
      continue;
    }
    const createdDate = parseDateFilterValue(normalized, lower, ["created:", "created_on:"]);
    if (createdDate !== undefined) {
      if (isIsoDate(createdDate)) filters.created_on = createdDate;
      continue;
    }
    const updatedDate = parseDateFilterValue(normalized, lower, [
      "updated:",
      "updated_on:",
      "modified:",
    ]);
    if (updatedDate !== undefined) {
      if (isIsoDate(updatedDate)) filters.updated_on = updatedDate;
      continue;
    }
    queryParts.push(normalized);
  }

  return {
    query: queryParts.join(" ").trim(),
    filters: compactFilters(filters),
    starredOnly,
  };
}

function parseDateFilterValue(
  normalized: string,
  lower: string,
  prefixes: string[],
): string | undefined {
  for (const prefix of prefixes) {
    if (lower.startsWith(prefix)) {
      return stripQuotes(normalized.slice(prefix.length).trim());
    }
  }
  return undefined;
}

function formatTermValue(value: string): string {
  const trimmed = value.trim();
  return /\s/.test(trimmed) ? `"${trimmed.replace(/"/g, '\\"')}"` : trimmed;
}

type PresenceFilterKey =
  | "has_attachment"
  | "has_cover"
  | "has_workflow"
  | "has_tag"
  | "has_property";

function appendPresenceFilter(parts: string[], name: string, value: string | undefined): void {
  if (value === "true") parts.push(`has:${name}`);
  if (value === "false") parts.push(`has:no${name}`);
}

function parsePresenceFilter(
  token: string,
): { key: PresenceFilterKey; value: "true" | "false" } | undefined {
  const aliases: Record<string, PresenceFilterKey> = {
    attachment: "has_attachment",
    cover: "has_cover",
    workflow: "has_workflow",
    tag: "has_tag",
    property: "has_property",
    properties: "has_property",
  };
  const hasMatch = /^has:(no)?(attachment|cover|workflow|tag|property|properties)$/.exec(token);
  if (hasMatch) {
    return { key: aliases[hasMatch[2]], value: hasMatch[1] ? "false" : "true" };
  }
  const valueMatch = /^(attachment|cover|workflow|tag|property|properties):(true|false)$/.exec(
    token,
  );
  if (valueMatch) {
    return { key: aliases[valueMatch[1]], value: valueMatch[2] as "true" | "false" };
  }
  return undefined;
}

function splitPropertyFilter(value: string): [string, string] {
  const separatorIndex = value.search(/[:=]/);
  if (separatorIndex === -1) return [value.trim(), ""];
  return [value.slice(0, separatorIndex).trim(), value.slice(separatorIndex + 1).trim()];
}

function stripQuotes(value: string): string {
  if (value.length >= 2 && value.startsWith('"') && value.endsWith('"')) {
    return value.slice(1, -1);
  }
  return value;
}

function isIsoDate(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value);
}

function compactFilters(filters: SearchFilters): SearchFilters {
  return Object.fromEntries(
    Object.entries(filters).filter(([, value]) => typeof value === "string" && value.trim()),
  ) as SearchFilters;
}
