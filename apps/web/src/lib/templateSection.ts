import type { PropertySchemaRecord, TemplateRecord } from "@/lib/dataClient";
import { Slice, type Fragment } from "prosemirror-model";
import type { EditorState, Transaction } from "prosemirror-state";
import { closeHistory } from "prosemirror-history";
import { getTemplateExperience, getTemplateFieldOptions, resolveTemplateExperience, type TemplateFieldValues, type TemplateRenderContext } from "@/lib/templateExperience";
import { parseTemplateMarker } from "@/lib/templateMarkers";

/** Returns only fields with a live body marker, excluding examples in code. */
export function getTemplateSectionFieldIds(content: string): Set<string> {
  const fieldIds = new Set<string>();
  let fence: { character: string; length: number } | null = null;

  for (const line of content.match(/[^\n]*\n|[^\n]+$/g) ?? []) {
    const text = line.replace(/\n$/, "");
    const fenceMatch = text.match(/^ {0,3}(`{3,}|~{3,})/);
    if (fence) {
      if (
        fenceMatch &&
        fenceMatch[1][0] === fence.character &&
        fenceMatch[1].length >= fence.length &&
        /^ {0,3}(?:`+|~+)[ \t]*$/.test(text)
      ) fence = null;
      continue;
    }
    if (fenceMatch) {
      fence = { character: fenceMatch[1][0], length: fenceMatch[1].length };
      continue;
    }
    if (/^(?: {4}| {0,3}\t)/.test(text)) continue;
    const marker = parseTemplateMarker(text.trim());
    if (marker?.kind === "field") fieldIds.add(marker.field_id);
  }

  return fieldIds;
}

/** Builds an insertion-only field contract without title or page-property bindings. */
export function getTemplateSectionFields(
  template: Pick<TemplateRecord, "content" | "properties">,
  schemas: PropertySchemaRecord[] = [],
) {
  const config = getTemplateExperience(template.properties);
  const bodyFieldIds = getTemplateSectionFieldIds(template.content);
  return config.fields
    .filter((field) => bodyFieldIds.has(field.id))
    .map((field) => ({
      ...field,
      ...(field.type === "dropdown" ? { options: getTemplateFieldOptions(field, schemas) } : {}),
      bindings: undefined,
    }));
}

export function resolveTemplateSection({
  template,
  context = {},
  fieldValues = {},
  schemas = [],
}: {
  template: TemplateRecord;
  context?: TemplateRenderContext;
  fieldValues?: TemplateFieldValues;
  schemas?: PropertySchemaRecord[];
}) {
  const config = {
    schema_version: 1 as const,
    fields: getTemplateSectionFields(template, schemas),
  };
  const resolved = resolveTemplateExperience({ template, config, context, fieldValues });
  return { ...resolved, fields: config.fields };
}

/** Creates one ProseMirror replacement transaction for a slash-command range. */
export function createTemplateSectionInsertionTransaction(
  state: EditorState,
  range: { from: number; to: number },
  content: Fragment,
): Transaction | null {
  if (
    !Number.isInteger(range.from) ||
    !Number.isInteger(range.to) ||
    range.from < 0 ||
    range.to < range.from ||
    range.to > state.doc.content.size
  ) return null;
  return closeHistory(state.tr.replaceRange(range.from, range.to, new Slice(content, 0, 0)));
}
