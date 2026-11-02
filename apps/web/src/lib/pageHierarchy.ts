/** Page topology checks. Limits count distinct participating records, never workspace size/depth. */
export const PAGE_OPERATION_NODE_LIMIT = 10_000;
export interface PageHierarchyNode { id: string; space_id: string; folder_id?: string | null; parent_id?: string | null }
export interface PageIndex<T extends PageHierarchyNode> {
  byId: Map<string, T>;
  children: Map<string, T[]>;
  duplicateIds: Set<string>;
}
export interface PageTraversalBudget { readonly size: number; visit(id: string): void }
export function createPageTraversalBudget(): PageTraversalBudget {
  const seen = new Set<string>();
  return {
    get size() { return seen.size; },
    visit(id) {
      if (seen.has(id)) return;
      if (seen.size >= PAGE_OPERATION_NODE_LIMIT)
        throw new Error("Page operation exceeds 10000 participating nodes");
      seen.add(id);
    },
  };
}
export function buildPageIndex<T extends PageHierarchyNode>(rows: Iterable<T>): PageIndex<T> {
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
function requirePage<T extends PageHierarchyNode>(index: PageIndex<T>, spaceId: string, id: string, budget: PageTraversalBudget): T {
  const row = index.byId.get(id);
  if (!row || row.space_id !== spaceId) throw new Error("Page not found");
  if (!id || index.duplicateIds.has(id)) throw new Error("Page hierarchy contains duplicate identity");
  budget.visit(id);
  return row;
}
/** Inclusive current-to-root ancestry. Missing/foreign parents and cycles refuse without repair. */
export function getPageAncestors<T extends PageHierarchyNode>(index: PageIndex<T>, spaceId: string, pageId: string, budget = createPageTraversalBudget()): T[] {
  const result: T[] = [];
  const seen = new Set<string>();
  const folderId = requirePage(index, spaceId, pageId, budget).folder_id ?? null;
  let id: string | null | undefined = pageId;
  while (id) {
    if (seen.has(id)) throw new Error("Page hierarchy contains a cycle");
    seen.add(id);
    const row: T = requirePage(index, spaceId, id, budget);
    if ((row.folder_id ?? null) !== folderId) throw new Error("Child pages must stay in the same folder as their parent");
    result.push(row);
    id = row.parent_id;
  }
  return result;
}
/** Root-first iterative subtree; also proves the root's existing parent chain. */
export function getPageSubtree<T extends PageHierarchyNode>(index: PageIndex<T>, spaceId: string, pageId: string, budget = createPageTraversalBudget()): T[] {
  getPageAncestors(index, spaceId, pageId, budget);
  const result: T[] = [];
  const seen = new Set<string>();
  const pending = [pageId];
  while (pending.length) {
    const id = pending.pop()!;
    if (seen.has(id)) throw new Error("Page hierarchy contains a cycle");
    seen.add(id);
    const row: T = requirePage(index, spaceId, id, budget);
    result.push(row);
    const children = index.children.get(id) ?? [];
    for (let i = children.length - 1; i >= 0; i--) {
      // Charge before queueing to keep broad-frontier allocation bounded as well.
      const child = requirePage(index, spaceId, children[i].id, budget);
      if ((child.folder_id ?? null) !== (row.folder_id ?? null)) throw new Error("Child pages must stay in the same folder as their parent");
      pending.push(children[i].id);
    }
  }
  return result;
}
export function validatePageDestination<T extends PageHierarchyNode>(index: PageIndex<T>, spaceId: string, folderId: string | null | undefined, parentId?: string | null, movingPageId?: string, budget = createPageTraversalBudget()): void {
  const ancestors = parentId ? getPageAncestors(index, spaceId, parentId, budget) : [];
  if (ancestors.some(row => (row.folder_id ?? null) !== (folderId ?? null))) throw new Error("Child pages must stay in the same folder as their parent");
  if (!movingPageId) return;
  const subtree = getPageSubtree(index, spaceId, movingPageId, budget);
  if (parentId === movingPageId) throw new Error("Page cannot be moved inside itself");
  const ids = new Set(subtree.map(row => row.id));
  if (ancestors.some(row => ids.has(row.id)))
    throw new Error("Page cannot be moved inside one of its descendants");
}

/** One catalog pass for rendering. Structural facts are not mutation/access authority. */
export function buildPageCatalog<T extends PageHierarchyNode>(rows: Iterable<T>) {
  const index = buildPageIndex(rows);
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
      if (!parent || parent.space_id !== row.space_id || (parent.folder_id ?? null) !== (row.folder_id ?? null)) { valid = false; break; }
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
    subtreeValid.set(row.id, (index.children.get(row.id) ?? []).every(child => !!depths.get(child.id) && child.space_id === row.space_id && (child.folder_id ?? null) === (row.folder_id ?? null)));
    const full = row.parent_id ? `${labels.get(row.parent_id)} / ${"title" in row ? String(row.title) : row.id}` : ("title" in row ? String(row.title) : row.id);
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
/** Compute all page destinations together; source proof/closure is traversed only once. */
export function pageCatalogMoveTargets<T extends PageHierarchyNode>(source: ReturnType<typeof buildPageCatalog<T>>, sourceSpace: string, pageId: string, target: ReturnType<typeof buildPageCatalog<T>>, targetSpace: string, targetFolder?: string | null): Set<string> {
  const budget = createPageTraversalBudget();
  const closure = getPageSubtree(source.index, sourceSpace, pageId, budget);
  const participating = new Set([...closure.map(row => row.id), ...getPageAncestors(source.index, sourceSpace, pageId, budget).map(row => row.id)]);
  const moved = new Set(closure.map(row => row.id));
  const additional = new Map<string, number>(), eligible = new Set<string>();
  const currentParent = source.index.byId.get(pageId)?.parent_id;
  for (const row of target.order) {
    const count = (row.parent_id ? additional.get(row.parent_id) ?? 0 : 0) + (participating.has(row.id) ? 0 : 1);
    additional.set(row.id, count);
    if (row.space_id === targetSpace && (targetFolder === undefined || (row.folder_id ?? null) === targetFolder) && !moved.has(row.id) && !(sourceSpace === targetSpace && row.id === currentParent) && budget.size + count <= PAGE_OPERATION_NODE_LIMIT) eligible.add(row.id);
  }
  return eligible;
}
