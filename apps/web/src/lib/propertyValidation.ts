/** Mirror server property_validation.rs without normalizing wire values. */
export interface PropertyDefinition {
  name: string;
  property_type: string;
  options?: unknown;
}
export const PROPERTY_TYPES = [
  "text",
  "long_text",
  "number",
  "date",
  "datetime",
  "checkbox",
  "select",
  "multi_select",
  "rating",
  "url",
  "user",
  "relation",
];
const bytes = (value: string) => new TextEncoder().encode(value).length;

function optionsFor(options: unknown): string[] {
  if (options == null) return [];
  const values = Array.isArray(options) ? options : (options as { options?: unknown }).options;
  if (
    !Array.isArray(values) ||
    values.some((value) => typeof value !== "string" || !value.trim()) ||
    new Set(values).size !== values.length
  )
    throw new Error("Select options must be an array of unique nonempty strings");
  return values;
}

export function validatePropertySchema(schema: PropertyDefinition): void {
  if (!schema.name.trim() || bytes(schema.name) > 500)
    throw new Error("Property name must contain 1 to 500 bytes");
  if (!PROPERTY_TYPES.includes(schema.property_type))
    throw new Error(`Invalid property type: ${schema.property_type}`);
  if (["select", "multi_select"].includes(schema.property_type)) optionsFor(schema.options);
}

function validDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const date = new Date(`${value}T00:00:00Z`);
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value;
}

export function validatePropertyValue(schema: PropertyDefinition, value: string | null): void {
  if (!PROPERTY_TYPES.includes(schema.property_type))
    throw new Error(`Invalid property type: ${schema.property_type}`);
  if (value === null || value === "") return;
  if (typeof value !== "string" || bytes(value) > 100_000)
    throw new Error("Property value must be a string of at most 100,000 bytes");
  let valid = true;
  switch (schema.property_type) {
    case "number":
      valid =
        /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value) &&
        Number.isFinite(Number(value));
      break;
    case "date":
      valid = validDate(value);
      break;
    case "datetime": {
      const match =
        /^(\d{4}-\d{2}-\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?(Z|[+-]\d{2}:\d{2})?$/.exec(
          value,
        );
      valid =
        !!match &&
        validDate(match[1]) &&
        Number(match[2]) < 24 &&
        Number(match[3]) < 60 &&
        Number(match[4] ?? 0) < 60 &&
        (!match[5] || match[4] !== undefined) &&
        Number.isFinite(Date.parse(value));
      break;
    }
    case "checkbox":
      valid = value === "true" || value === "false";
      break;
    case "rating":
      valid = /^[0-5]$/.test(value);
      break;
    case "url":
      try {
        const url = new URL(value);
        valid = ["http:", "https:"].includes(url.protocol) && !!url.hostname;
      } catch {
        valid = false;
      }
      break;
    case "select":
      valid = optionsFor(schema.options).includes(value);
      break;
    case "multi_select": {
      const options = optionsFor(schema.options);
      try {
        const selected: unknown = JSON.parse(value);
        valid =
          Array.isArray(selected) &&
          selected.every((item) => typeof item === "string" && options.includes(item)) &&
          new Set(selected).size === selected.length;
      } catch {
        valid = false;
      }
      break;
    }
  }
  if (!valid)
    throw new Error(
      `Property value does not match ${schema.property_type}; clear it or supply a valid value`,
    );
}

export function validatePropertySchemaChange(
  previous: PropertyDefinition,
  next: PropertyDefinition,
  values: (string | null)[],
): void {
  validatePropertySchema(next);
  if (
    previous.property_type !== next.property_type &&
    values.some((value) => value !== null && value !== "")
  )
    throw new Error(
      "Property type cannot change while nonempty values exist; clear values or use an explicit conversion",
    );
  for (const value of values) validatePropertyValue(next, value);
}
