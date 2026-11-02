import type { Extensions } from "@tiptap/core";
import { runtimeBlockId } from "@/components/editor/slashEditorRuntimeBlockId";

/**
 * SlashEditor is an interaction dependency, not Starlane's document model.
 *
 * Keep every upstream extension behind this module so package upgrades cannot
 * silently take ownership of the TipTap schema, Markdown serializer, Automerge
 * boundary, comments, uploads, or Starlane-specific blocks.
 */
export const SLASH_EDITOR_PINNED_VERSION = "0.7.0";

export const STARLANE_BLOCK_ID_EXCLUSIONS = ["section", "sectionColumn"] as const;

export interface StarlaneBlockInteractionOptions {
  /** Additional structural nodes that must not receive block identity. */
  excludeBlockIdTypes?: string[];
}

/**
 * Returns only the headless identity primitive Starlane has explicitly
 * adopted. Drag remains a later, independently measured slice. Never replace
 * this with `createBlockKit()`; that kit owns a competing StarterKit schema and
 * serializer configuration.
 */
export function createStarlaneBlockInteractionExtensions({
  excludeBlockIdTypes = [],
}: StarlaneBlockInteractionOptions = {}): Extensions {
  return [
    runtimeBlockId({
      types: "auto",
      exclude: [...STARLANE_BLOCK_ID_EXCLUSIONS, ...excludeBlockIdTypes],
    }),
  ];
}
