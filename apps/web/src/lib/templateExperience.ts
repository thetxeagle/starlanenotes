import type { PropertySchemaRecord, TemplateRecord } from "@/lib/dataClient";
import { parseTemplateMarker } from "@/lib/templateMarkers";
import { serializeTemplateValue } from "@/lib/templateValues";
import {
  renderTemplateContent,
  templatePresentationAsRecord,
  templatePropertiesAsRecord,
  type TemplatePresentation,
} from "@/lib/templates";

export const TEMPLATE_EXPERIENCE_KEY = "__starlane_template";

export type TemplateFieldType = "text" | "multiline" | "date" | "dropdown";

export interface TemplateFieldBinding {
  target: "property";
  property_id: string;
}

export interface TemplateFieldDefinition {
  id: string;
  label: string;
  type: TemplateFieldType;
  help_text?: string;
  default_value?: string;
  required?: boolean;
  options?: string[];
  bindings?: TemplateFieldBinding[];
}

export type TemplateTitlePart =
  | { kind: "text"; value: string }
  | { kind: "field"; field_id: string };

export interface TemplateExperienceConfig {
  schema_version: 1;
  fields: TemplateFieldDefinition[];
  title?: TemplateTitlePart[];
}

export type TemplateFieldValues = Record<string, string>;

export interface TemplateRenderContext {
  date?: Date;
  title?: string;
  space?: string;
  author?: string;
  properties?: Record<string, unknown>;
}

export interface ResolvedTemplateExperience {
  title?: string;
  content: string;
  property_values: Array<{ property_id: string; value: string }>;
  property_defaults: Record<string, unknown>;
  presentation: TemplatePresentation;
  unresolvedFields: Array<{ id: string; label: string; reason: string }>;
  guidance: Array<{ id: string; text: string }>;
}

const FIELD_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9_-]{0,159}$/;

/** Reads the reserved envelope from either a template record or its properties value. */
export function getTemplateExperience(
  value: TemplateRecord | unknown,
): TemplateExperienceConfig {
  const properties = isTemplateRecord(value) ? value.properties : value;
  const raw = properties && typeof properties === "object" && !Array.isArray(properties)
    ? (properties as Record<string, unknown>)[TEMPLATE_EXPERIENCE_KEY]
    : undefined;
  return normalizeTemplateExperience(raw);
}

/** Returns a validated v1 config, using an empty config for legacy templates. */
export function normalizeTemplateExperience(value: unknown): TemplateExperienceConfig {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return { schema_version: 1, fields: [] };
  }
  const raw = value as Record<string, unknown>;
  if (raw.schema_version !== 1 || !Array.isArray(raw.fields)) {
    return { schema_version: 1, fields: [] };
  }
  const fields = raw.fields.flatMap((candidate) => {
    if (!candidate || typeof candidate !== "object" || Array.isArray(candidate)) return [];
    const field = candidate as Record<string, unknown>;
    if (
      typeof field.id !== "string" || !FIELD_ID_PATTERN.test(field.id) ||
      typeof field.label !== "string" || !field.label.trim() ||
      !isFieldType(field.type)
    ) return [];
    const options = Array.isArray(field.options)
      ? field.options.filter((option): option is string => typeof option === "string")
      : undefined;
    const bindings = Array.isArray(field.bindings)
      ? field.bindings.flatMap((binding) => {
          if (!binding || typeof binding !== "object" || Array.isArray(binding)) return [];
          const item = binding as Record<string, unknown>;
          return item.target === "property" && typeof item.property_id === "string" && item.property_id
            ? [{ target: "property" as const, property_id: item.property_id }]
            : [];
        })
      : undefined;
    return [{
      id: field.id,
      label: field.label.trim(),
      type: field.type,
      ...(typeof field.help_text === "string" && field.help_text ? { help_text: field.help_text } : {}),
      ...(typeof field.default_value === "string" ? { default_value: field.default_value } : {}),
      ...(typeof field.required === "boolean" ? { required: field.required } : {}),
      ...(options ? { options } : {}),
      ...(bindings ? { bindings } : {}),
    }];
  });
  const ids = new Set<string>();
  const uniqueFields = fields.filter((field) => {
    if (ids.has(field.id)) return false;
    ids.add(field.id);
    return true;
  });
  const title: TemplateTitlePart[] = [];
  if (Array.isArray(raw.title)) {
    for (const part of raw.title) {
      if (!part || typeof part !== "object" || Array.isArray(part)) continue;
      const item = part as Record<string, unknown>;
      if (item.kind === "text" && typeof item.value === "string") {
        title.push({ kind: "text", value: item.value });
        continue;
      }
      if (item.kind === "field" && typeof item.field_id === "string" && ids.has(item.field_id)) {
        title.push({ kind: "field", field_id: item.field_id });
      }
    }
  }
  return {
    schema_version: 1,
    fields: uniqueFields,
    ...(title.length ? { title } : {}),
  };
}

