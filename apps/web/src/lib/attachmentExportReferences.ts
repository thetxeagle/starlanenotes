import { rewriteExactAssetReferences } from "./nativeArchiveReferences";
/** Actual Markdown/portable reference spans, excluding code and filename guesses. */
export function referencedPinnedAttachmentIds(content: string): string[] {
  const ids = new Set<string>();
  const candidates = new Set(content.match(/starlane-attachment:\/\/[^\s<>()"'\\]+/g) ?? []);
  for (const reference of candidates) {
    if (rewriteExactAssetReferences(content, new Map([[reference, "owned-export-reference"]])) === content) continue;
    const id = reference.slice("starlane-attachment://".length);
    if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id)) throw new Error("Attachment export requires an exact supported version ID");
    ids.add(id);
  }
  return [...ids];
}
