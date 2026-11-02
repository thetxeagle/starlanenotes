/** Folder topology checks. Limits count distinct participating records, never workspace size/depth. */
export const FOLDER_OPERATION_NODE_LIMIT = 10_000;
export interface FolderHierarchyNode { id: string; space_id: string; parent_id?: string | null }
export interface FolderIndex<T extends FolderHierarchyNode> {
  byId: Map<string, T>;
  children: Map<string, T[]>;
  duplicateIds: Set<string>;
}
export interface FolderTraversalBudget { readonly size: number; visit(id: string): void }
export function createFolderTraversalBudget(): FolderTraversalBudget {
  const seen = new Set<string>();
  return {
    get size() { return seen.size; },
    visit(id) {
      if (seen.has(id)) return;
      if (seen.size >= FOLDER_OPERATION_NODE_LIMIT)
        throw new Error("Folder operation exceeds 10000 participating nodes");
      seen.add(id);
    },
  };
}
export function buildFolderIndex<T extends FolderHierarchyNode>(rows: Iterable<T>): FolderIndex<T> {
  const byId = new Map<string, T>();
  const children = new Map<string, T[]>();
  const duplicateIds = new Set<string>();
  for (const row of rows) {
    if (byId.has(row.id)) duplicateIds.add(row.id);
    byId.set(row.id, row);
    if (row.parent_id) {
      const siblings = children.get(row.parent_id) ?? [];
      siblings.push(row);
      children.set(row.parent_id, siblings);
    }
  }
  return { byId, children, duplicateIds };
}
function requireFolder<T extends FolderHierarchyNode>(index: FolderIndex<T>, spaceId: string, id: string, budget: FolderTraversalBudget): T {
  const row = index.byId.get(id);
  if (!row || row.space_id !== spaceId) throw new Error("Folder not found");
  if (!id || index.duplicateIds.has(id)) throw new Error("Folder hierarchy contains duplicate identity");
  budget.visit(id);
  return row;
}
/** Inclusive current-to-root ancestry. Missing/foreign parents and cycles refuse without repair. */
export function getFolderAncestors<T extends FolderHierarchyNode>(index: FolderIndex<T>, spaceId: string, folderId: string, budget = createFolderTraversalBudget()): T[] {
  const result: T[] = [];
  const seen = new Set<string>();
  let id: string | null | undefined = folderId;
  while (id) {
    if (seen.has(id)) throw new Error("Folder hierarchy contains a cycle");
    seen.add(id);
    const row: T = requireFolder(index, spaceId, id, budget);
    result.push(row);
    id = row.parent_id;
  }
  return result;
}
/** Root-first iterative subtree; also proves the root's existing parent chain. */
export function getFolderSubtree<T extends FolderHierarchyNode>(index: FolderIndex<T>, spaceId: string, folderId: string, budget = createFolderTraversalBudget()): T[] {
  getFolderAncestors(index, spaceId, folderId, budget);
  const result: T[] = [];
  const seen = new Set<string>();
  const pending = [folderId];
  while (pending.length) {
    const id = pending.pop()!;
    if (seen.has(id)) throw new Error("Folder hierarchy contains a cycle");
    seen.add(id);
    const row: T = requireFolder(index, spaceId, id, budget);
    result.push(row);
    const children = index.children.get(id) ?? [];
    for (let i = children.length - 1; i >= 0; i--) {
      // Charge before queueing to keep broad-frontier allocation bounded as well.
      requireFolder(index, spaceId, children[i].id, budget);
      pending.push(children[i].id);
    }
  }
  return result;
}
export function validateFolderDestination<T extends FolderHierarchyNode>(index: FolderIndex<T>, spaceId: string, parentId?: string | null, movingFolderId?: string, budget = createFolderTraversalBudget()): void {
  const ancestors = parentId ? getFolderAncestors(index, spaceId, parentId, budget) : [];
  if (!movingFolderId) return;
  const subtree = getFolderSubtree(index, spaceId, movingFolderId, budget);
  if (parentId === movingFolderId) throw new Error("Folder cannot be moved inside itself");
  const ids = new Set(subtree.map(row => row.id));
  if (ancestors.some(row => ids.has(row.id)))
    throw new Error("Folder cannot be moved inside one of its descendants");
}

