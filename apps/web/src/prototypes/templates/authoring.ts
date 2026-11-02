import type { JSONContent } from "@tiptap/core";

export type AuthoringFieldKind = "text" | "multiline" | "date" | "dropdown";

export interface AuthoringField {
  id: string;
  label: string;
  kind: AuthoringFieldKind;
  required: boolean;
  options: string[];
  defaultValue: string;
}

export interface WritingGuidance {
  id: string;
  section: string;
  prompt: string;
}

export interface AuthoringTemplate {
  id: string;
  name: string;
  description: string;
  fields: AuthoringField[];
  content: string;
  contentJson?: JSONContent;
  guidance: WritingGuidance[];
}

export interface TemplatePageSnapshot {
  id: string;
  title: string;
  content: string;
  contentJson?: JSONContent;
  properties: Record<string, string>;
  bindings: Array<{ marker: string; value: string }>;
  bindingsResolved: boolean;
}

export type FieldValues = Record<string, string>;

const field = (
  id: string,
  label: string,
  kind: AuthoringFieldKind = "text",
  options: string[] = [],
  defaultValue = "",
): AuthoringField => ({ id, label, kind, options, defaultValue, required: false });

const guidance = (id: string, section: string, prompt: string): WritingGuidance => ({
  id,
  section,
  prompt,
});

export function createBlankTemplate(id = "blank"): AuthoringTemplate {
  return { id, name: "Untitled template", description: "", fields: [], content: "", guidance: [] };
}

export const EXAMPLE_TEMPLATES: AuthoringTemplate[] = [
  {
    id: "recipe",
    name: "Recipe",
    description: "Capture ingredients, timing, and the steps that make a dish work.",
    fields: [
      field("recipe_category", "Category", "dropdown", ["Breakfast", "Main", "Side", "Dessert"]),
      field("prep_time", "Prep time", "text", [], "15 minutes"),
      field("cook_time", "Cook time", "text", [], "30 minutes"),
      field("servings", "Servings", "text", [], "4"),
      field("recipe_ingredient", "Ingredient list", "multiline"),
      field("first_step", "First method step", "multiline"),
    ],
    content:
      "## Ingredients\n\n- {{Ingredient list}}\n\n## Method\n\n1. {{First method step}}\n\n## Notes\n",
    guidance: [
      guidance("recipe-ingredients", "Ingredients", "List each ingredient with its amount."),
      guidance("recipe-method", "Method", "Write clear steps in the order they happen."),
      guidance(
        "recipe-notes",
        "Notes",
        "Record substitutions, storage, or what you would change next time.",
      ),
    ],
  },
  {
    id: "blog-post",
    name: "Blog Post",
    description: "Shape a post from its working title through a final reader takeaway.",
    fields: [
      field("post_title", "Working title", "text"),
      field("post_date", "Publish date", "date"),
      field("post_status", "Status", "dropdown", ["Idea", "Draft", "Ready to publish"]),
      field("opening_line", "Opening line", "multiline"),
    ],
    content: "## Opening\n\n{{Opening line}}\n\n## Main points\n\n## Takeaway\n",
    guidance: [
      guidance("blog-opening", "Opening", "Give the reader a reason to keep going."),
      guidance("blog-points", "Main points", "Develop the central idea with examples or evidence."),
      guidance("blog-takeaway", "Takeaway", "Leave the reader with one clear conclusion."),
    ],
  },
];

export function addField(template: AuthoringTemplate, nextId: string): AuthoringTemplate {
  const count = template.fields.length + 1;
  return { ...template, fields: [...template.fields, field(nextId, `New field ${count}`)] };
}

export function addGuidance(template: AuthoringTemplate, nextId: string): AuthoringTemplate {
  const count = template.guidance.length + 1;
  return {
    ...template,
    guidance: [...template.guidance, guidance(nextId, `Section ${count}`, "")],
  };
}

export function initialFieldValues(template: AuthoringTemplate): FieldValues {
  return Object.fromEntries(template.fields.map(({ id, defaultValue }) => [id, defaultValue]));
}

