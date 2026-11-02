/** ordinary page Edit projection, independent from draft participation. */
export interface PageEditProjection {
  locked?: boolean;
  locked_by?: string | null;
  page_edit_allowed?: boolean;
  page_access_source?: "direct_page_grant";
  archived_at?: string | null;
  deleted_at?: string | null;
  updated_at?: string;
}
export type PageEditBlockedReason = "selection_changed" | "inactive_page" | "direct_viewer" | "account_unavailable" | "capability_unverified" | "permission_denied" | "locked";
export interface PageEditCapability {
  isLocked: boolean;
  lockOwnedByActor: boolean;
  lockBlocksActor: boolean;
  canEditPage: boolean;
  blockedReason: PageEditBlockedReason | null;
}
export function getPageEditCapability(input: {
  mode: "server" | "local";
  page: PageEditProjection | null;
  actorId?: string | null;
  /** Verified current account/server/provider/route generation, never inferred from actor text. */
  selectionCurrent: boolean;
  /** Locked-owner exception only: live GET admission for this projection, never cached fallback. */
  capabilityCurrent?: boolean;
}): PageEditCapability {
  const {mode,page,actorId,selectionCurrent,capabilityCurrent} = input;
  const isLocked = page?.locked === true;
  // Local records do not preserve lock owner identity. Never invent a local owner exemption.
  const lockOwnedByActor = mode === "server" && isLocked && !!actorId && page?.locked_by === actorId;
  const lockBlocksActor = isLocked && !lockOwnedByActor;
  let blockedReason:PageEditBlockedReason|null = null;
  if (!page || !selectionCurrent) blockedReason = "selection_changed";
  else if (page.archived_at || page.deleted_at) blockedReason = "inactive_page";
  else if (mode === "server" && page.page_access_source === "direct_page_grant") blockedReason = "direct_viewer";
  else if (mode === "server" && !actorId) blockedReason = "account_unavailable";
  else if (lockBlocksActor) blockedReason = "locked";
  else if (mode === "server" && isLocked && (!capabilityCurrent || typeof page.page_edit_allowed !== "boolean")) blockedReason = "capability_unverified";
  else if (mode === "server" && page.page_edit_allowed === false) blockedReason = "permission_denied";
  return {isLocked,lockOwnedByActor,lockBlocksActor,canEditPage:blockedReason === null,blockedReason};
}
export function pageEditBlockedExplanation(reason:PageEditBlockedReason|null): string | undefined {
  switch(reason) {
    case "selection_changed": return "Page or account selection changed. Keep your edits and reload this page before editing.";
    case "inactive_page": return "This page is archived or deleted. Keep your retained work while reviewing the page.";
    case "direct_viewer": return "This page is shared with you for viewing. Current Edit access is required.";
    case "account_unavailable": return "Sign in to the original account and refresh this page before editing.";
    case "capability_unverified": return "Current Edit access could not be verified. Keep your edits and refresh this page.";
    case "permission_denied": return "Current Edit access is unavailable. Keep your edits while reviewing access.";
    case "locked": return "This page is locked by another or unknown owner. Keep your edits and review the lock before editing.";
    default: return undefined;
  }
}

/** Authority observations must agree with visible lock/capability/lifecycle metadata. */
export function pageEditObservationFingerprint(page:PageEditProjection|null):string {
  return JSON.stringify([page?.locked,page?.locked_by,page?.page_edit_allowed,page?.archived_at,page?.deleted_at,page?.updated_at]);
}
