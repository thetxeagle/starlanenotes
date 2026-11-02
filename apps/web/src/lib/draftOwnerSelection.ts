import { verifyDraftSaveSelection, type DraftSaveSelection } from "./dataClient";

/** Accepted list/dialog provenance, not a fresh-current authority enrollment. */
export async function runDraftOwnerSelection<T>(original: DraftSaveSelection | null, action: () => Promise<T>): Promise<T> {
  if (!original) throw new Error("Reload Drafts to establish the original selected workspace before managing this draft");
  verifyDraftSaveSelection(original);
  const result = await action();
  verifyDraftSaveSelection(original);
  return result;
}
