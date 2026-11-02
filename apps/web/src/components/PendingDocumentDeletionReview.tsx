import {Button} from "@/components/ui/button";
import type {PendingDocumentDeletion} from "@/lib/pendingDocumentDelete";
export function PendingDocumentDeletionReview({rows,busy,onReconcile}:{rows:PendingDocumentDeletion[];busy:string|null;onReconcile:(row:PendingDocumentDeletion)=>void}) {
 if(!rows.length)return null;
 return <section className="border-border border-b px-4 py-3" aria-label="Pending permanent deletions">
  <h3 className="text-text text-sm font-medium">Pending permanent deletions</h3>
  <p className="text-text-muted mt-1 text-xs">A missing page does not prove the original deletion outcome. Page deletion and physical attachment cleanup have separate outcomes. Review the retained original request.</p>
  <div className="mt-3 space-y-3">{rows.map(row=><div key={`${row.kind}:${row.id}`} className="rounded-card border-border border p-3">
   <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
    <div className="min-w-0 flex-1">
     <p className="text-text break-words text-sm font-medium">{row.kind==="cleanup"?"Attachment cleanup for original page":"Original page"} {row.document}</p>
     <p className="text-text-muted mt-1 break-words text-xs">Original space: {row.space}</p>
     <p className="text-text-muted mt-1 break-all text-xs">Operation: {row.operation}</p>
     {row.originalRoot&&<p className="text-text-muted mt-1 break-words text-xs">Original vault: {row.originalRoot}</p>}
     <p className="text-text-muted mt-1 break-words text-xs">Created: {row.createdAt} · Trash cycle: {row.deletedAt}</p>
     <p className="text-text-muted mt-2 break-words text-xs">{row.reason}</p>
    </div>
    <Button type="button" variant="secondary" size="sm" disabled={!row.canReconcile||busy!==null} onClick={()=>onReconcile(row)} className="w-full shrink-0 sm:w-auto">{busy===row.id?"Reconciling…":row.kind==="cleanup"?"Retry original attachment cleanup":"Reconcile original deletion"}</Button>
   </div>
  </div>)}</div>
 </section>;
}
