import {
  validatePageCssProvenance,
  retainedPageCssTextLength,
  type PageCssProvenance,
} from "./pageCssProvenance";

export const DOCUMENT_DRAFT_RECOVERY_STORAGE_PREFIX = "starlane_document_draft_recovery_v1";
const MAX_RECOVERY_TEXT_CHARS = 2_000_000;

export interface DocumentDraftRecoveryScope {
  storageMode: "local" | "server";
  authIdentity: string;
  serverTarget: string;
  spaceId: string;
  documentId: string;
  draftId: string;
}

export interface DocumentDraftRecoveryContent {
  title: string;
  content: string;
  synopsis: string;
  pageWidth: "standard" | "wide" | "full";
  customCss: string;
  /** Exact retained source evidence; absent on legacy checkpoints. Never inferred from rendered CSS. */
  cssProvenance?: PageCssProvenance;
  /** Missing on legacy checkpoints: keep the current draft presentation. */
  icon?: string | null;
  coverImage?: string | null;
}

/** Exact original raw draft bytes, separate from the locally edited checkpoint. */
export interface DocumentDraftRecoverySource {
  draftId: string;
  sourceDigest?: string;
  title: string;
  content: string;
  icon: string | null;
  coverImage: string | null;
}

export type DocumentDraftRecoverySourceComparison = "matching" | "changed" | "legacy-unverified";

export interface DocumentDraftRecoverySnapshot extends DocumentDraftRecoveryContent {
  snapshotId: string;
  capturedAt: string;
  baseDraftUpdatedAt: string;
  baseDraftSource?: DocumentDraftRecoverySource;
}

/** Confirm every checkpoint field before clearing its exact ID after a save. */
export function documentDraftRecoveryMatchesSaved(
  snapshot: DocumentDraftRecoverySnapshot,
  saved: { title: string; content: string; icon?: string | null; cover_image?: string | null },
  preparedCheckpoint: { title: string; content: string },
): boolean {
  if (
    (snapshot.cssProvenance !== undefined && !validatePageCssProvenance(snapshot.cssProvenance)) ||
    typeof preparedCheckpoint.title !== "string" ||
    typeof preparedCheckpoint.content !== "string" ||
    saved.title !== preparedCheckpoint.title ||
    saved.content !== preparedCheckpoint.content
  )
    return false;
  const presentationMatches = (
    retained: string | null | undefined,
    acknowledged: string | null | undefined,
  ) => {
    if (retained === undefined) return true;
    if (retained !== null && typeof retained !== "string") return false;
    // Backend preserves nonempty presentation bytes and normalizes blank removal.
    const expected = retained === null || !retained.trim() ? null : retained;
    return (acknowledged ?? null) === expected;
  };
  return (
    presentationMatches(snapshot.icon, saved.icon) &&
    presentationMatches(snapshot.coverImage, saved.cover_image)
  );
}

export type DocumentDraftRecoverySaveOutcome = "confirmed" | "queued";

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

function getStorage(): StorageLike | null {
  try {
    return typeof globalThis.localStorage === "undefined" ? null : globalThis.localStorage;
  } catch {
    return null;
  }
}

function hasValidScope(scope: DocumentDraftRecoveryScope): boolean {
  return Object.values(scope).every((value) => typeof value === "string" && value.length > 0);
}

function storageKey(scope: DocumentDraftRecoveryScope): string {
  const parts = [
    scope.storageMode,
    scope.serverTarget,
    scope.authIdentity,
    scope.spaceId,
    scope.documentId,
    scope.draftId,
  ];
  return `${DOCUMENT_DRAFT_RECOVERY_STORAGE_PREFIX}:${parts.map(encodeURIComponent).join(":")}`;
}

