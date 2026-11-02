import {readCleanupAttempts,type CleanupAttempt} from "./permanentDocumentCleanup";
import {captureOfflineServerContext,getApiBase,isTauri} from "./api";
import {getCurrentLaunchMode} from "./launchMode";
import {listOfflineServerMutations,type QueuedServerMutation} from "./offlineServerCache";
import {sameOfflineReplayOrigin} from "./offlineReplayOrigin";
import {canonicalDeleteOperation,deleteRoute,originalDeleteTarget} from "./documentDeleteIntent";
import {listPendingNativeDocumentDeletes,describeCurrentNativeDeletionSelection,type NativeDeleteAttempt} from "./nativeDocumentDelete";
import {captureNativeArchiveLocalSession} from "./nativeArchiveCurrentContext";
export interface PendingDocumentDeletion {kind:"server"|"native"|"cleanup";id:string;operation:string;space:string;document:string;createdAt:string;deletedAt:string;originalRoot?:string;canReconcile:boolean;reason:string;original:QueuedServerMutation|NativeDeleteAttempt|CleanupAttempt}
export async function listPendingDocumentDeletions():Promise<PendingDocumentDeletion[]> {
 if(getCurrentLaunchMode()==="server") {
  const context=captureOfflineServerContext();const rows=listOfflineServerMutations(getApiBase());
  const result=rows.filter(r=>deleteRoute(r.method,r.path)).map(r=>{
   const match=/^\/spaces\/([^/?#]+)\/documents\/([^/?#]+)\/trash$/.exec(r.path)!;
   const target=originalDeleteTarget(r.body),valid=canonicalDeleteOperation(r.deletionOperationId)&&!!target?.deleted_at&&sameOfflineReplayOrigin(r.origin,context.origin)&&rows.filter(other=>other.path===r.path).length===1;
   return{kind:"server" as const,id:r.id,operation:r.deletionOperationId??"Original UUID unavailable",space:match[1],document:match[2],createdAt:target?.created_at??"Original creation unavailable",deletedAt:target?.deleted_at??"Original Trash cycle unavailable",canReconcile:valid,reason:valid?"Retry only this exact original request; the server checks current deletion authority.":"Original witness or server session unavailable. Retain evidence; reload cannot authorize a guessed retry.",original:structuredClone(r)};
  });
  const cleanup=readCleanupAttempts().filter(r=>r.origin.serverBase===context.origin.serverBase).map(r=>{
   const valid=sameOfflineReplayOrigin(r.origin,context.origin)&&!rows.some(other=>other.deletionOperationId===r.operation);
   return{kind:"cleanup" as const,id:`cleanup:${r.operation}`,operation:r.operation,space:r.space,document:r.document,createdAt:r.original_target.created_at,deletedAt:r.original_target.deleted_at,canReconcile:valid,reason:valid?"Page deletion is acknowledged. Attachment cleanup still requires the exact original operation; bytes and charges remain until physical completion is proved.":"Original cleanup session differs or page deletion still needs reconciliation. Retain the original receipt.",original:structuredClone(r)};
  });
  const combined:PendingDocumentDeletion[]=[...result,...cleanup];
  if(JSON.stringify(combined).length>1048576 || combined.length>64)throw new Error("Pending deletion review exceeds bound; retain all original evidence");return combined;
 }
 if(!isTauri())return[];
 const session=captureNativeArchiveLocalSession(),current=await session.describe();session.verify();
 const nativeRows=listPendingNativeDocumentDeletes();
 const physical=nativeRows.length===1?await describeCurrentNativeDeletionSelection(nativeRows[0].request.space_id):undefined;session.verify();
 return nativeRows.map((r,index)=>{
  const valid=nativeRows.length===1&&Object.keys(r.request.original_binding.context).length===7&&Object.entries(session.context).every(([key,value])=>(r.request.original_binding.context as unknown as Record<string,unknown>)[key]===value)&&r.request.original_binding.root===current.root&&!!physical&&["root","database_directory","database"].every(key=>{const a=r.request.original_binding.custody[key as keyof typeof physical.custody],b=physical.custody[key as keyof typeof physical.custody];return a.device===b.device&&a.inode===b.inode;});
  return{kind:"native",id:nativeRows.length===1?r.request.operation_id:`${r.request.operation_id}:conflict:${index}`,operation:r.request.operation_id,space:r.request.space_id,document:r.request.document_id,createdAt:r.request.original_target.created_at,deletedAt:r.request.original_target.deleted_at,originalRoot:r.request.original_binding.root,canReconcile:valid,reason:valid?"Reconcile only the exact retained native request and original receipt.":"Original native session/vault differs or retained records conflict. Keep every attempt; changed context cannot authorize retry.",original:structuredClone(r)};
 });
}
