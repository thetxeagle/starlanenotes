import type { DocumentRecord } from "./dataClient";

type HierarchyPage = Pick<DocumentRecord, "id" | "title" | "parent_id">;

/** Resolve only ancestors returned by the permission-scoped page list. */
export function buildDocumentParentPath(
  documents: HierarchyPage[],
  documentId: string,
  directParentId?: string | null,
): Pick<HierarchyPage, "id" | "title">[] {
  const byId = new Map(documents.map((document) => [document.id, document]));
  let cursor = directParentId !== undefined ? directParentId : byId.get(documentId)?.parent_id;
  const seen = new Set([documentId]);
  const parents: Pick<HierarchyPage, "id" | "title">[] = [];
  while (cursor && !seen.has(cursor)) {
    seen.add(cursor);
    const parent = byId.get(cursor);
    if (!parent) break;
    parents.unshift({ id: parent.id, title: parent.title });
    cursor = parent.parent_id;
  }
  return parents;
}
