export type SwipeAction = "pin" | "delete" | null;

export interface RecentKeyInput {
  id: string;
  spaceId: string;
}

export interface RecentSortable extends RecentKeyInput {
  updated_at?: string;
}

export function getRecentKey(item: RecentKeyInput): string {
  return `${item.spaceId}:${item.id}`;
}

export function getSwipeAction(dx: number, dy: number, threshold = 70): SwipeAction {
  if (Math.abs(dx) < threshold || Math.abs(dx) <= Math.abs(dy)) {
    return null;
  }
  return dx > 0 ? "pin" : "delete";
}

export function sortRecentItems<T extends RecentSortable>(
  list: T[],
  pinnedMap: Record<string, true>,
): T[] {
  return [...list].sort((a, b) => {
    const keyA = getRecentKey(a);
    const keyB = getRecentKey(b);
    const pinnedA = !!pinnedMap[keyA];
    const pinnedB = !!pinnedMap[keyB];
    if (pinnedA !== pinnedB) return pinnedA ? -1 : 1;

    const timeA = a.updated_at ? new Date(a.updated_at).getTime() : 0;
    const timeB = b.updated_at ? new Date(b.updated_at).getTime() : 0;
    return timeB - timeA;
  });
}