function newSnapshotId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `draft-recovery-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

function validSourceDigest(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
}

function isDraftSource(value: unknown): value is DocumentDraftRecoverySource {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const source = value as Partial<DocumentDraftRecoverySource>;
  return (
    typeof source.draftId === "string" &&
    source.draftId.length > 0 &&
    (source.sourceDigest === undefined || validSourceDigest(source.sourceDigest)) &&
    typeof source.title === "string" &&
    typeof source.content === "string" &&
    (source.icon === null || typeof source.icon === "string") &&
    (source.coverImage === null || typeof source.coverImage === "string") &&
    source.title.length +
      source.content.length +
      (source.icon?.length ?? 0) +
      (source.coverImage?.length ?? 0) <=
      MAX_RECOVERY_TEXT_CHARS
  );
}

/** Timestamp equality never establishes original-source equality. */
export function compareDocumentDraftRecoverySource(
  snapshot: DocumentDraftRecoverySnapshot,
  selectedDraft: { id: string; source_digest?: string | null },
): DocumentDraftRecoverySourceComparison {
  const source = snapshot.baseDraftSource;
  if (!source || !isDraftSource(source)) return "legacy-unverified";
  if (source.draftId !== selectedDraft.id) return "changed";
  if (!validSourceDigest(source.sourceDigest) || !validSourceDigest(selectedDraft.source_digest)) {
    return "legacy-unverified";
  }
  return source.sourceDigest === selectedDraft.source_digest ? "matching" : "changed";
}

function isSnapshot(value: unknown): value is DocumentDraftRecoverySnapshot {
  if (!value || typeof value !== "object") return false;
  const snapshot = value as Partial<DocumentDraftRecoverySnapshot>;
  return (
    typeof snapshot.snapshotId === "string" &&
    typeof snapshot.capturedAt === "string" &&
    typeof snapshot.baseDraftUpdatedAt === "string" &&
    typeof snapshot.title === "string" &&
    typeof snapshot.content === "string" &&
    typeof snapshot.synopsis === "string" &&
    (snapshot.pageWidth === "standard" ||
      snapshot.pageWidth === "wide" ||
      snapshot.pageWidth === "full") &&
    typeof snapshot.customCss === "string" &&
    hasBoundedContent(snapshot) &&
    (snapshot.baseDraftSource === undefined || isDraftSource(snapshot.baseDraftSource))
  );
}

function hasBoundedContent(content: {
  title?: unknown;
  content?: unknown;
  synopsis?: unknown;
  customCss?: unknown;
  cssProvenance?: unknown;
  icon?: unknown;
  coverImage?: unknown;
}): boolean {
  return (
    typeof content.title === "string" &&
    typeof content.content === "string" &&
    typeof content.synopsis === "string" &&
    typeof content.customCss === "string" &&
    (content.cssProvenance === undefined || validatePageCssProvenance(content.cssProvenance)) &&
    (content.icon === undefined || content.icon === null || typeof content.icon === "string") &&
    (content.coverImage === undefined ||
      content.coverImage === null ||
      typeof content.coverImage === "string") &&
    content.title.length <= MAX_RECOVERY_TEXT_CHARS &&
    content.content.length <= MAX_RECOVERY_TEXT_CHARS &&
    content.synopsis.length <= MAX_RECOVERY_TEXT_CHARS &&
    content.customCss.length <= MAX_RECOVERY_TEXT_CHARS &&
    content.title.length +
      content.content.length +
      content.synopsis.length +
      content.customCss.length +
      (typeof content.icon === "string" ? content.icon.length : 0) +
      (typeof content.coverImage === "string" ? content.coverImage.length : 0) +
      (content.cssProvenance === undefined ? 0 : retainedPageCssTextLength(content.cssProvenance)) <=
      MAX_RECOVERY_TEXT_CHARS
  );
}

function scopeMatches(
  left: DocumentDraftRecoveryScope,
  right: DocumentDraftRecoveryScope,
): boolean {
  return (
    left.storageMode === right.storageMode &&
    left.authIdentity === right.authIdentity &&
    left.serverTarget === right.serverTarget &&
    left.spaceId === right.spaceId &&
    left.documentId === right.documentId &&
    left.draftId === right.draftId
  );
}

function readStoredRecord(
  scope: DocumentDraftRecoveryScope,
  storage: StorageLike,
  retainedRaw?: string | null,
): { scope: DocumentDraftRecoveryScope; snapshot: DocumentDraftRecoverySnapshot } | null {
  try {
    const raw = retainedRaw === undefined ? storage.getItem(storageKey(scope)) : retainedRaw;
    if (!raw) return null;
    const record = JSON.parse(raw) as {
      scope?: DocumentDraftRecoveryScope;
      snapshot?: unknown;
    };
    if (
      !record.scope ||
      !hasValidScope(record.scope) ||
      !scopeMatches(record.scope, scope) ||
      !isSnapshot(record.snapshot) ||
      (record.snapshot.baseDraftSource !== undefined &&
        record.snapshot.baseDraftSource.draftId !== scope.draftId)
    ) {
      return null;
    }
    return { scope: record.scope, snapshot: record.snapshot };
  } catch {
    return null;
  }
}

/** Store the newest unsaved draft state before its network debounce can run. */
export function persistDocumentDraftRecovery(
  scope: DocumentDraftRecoveryScope,
  content: DocumentDraftRecoveryContent,
  baseDraftUpdatedAt: string,
  storage: StorageLike | null = getStorage(),
  capturedAt = new Date().toISOString(),
  baseDraftSource?: DocumentDraftRecoverySource,
): string | null {
  if (
    !storage ||
    !hasValidScope(scope) ||
    !baseDraftUpdatedAt ||
    typeof content.title !== "string" ||
    typeof content.content !== "string" ||
    typeof content.synopsis !== "string" ||
    typeof content.customCss !== "string" ||
    (content.pageWidth !== "standard" &&
      content.pageWidth !== "wide" &&
      content.pageWidth !== "full") ||
    !hasBoundedContent(content) ||
    (baseDraftSource !== undefined &&
      (!isDraftSource(baseDraftSource) || baseDraftSource.draftId !== scope.draftId))
  ) {
    return null;
  }
  // Rewriting unsaved edits cannot rebase them onto a newer confirmed source.
  // A legacy checkpoint stays unverified until it is explicitly cleared/discarded.
  let existing: DocumentDraftRecoverySnapshot | undefined;
  try {
    const raw = storage.getItem(storageKey(scope));
    const record = readStoredRecord(scope, storage, raw);
    // An unreadable original must remain available for explicit raw recovery.
    // Missing and corrupt checkpoints are deliberately different write states.
    if (raw !== null && !record) return null;
    existing = record?.snapshot;
  } catch {
    return null;
  }
  const cssProvenance = content.cssProvenance ?? existing?.cssProvenance;
  if (!hasBoundedContent({ ...content, cssProvenance })) return null;
  const originalSource = existing ? existing.baseDraftSource : baseDraftSource;
  const snapshot: DocumentDraftRecoverySnapshot = {
    ...content,
    ...(cssProvenance === undefined ? {} : { cssProvenance }),
    snapshotId: newSnapshotId(),
    capturedAt,
    baseDraftUpdatedAt: existing?.baseDraftUpdatedAt ?? baseDraftUpdatedAt,
    ...(originalSource === undefined
      ? {}
      : {
          baseDraftSource: {
            draftId: originalSource.draftId,
            ...(originalSource.sourceDigest === undefined
              ? {}
              : { sourceDigest: originalSource.sourceDigest }),
            title: originalSource.title,
            content: originalSource.content,
            icon: originalSource.icon,
            coverImage: originalSource.coverImage,
          },
        }),
  };
  try {
    storage.setItem(storageKey(scope), JSON.stringify({ scope, snapshot }));
    return snapshot.snapshotId;
  } catch {
    return null;
  }
}

/** Read only after the caller has confirmed access to this exact draft. */
export function readDocumentDraftRecovery(
  scope: DocumentDraftRecoveryScope,
  storage: StorageLike | null = getStorage(),
): DocumentDraftRecoverySnapshot | null {
  if (!storage || !hasValidScope(scope)) return null;
  return readStoredRecord(scope, storage)?.snapshot ?? null;
}

/**
 * Clear only the exact checkpoint whose save was confirmed by the server or
 * durably queued in the session-scoped offline mutation store.
 */
export function clearDocumentDraftRecovery(
  scope: DocumentDraftRecoveryScope,
  snapshotId: string,
  outcome: DocumentDraftRecoverySaveOutcome,
  storage: StorageLike | null = getStorage(),
): boolean {
  if (
    !storage ||
    !hasValidScope(scope) ||
    !snapshotId ||
    (outcome !== "confirmed" && outcome !== "queued")
  ) {
    return false;
  }
  return removeMatchingSnapshot(scope, snapshotId, storage);
}

/** Discard only the recovery checkpoint the user explicitly chose to keep/discard. */
export function discardDocumentDraftRecovery(
  scope: DocumentDraftRecoveryScope,
  snapshotId: string,
  storage: StorageLike | null = getStorage(),
): boolean {
  if (!storage || !hasValidScope(scope) || !snapshotId) return false;
  return removeMatchingSnapshot(scope, snapshotId, storage);
}

function removeMatchingSnapshot(
  scope: DocumentDraftRecoveryScope,
  snapshotId: string,
  storage: StorageLike,
): boolean {
  const current = readStoredRecord(scope, storage);
  if (!current || current.snapshot.snapshotId !== snapshotId) return false;
  try {
    storage.removeItem(storageKey(scope));
    return true;
  } catch {
    return false;
  }
}