/** One catalog pass for rendering. Structural facts are not mutation/access authority. */
export function buildFolderCatalog<T extends FolderHierarchyNode>(rows: Iterable<T>) {
  const index = buildFolderIndex(rows);
  const depths = new Map<string, number>();
  const order: T[] = [];
  for (const start of index.byId.values()) {
    if (depths.has(start.id)) continue;
    const trail: T[] = [], seen = new Set<string>();
    let row: T | undefined = start;
    let valid = true;
    while (row && !depths.has(row.id)) {
      if (seen.has(row.id) || index.duplicateIds.has(row.id)) { valid = false; break; }
      seen.add(row.id);
      trail.push(row);
      if (!row.parent_id) break;
      const parent = index.byId.get(row.parent_id);
      if (!parent || parent.space_id !== row.space_id) { valid = false; break; }
      row = parent;
    }
    let depth = row && depths.has(row.id) ? depths.get(row.id)! : 0;
    if (row && depths.has(row.id) && depth === 0) valid = false;
    for (let i = trail.length - 1; i >= 0; i--) {
      const current = trail[i];
      depths.set(current.id, valid ? ++depth : 0);
      if (valid) order.push(current);
    }
  }
  const children = new Map<string, T[]>(), subtreeSizes = new Map<string, number>();
  const subtreeValid = new Map<string, boolean>(), labels = new Map<string, string>();
  const roots: T[] = [];
  for (const row of order) {
    if (row.parent_id) {
      const siblings = children.get(row.parent_id) ?? [];
      siblings.push(row); children.set(row.parent_id, siblings);
    } else roots.push(row);
    subtreeSizes.set(row.id, 1);
    subtreeValid.set(row.id, (index.children.get(row.id) ?? []).every(child => !!depths.get(child.id) && child.space_id === row.space_id));
    const full = row.parent_id ? `${labels.get(row.parent_id)} / ${"name" in row ? String(row.name) : row.id}` : ("name" in row ? String(row.name) : row.id);
    labels.set(row.id, full.length > 2048 ? `…${full.slice(-2047)}` : full);
  }
  for (let i = order.length - 1; i >= 0; i--) {
    const row = order[i];
    if (!row.parent_id) continue;
    subtreeSizes.set(row.parent_id, (subtreeSizes.get(row.parent_id) ?? 1) + subtreeSizes.get(row.id)!);
    if (!subtreeValid.get(row.id)) subtreeValid.set(row.parent_id, false);
  }
  return { index, depths, order, children, roots, subtreeSizes, subtreeValid, labels };
}
/** Compute all folder destinations together; source proof/closure is traversed only once. */
export function folderCatalogMoveTargets<T extends FolderHierarchyNode>(source: ReturnType<typeof buildFolderCatalog<T>>, sourceSpace: string, folderId: string, target: ReturnType<typeof buildFolderCatalog<T>>, targetSpace: string): Set<string> {
  const budget = createFolderTraversalBudget();
  const closure = getFolderSubtree(source.index, sourceSpace, folderId, budget);
  const participating = new Set([...closure.map(row => row.id), ...getFolderAncestors(source.index, sourceSpace, folderId, budget).map(row => row.id)]);
  const moved = new Set(closure.map(row => row.id));
  const additional = new Map<string, number>(), eligible = new Set<string>();
  const currentParent = source.index.byId.get(folderId)?.parent_id;
  for (const row of target.order) {
    const count = (row.parent_id ? additional.get(row.parent_id) ?? 0 : 0) + (participating.has(row.id) ? 0 : 1);
    additional.set(row.id, count);
    if (row.space_id === targetSpace && !moved.has(row.id) && !(sourceSpace === targetSpace && row.id === currentParent) && budget.size + count <= FOLDER_OPERATION_NODE_LIMIT) eligible.add(row.id);
  }
  return eligible;
}
