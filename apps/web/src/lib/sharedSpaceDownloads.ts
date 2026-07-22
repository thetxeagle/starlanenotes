const SHARED_SPACE_DOWNLOADS_KEY = "starlane_shared_space_downloads";

export interface SharedSpaceDownloadCheckpoint {
  spaceId: string;
  spaceName: string;
  slug?: string;
  sourceServerUrl?: string;
  downloadedByUserId?: string;
  downloadedByDisplayName?: string;
  downloadedAt: string;
  documentCount: number;
  folderCount: number;
  tagCount: number;
  propertyCount: number;
}

export function readSharedSpaceDownloadCheckpoints(): SharedSpaceDownloadCheckpoint[] {
  if (typeof window === "undefined") return [];
  const raw = window.localStorage.getItem(SHARED_SPACE_DOWNLOADS_KEY);
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map(normalizeCheckpoint)
      .filter((checkpoint): checkpoint is SharedSpaceDownloadCheckpoint => checkpoint !== null)
      .sort((a, b) => (a.downloadedAt < b.downloadedAt ? 1 : -1));
  } catch {
    return [];
  }
}

export function writeSharedSpaceDownloadCheckpoints(
  checkpoints: SharedSpaceDownloadCheckpoint[],
): void {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(SHARED_SPACE_DOWNLOADS_KEY, JSON.stringify(checkpoints));
}

export function recordSharedSpaceDownloadCheckpoint(
  checkpoint: SharedSpaceDownloadCheckpoint,
): SharedSpaceDownloadCheckpoint[] {
  const existing = readSharedSpaceDownloadCheckpoints().filter(
    (item) => item.spaceId !== checkpoint.spaceId,
  );
  const next = [checkpoint, ...existing].slice(0, 50);
  writeSharedSpaceDownloadCheckpoints(next);
  return next;
}

export function findSharedSpaceDownloadCheckpoint(
  checkpoints: SharedSpaceDownloadCheckpoint[],
  spaceId: string,
): SharedSpaceDownloadCheckpoint | null {
  return checkpoints.find((checkpoint) => checkpoint.spaceId === spaceId) ?? null;
}

export function formatSharedSpaceDownloadCheckpoint(
  checkpoint: SharedSpaceDownloadCheckpoint,
): string {
  return `${checkpoint.documentCount} doc${checkpoint.documentCount === 1 ? "" : "s"} downloaded ${formatDownloadDate(checkpoint.downloadedAt)}`;
}

function normalizeCheckpoint(value: unknown): SharedSpaceDownloadCheckpoint | null {
  if (!value || typeof value !== "object") return null;
  const item = value as Partial<SharedSpaceDownloadCheckpoint>;
  if (
    typeof item.spaceId !== "string" ||
    typeof item.spaceName !== "string" ||
    typeof item.downloadedAt !== "string"
  ) {
    return null;
  }
  return {
    spaceId: item.spaceId,
    spaceName: item.spaceName,
    slug: typeof item.slug === "string" ? item.slug : undefined,
    sourceServerUrl: normalizeOptionalString(item.sourceServerUrl),
    downloadedByUserId: normalizeOptionalString(item.downloadedByUserId),
    downloadedByDisplayName: normalizeOptionalString(item.downloadedByDisplayName),
    downloadedAt: item.downloadedAt,
    documentCount: typeof item.documentCount === "number" ? item.documentCount : 0,
    folderCount: typeof item.folderCount === "number" ? item.folderCount : 0,
    tagCount: typeof item.tagCount === "number" ? item.tagCount : 0,
    propertyCount: typeof item.propertyCount === "number" ? item.propertyCount : 0,
  };
}

function normalizeOptionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function formatDownloadDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}