/** Resolves named fields without creating a page or changing template storage. */
export function resolveTemplateExperience({
  template,
  properties: inputProperties,
  content,
  config,
  context = {},
  fieldValues = {},
  schemas = [],
}: {
  template?: TemplateRecord;
  properties?: unknown;
  content?: string;
  config?: TemplateExperienceConfig;
  context?: TemplateRenderContext;
  fieldValues?: TemplateFieldValues;
  schemas?: PropertySchemaRecord[];
}): ResolvedTemplateExperience {
  const sourceContent = content ?? template?.content ?? "";
  const properties = template?.properties ?? inputProperties;
  const fieldConfig = config ?? getTemplateExperience(properties);
  const fields = normalizeTemplateExperience(fieldConfig).fields;
  const fieldsById = new Map(fields.map((field) => [field.id, field]));
  const values = new Map<string, string>();
  const unresolvedFields: ResolvedTemplateExperience["unresolvedFields"] = [];

  for (const field of fields) {
    const hasSuppliedValue = Object.prototype.hasOwnProperty.call(fieldValues, field.id);
    const rawValue = hasSuppliedValue ? fieldValues[field.id] ?? "" : field.default_value ?? "";
    const boundSchema = (field.bindings ?? [])
      .map((binding) => schemas.find((candidate) => candidate.id === binding.property_id))
      .find(Boolean);
    const normalized = validateFieldValue(
      field,
      rawValue,
      field.type === "dropdown" && boundSchema
        ? (Array.isArray(boundSchema.options) ? boundSchema.options.filter((option): option is string => typeof option === "string") : [])
        : field.options,
    );
    if (normalized.error) {
      unresolvedFields.push({ id: field.id, label: field.label, reason: normalized.error });
      continue;
    }
    if (field.required && !normalized.value.trim()) {
      unresolvedFields.push({ id: field.id, label: field.label, reason: "A value is required." });
      continue;
    }
    values.set(field.id, normalized.value);
  }

  const property_values: ResolvedTemplateExperience["property_values"] = [];
  const propertyIds = new Set<string>();
  const fieldByProperty = new Map<string, string>();
  for (const field of fields) {
    for (const binding of field.bindings ?? []) {
      const schema = schemas.find((candidate) => candidate.id === binding.property_id);
      if (!schema || !isCompatibleProperty(field.type, schema.property_type)) {
        if (!unresolvedFields.some((item) => item.id === field.id)) {
          unresolvedFields.push({
            id: field.id,
            label: field.label,
            reason: schema ? `The property “${schema.name}” is incompatible with this field.` : "The bound property is missing.",
          });
        }
        continue;
      }
      const previousFieldId = fieldByProperty.get(schema.id);
      if (previousFieldId && previousFieldId !== field.id) {
        unresolvedFields.push({ id: field.id, label: field.label, reason: `Property “${schema.name}” is already bound to another field.` });
        continue;
      }
      if (propertyIds.has(schema.id)) continue;
      const value = values.get(field.id) ?? "";
      if (new TextEncoder().encode(value).byteLength > 100_000 || property_values.length >= 100) {
        unresolvedFields.push({
          id: field.id,
          label: field.label,
          reason: property_values.length >= 100 ? "A page supports at most 100 field property assignments." : "This property value exceeds 100,000 bytes. Shorten the response.",
        });
        continue;
      }
      propertyIds.add(schema.id);
      fieldByProperty.set(schema.id, field.id);
      property_values.push({ property_id: schema.id, value });
    }
  }

  const propertyDefaults = templatePropertiesAsRecord(properties) ?? {};
  const legacyResolved = renderTemplateContent(sourceContent, {
    ...context,
    properties: context.properties ?? propertyDefaults,
  }).content;
  const guidance: ResolvedTemplateExperience["guidance"] = [];
  const renderedContent = replaceBodyMarkers(legacyResolved, (line) => {
    const marker = parseTemplateMarker(line.trim());
    if (!marker || marker.kind === "opaque") return line;
    if (marker.kind === "guidance") {
      guidance.push({ id: marker.id, text: marker.text });
      return line;
    }
    const field = fieldsById.get(marker.field_id);
    if (!field) {
      unresolvedFields.push({ id: marker.field_id, label: marker.field_id, reason: "This body marker points to a missing field." });
      return line;
    }
    const value = values.get(field.id);
    if (value === undefined) return line;
    return serializeTemplateValue(value) + (line.endsWith("\n") ? "\n" : "");
  });

  const title = fieldConfig.title?.length
    ? fieldConfig.title.map((part) => part.kind === "text" ? part.value : values.get(part.field_id) ?? "").join("")
    : undefined;
  return {
    title,
    content: renderedContent,
    property_values,
    property_defaults: propertyDefaults,
    presentation: templatePresentationAsRecord(properties),
    unresolvedFields,
    guidance,
  };
}

