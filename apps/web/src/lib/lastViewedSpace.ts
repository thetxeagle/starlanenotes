const LAST_VIEWED_SPACE_KEY = "starlane_last_viewed_space_id";

function storage(): Storage | null {
  if (typeof window === "undefined") return null;
  return window.localStorage;
}

export function readLastViewedSpaceId(): string | null {
  const value = storage()?.getItem(LAST_VIEWED_SPACE_KEY)?.trim() ?? "";
  return value || null;
}

export function writeLastViewedSpaceId(spaceId: string): void {
  const next = spaceId.trim();
  if (!next) return;
  storage()?.setItem(LAST_VIEWED_SPACE_KEY, next);
}

export function clearLastViewedSpaceId(): void {
  storage()?.removeItem(LAST_VIEWED_SPACE_KEY);
}

export function buildSpaceHomePath(spaceId: string): string {
  return `/spaces/${encodeURIComponent(spaceId)}/docs`;
}

export function getLastViewedSpaceHomePath(fallbackSpaceId?: string): string {
  const spaceId = fallbackSpaceId?.trim() || readLastViewedSpaceId();
  return spaceId ? buildSpaceHomePath(spaceId) : "/";
}

export interface SpaceIdentity {
  id: string;
}

export function getPreferredSpaceHomePath(
  spaces: SpaceIdentity[],
  preferredSpaceId = readLastViewedSpaceId(),
): string | null {
  const preferred = preferredSpaceId?.trim();
  const selected =
    (preferred ? spaces.find((space) => space.id === preferred) : undefined) ?? spaces[0];
  return selected ? buildSpaceHomePath(selected.id) : null;
}
