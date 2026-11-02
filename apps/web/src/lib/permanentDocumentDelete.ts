import { cleanupAttemptFromAcknowledgedMutation, retainAcknowledgedCleanup } from "./permanentDocumentCleanup";
import { api, captureOfflineServerContext, getApiBase } from "./api";
import { acknowledgeOfflineServerMutation, listOfflineServerAttachments, listOfflineServerMutations, queueOfflineServerMutation, retainUnknownOfflineServerMutation, updateOfflineServerMutation, type QueuedServerMutation } from "./offlineServerCache";
import { sameOfflineReplayOrigin, verifyOfflineReplayContext } from "./offlineReplayOrigin";
import { canonicalDeleteOperation, requirePermanentDeleteTarget } from "./documentDeleteIntent";

/** Journal before dispatch; exact retry reuses operation and witness, never a current target. */
async function deleteOriginalServerDocument(space: string, document: string, target: {created_at?: string; deleted_at?: string | null}, expected?:QueuedServerMutation): Promise<void> {
  if (!space || !document || /[/?#]/.test(space + document)) throw new Error("Original deletion target is invalid");
  const body = {original_target: requirePermanentDeleteTarget(target)};
  const context = captureOfflineServerContext();
  const path = `/spaces/${space}/documents/${document}/trash`;
  const retained = listOfflineServerMutations(context.origin.serverBase).filter(row => row.path === path);
  if (retained.length > 1) throw new Error("Conflicting retained deletion attempts require review");
  let operation = retained[0];
  if (operation && (operation.method !== "DELETE" || !canonicalDeleteOperation(operation.deletionOperationId) || !sameOfflineReplayOrigin(operation.origin, context.origin) || JSON.stringify(operation.body) !== JSON.stringify(body))) throw new Error("Retained deletion belongs to its original page and session; review that attempt before deleting another incarnation");
  if(expected && JSON.stringify(operation)!==JSON.stringify(expected))throw new Error("Original named deletion evidence changed or is absent; no new operation may be created");
  if (!operation) operation = queueOfflineServerMutation(context.origin.serverBase, path, "DELETE", body, context.origin.cacheScope, context.origin, "unknown", crypto.randomUUID());
  const original = JSON.parse(JSON.stringify(operation)) as typeof operation;
  const current = listOfflineServerMutations(context.origin.serverBase).find(row => row.id === original.id);
  if (JSON.stringify(current) !== JSON.stringify(original)) throw new Error("Deletion journal changed before dispatch; retain it for review");
  // Never leapfrog an earlier unresolved mutation or attachment dependency.
  const earlier = listOfflineServerMutations(context.origin.serverBase).some(row => row.id !== original.id && (row.queueOrder ?? 0) < (original.queueOrder ?? 0));
  if (earlier || listOfflineServerAttachments(context.origin.serverBase).length > 0) throw new Error("Earlier pending work must be reviewed before permanent deletion");
  try {
    await api.fetchDocumentDelete(context, path, original.deletionOperationId!, original.body);
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    // Persist attachment cleanup identity before removing the metadata intent.
    await retainAcknowledgedCleanup(cleanupAttemptFromAcknowledgedMutation(original), () => verifyOfflineReplayContext(context, getApiBase(), api.getToken()));
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    acknowledgeOfflineServerMutation(original.id, original);
  } catch (error) {
    try { updateOfflineServerMutation({...original, replayState: "unknown", attempts: original.attempts + 1, lastError: "Original deletion outcome unknown; retry only this exact retained page and session"}, original); } catch { retainUnknownOfflineServerMutation(original, "Concurrent deletion journal change; original dispatched UUID and witness retained for explicit review"); }
    throw error;
  }
}

export async function permanentlyDeleteOriginalServerDocument(space: string, document: string, target: {created_at?: string; deleted_at?: string | null}): Promise<void> {
  if (!navigator.locks?.request) throw new Error("This browser cannot serialize original deletion attempts. Retain this page and use a supported browser before retrying.");
  return navigator.locks.request(`starlane-original-delete:${getApiBase()}:${space}:${document}`, () => deleteOriginalServerDocument(space, document, target));
}

/** Only the selected retained evidence can reconcile; absence never allocates a UUID. */
export async function reconcileRetainedServerDocumentDelete(original:QueuedServerMutation):Promise<void> {
 const row=JSON.parse(JSON.stringify(original)) as QueuedServerMutation;
 const match=/^\/spaces\/([^/?#]+)\/documents\/([^/?#]+)\/trash$/.exec(row.path);
 if(!match || row.method!=="DELETE" || !canonicalDeleteOperation(row.deletionOperationId))throw new Error("Legacy deletion evidence requires original target review; no guessed retry is allowed");
 const target=(row.body as {original_target?:{created_at?:string;deleted_at?:string|null}})?.original_target;
 if(!target)throw new Error("Original named deletion witness unavailable; retain evidence");requirePermanentDeleteTarget(target);
 if(!navigator.locks?.request)throw new Error("Original deletion reconciliation requires cross-window serialization");
 return navigator.locks.request(`starlane-original-delete:${getApiBase()}:${match[1]}:${match[2]}`,()=>deleteOriginalServerDocument(match[1],match[2],target,row));
}