export function isValidDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const date = new Date(`${value}T00:00:00Z`);
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value;
}

export function validateDefinition(template: AuthoringTemplate): string[] {
  const errors: string[] = [];
  const labels = template.fields.map(({ label }) => label.trim().toLowerCase());
  if (labels.some((label) => !label || /[{}]/.test(label)))
    errors.push("Give each field a label without braces");
  if (new Set(labels).size !== labels.length) errors.push("Field labels must be unique");
  for (const field of template.fields) {
    const options = field.options.map((option) => option.trim()).filter(Boolean);
    if (field.kind === "dropdown" && (!options.length || new Set(options).size !== options.length))
      errors.push(`${field.label}: add distinct dropdown options`);
    if (field.defaultValue && field.kind === "date" && !isValidDate(field.defaultValue))
      errors.push(`${field.label}: suggested answer must be a valid date`);
    if (field.defaultValue && field.kind === "dropdown" && !options.includes(field.defaultValue))
      errors.push(`${field.label}: suggested answer must match an option`);
  }
  return errors;
}

export function validateFields(template: AuthoringTemplate, values: FieldValues): string[] {
  const errors: string[] = [];
  const labels = template.fields.map(({ label }) => label.trim().toLowerCase()).filter(Boolean);
  if (new Set(labels).size !== labels.length) errors.push("Field labels must be unique");
  for (const field of template.fields) {
    const value = values[field.id] ?? "";
    const label = field.label || "Untitled field";
    if (field.required && !value.trim()) errors.push(label);
    else if (value && field.kind === "date" && !isValidDate(value)) errors.push(label);
    else if (
      value &&
      field.kind === "dropdown" &&
      !field.options.map((option) => option.trim()).includes(value)
    )
      errors.push(label);
  }
  return [...new Set(errors)];
}

export function createPageSnapshot(
  template: AuthoringTemplate,
  values: FieldValues,
  title: string,
  nextId: string,
): TemplatePageSnapshot {
  return {
    id: nextId,
    title: title.trim() || template.name,
    // The page editor replaces only known binding markers with literal text nodes
    // after Markdown parsing. Keeping markers here avoids parsing field answers as Markdown.
    content: template.content,
    contentJson: template.contentJson ? structuredClone(template.contentJson) : undefined,
    properties: Object.fromEntries(
      template.fields.map(({ id, label }) => [label, values[id] ?? ""]),
    ),
    bindings: template.fields.map(({ label, id }) => ({
      marker: `{{${label.trim()}}}`,
      value: values[id] ?? "",
    })),
    bindingsResolved: false,
  };
}

export function renameFieldBindings(
  template: AuthoringTemplate,
  fieldId: string,
  nextLabel: string,
): AuthoringTemplate {
  const previous = template.fields.find(({ id }) => id === fieldId)?.label.trim();
  if (previous === undefined || previous === nextLabel.trim())
    return {
      ...template,
      fields: template.fields.map((item) =>
        item.id === fieldId ? { ...item, label: nextLabel } : item,
      ),
    };
  const escaped = previous.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const tokenPattern = new RegExp(`\\{\\{\\s*${escaped}\\s*\\}\\}`, "gi");
  return {
    ...template,
    fields: template.fields.map((item) =>
      item.id === fieldId ? { ...item, label: nextLabel } : item,
    ),
    content: template.content.replace(tokenPattern, () => `{{${nextLabel.trim()}}}`),
    contentJson: undefined,
  };
}

export function removeField(template: AuthoringTemplate, fieldId: string): AuthoringTemplate {
  const label = template.fields.find(({ id }) => id === fieldId)?.label.trim();
  if (!label) return { ...template, fields: template.fields.filter(({ id }) => id !== fieldId) };
  const escaped = label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const tokenPattern = new RegExp(`\\{\\{\\s*${escaped}\\s*\\}\\}`, "gi");
  return {
    ...template,
    fields: template.fields.filter(({ id }) => id !== fieldId),
    content: template.content.replace(tokenPattern, ""),
    contentJson: undefined,
  };
}
