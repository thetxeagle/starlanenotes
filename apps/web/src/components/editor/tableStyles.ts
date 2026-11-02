import { normalizeTablePlacement, type TablePlacement } from "./blockPlacement";
export const TABLE_STYLE_IDS = ["directory", "striped", "compact", "grid"] as const;

export type TableStyleId = (typeof TABLE_STYLE_IDS)[number];

export interface TableStylePreset {
  description: string;
  id: TableStyleId;
  label: string;
}

export const TABLE_STYLE_PRESETS: readonly TableStylePreset[] = [
  {
    id: "directory",
    label: "Directory",
    description: "Quiet header and clean row separators.",
  },
  {
    id: "striped",
    label: "Striped",
    description: "Alternating rows for longer datasets.",
  },
  {
    id: "compact",
    label: "Compact",
    description: "Dense spacing for reference tables.",
  },
  {
    id: "grid",
    label: "Grid",
    description: "Visible cell boundaries for matrices.",
  },
] as const;

const TABLE_STYLE_DIRECTIVE =
  /^<!--\s*starlane-table:\s*([\w-]+)(?:\s+align=(left|center|right))?\s*-->$/i;

export function normalizeTableStyle(value: unknown): TableStyleId {
  return typeof value === "string" &&
    TABLE_STYLE_IDS.includes(value.trim().toLowerCase() as TableStyleId)
    ? (value.trim().toLowerCase() as TableStyleId)
    : "directory";
}

export function parseTableStyleDirective(line: string): TableStyleId | null {
  const match = line.trim().match(TABLE_STYLE_DIRECTIVE);
  return match ? normalizeTableStyle(match[1]) : null;
}

export function parseTablePlacementDirective(line: string): TablePlacement {
  return normalizeTablePlacement(line.trim().match(TABLE_STYLE_DIRECTIVE)?.[2]);
}

export function serializeTableStyleDirective(style: unknown, placement?: unknown): string {
  const normalized = normalizeTableStyle(style);
  const alignment = normalizeTablePlacement(placement);
  return normalized === "directory" && alignment === "center"
    ? ""
    : `<!-- starlane-table: ${normalized}${alignment === "center" ? "" : ` align=${alignment}`} -->`;
}