function validateFieldValue(field: TemplateFieldDefinition, value: string, options?: string[]): { value: string; error?: string } {
  if (field.type === "dropdown" && value && !options?.includes(value)) {
    return { value, error: "Choose one of the configured options." };
  }
  if (field.type === "date" && value && !isValidIsoDate(value)) {
    return { value, error: "Enter a valid date." };
  }
  return { value: field.type === "multiline" ? value : value.replace(/[\r\n]+/g, " ").trim() };
}

export function getTemplateFieldOptions(field: TemplateFieldDefinition, schemas: PropertySchemaRecord[]): string[] {
  if (field.type !== "dropdown") return [];
  const binding = field.bindings?.find((item) => item.target === "property");
  const schema = binding ? schemas.find((candidate) => candidate.id === binding.property_id) : undefined;
  if (!schema) return field.options ?? [];
  if (!Array.isArray(schema.options)) return [];
  return schema.options.filter((option): option is string => typeof option === "string");
}

function isValidIsoDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const [year, month, day] = value.split("-").map(Number);
  const date = new Date(Date.UTC(year, month - 1, day));
  return date.getUTCFullYear() === year && date.getUTCMonth() === month - 1 && date.getUTCDate() === day;
}

function replaceBodyMarkers(content: string, replace: (line: string) => string): string {
  const lines = content.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  let fence: { character: string; length: number } | null = null;
  return lines.map((line) => {
    const text = line.replace(/\n$/, "");
    const fenceMatch = text.match(/^ {0,3}(`{3,}|~{3,})/);
    if (fence) {
      if (fenceMatch && fenceMatch[1][0] === fence.character && fenceMatch[1].length >= fence.length && /^ {0,3}(?:`+|~+)[ \t]*$/.test(text)) fence = null;
      return line;
    }
    if (fenceMatch) {
      fence = { character: fenceMatch[1][0], length: fenceMatch[1].length };
      return line;
    }
    if (/^(?: {4}| {0,3}\t)/.test(text)) return line;
    return replace(line);
  }).join("");
}

function isCompatibleProperty(field: TemplateFieldType, property: string): boolean {
  if (field === "text" || field === "multiline") return property === "text";
  if (field === "date") return property === "date";
  return property === "select";
}

function isFieldType(value: unknown): value is TemplateFieldType {
  return value === "text" || value === "multiline" || value === "date" || value === "dropdown";
}

function isTemplateRecord(value: unknown): value is TemplateRecord {
  return Boolean(value && typeof value === "object" && !Array.isArray(value) && "content" in value && "name" in value);
}
