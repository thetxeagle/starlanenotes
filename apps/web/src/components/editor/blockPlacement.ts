export const BLOCK_PLACEMENTS = ["left", "center", "right", "expand"] as const;
export type BlockPlacement = (typeof BLOCK_PLACEMENTS)[number];
export type TablePlacement = Exclude<BlockPlacement, "expand">;
export function normalizeBlockPlacement(value: unknown): BlockPlacement {
  const normalized = typeof value === "string" ? value.trim().toLowerCase() : "";
  return BLOCK_PLACEMENTS.includes(normalized as BlockPlacement) ? (normalized as BlockPlacement) : "center";
}
export function normalizeTablePlacement(value: unknown): TablePlacement {
  const placement = normalizeBlockPlacement(value);
  return placement === "expand" ? "center" : placement;
}
