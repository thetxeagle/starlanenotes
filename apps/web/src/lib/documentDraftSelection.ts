export interface DocumentDraftSelectionCandidate {
  id: string;
  access_role: string;
  updated_at: string;
}

/**
 * Select an explicitly requested accessible draft, or the current user's newest
 * owned draft. Shared editor and viewer drafts are never automatic fallbacks.
 * An undefined result means no owned draft exists, or the explicit ID is absent
 * from the accessible list; callers can distinguish those cases by the request.
 */
export function selectDocumentDraft<TDraft extends DocumentDraftSelectionCandidate>(
  drafts: readonly TDraft[],
  requestedDraftId?: string | null,
): TDraft | undefined {
  if (requestedDraftId) {
    return drafts.find((draft) => draft.id === requestedDraftId);
  }

  const ownedDrafts = drafts.filter((draft) => draft.access_role === "owner");
  return [...ownedDrafts].sort(compareOwnedDrafts)[0];
}

/**
 * Editing may resume an owned draft automatically, but an accessible shared
 * draft must remain an explicit user choice. Creating a private fork before
 * that choice can discard the only valid unpublished source context.
 */
export function shouldChooseAccessibleDraft(
  drafts: readonly DocumentDraftSelectionCandidate[],
): boolean {
  return !selectDocumentDraft(drafts) && drafts.length > 0;
}

function compareOwnedDrafts(
  left: DocumentDraftSelectionCandidate,
  right: DocumentDraftSelectionCandidate,
): number {
  const leftUpdatedAt = Date.parse(left.updated_at);
  const rightUpdatedAt = Date.parse(right.updated_at);
  const leftTime = Number.isNaN(leftUpdatedAt) ? Number.NEGATIVE_INFINITY : leftUpdatedAt;
  const rightTime = Number.isNaN(rightUpdatedAt) ? Number.NEGATIVE_INFINITY : rightUpdatedAt;

  if (leftTime !== rightTime) return rightTime - leftTime;
  return left.id.localeCompare(right.id);
}
