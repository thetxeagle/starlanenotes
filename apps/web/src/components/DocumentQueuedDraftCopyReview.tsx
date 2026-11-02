import type { DraftRecord } from "@/lib/dataClient";
import { Button } from "@/components/ui/button";
import { DialogShell, DialogHeader, DialogTitle, DialogBody, DialogFooter, DialogCloseAction } from "@/components/ui/dialog-shell";
import type { QueuedCopyReviewRecord } from "@/lib/queuedDraftCopy";
import type { DraftRecoveryCopyRecord } from "@/lib/documentDraftRecoveryActions";

export function DocumentQueuedDraftCopyReview({record, operation, busy, error, onCreate, onClose, onDownload, onOpenTarget, targetReview, onReviewTarget, onReplaceTarget, restartReview, onReviewRejectedBirth, onRestartRejectedBirth}: {
 record:QueuedCopyReviewRecord; operation:DraftRecoveryCopyRecord|null; busy:boolean; error?:string;
 onCreate:()=>void; onClose:()=>void; onDownload:()=>void; onOpenTarget?:()=>void;
 restartReview?:DraftRecord; onReviewRejectedBirth?:()=>void; onRestartRejectedBirth?:()=>void;
 targetReview?:DraftRecord|null; onReviewTarget?:()=>void; onReplaceTarget?:()=>void;
}) {
 const fields=record.composition.fields;
 const pendingSave=operation && ["save-pending","save-held","save-rejected"].includes(operation.state);
 const uncertainBirth=operation && ["birth-pending","birth-held"].includes(operation.state);
 return <DialogShell labelledBy="queued-copy-title" describedBy="queued-copy-description" maxWidth="xl" onClose={()=>{if(!busy)onClose();}}>
  <DialogHeader><div><DialogTitle id="queued-copy-title">Review separate recovery draft</DialogTitle>
   <p id="queued-copy-description" className="text-text-muted mt-1 text-sm">Review the complete raw content proposed for a separate private draft. Fields not sent in the original request use the current draft observed when this proposal was first retained.</p></div><DialogCloseAction disabled={busy} onClick={onClose}/></DialogHeader>
  <DialogBody><p className="text-text-muted mb-4 text-sm">This keeps the original queued request, current editor and device checkpoint. It does not publish, replace the original draft or change access. Retrying a retained birth uses the same operation.</p>
   <dl className="text-text-muted mb-3 space-y-2 text-sm [overflow-wrap:anywhere]">{(["title","icon","cover_image"] as const).map(field=><div key={field}><dt className="font-medium">{field==="cover_image"?"Cover reference":field==="title"?"Title":"Icon"} · {record.composition.provenance[field]==="original-request"?"original requested change":"current draft at first review"}</dt><dd>{fields[field]??"None"}</dd></div>)}</dl>
   <p className="text-text-muted mb-2 text-sm">Raw body · {record.composition.provenance.content==="original-request"?"original requested change":"current draft at first review"}</p>
   <pre tabIndex={0} aria-label="Separate recovery raw body" className="border-border bg-surface-raised text-text max-h-64 overflow-auto rounded border p-3 text-sm [overflow-wrap:anywhere] whitespace-pre-wrap">{fields.content||"Empty body"}</pre>
   {operation && <p className="text-text-muted mt-4 text-sm">{operation.state==="confirmed"||operation.state==="finished"?"Original recovery operation confirmed. Open the draft to review its current state.":pendingSave?"A recovery target exists, but its save needs review. Keep the retained proposal and original target; creating another draft is blocked.":uncertainBirth?"Birth outcome is retained. Only the original birth request may be retried; no second draft is requested.":"Original recovery operation retained."}</p>}
   {restartReview && <section aria-label="Reviewed source for rejected recovery birth" className="border-border mt-4 rounded border p-3"><h3 className="text-text font-medium">Fresh source for a rejected birth</h3><p className="text-text-muted mt-2 text-sm [overflow-wrap:anywhere]">{restartReview.title}</p><pre tabIndex={0} className="bg-surface-raised text-text mt-2 max-h-64 overflow-auto p-3 text-sm [overflow-wrap:anywhere] whitespace-pre-wrap">{restartReview.content||"Empty body"}</pre><p className="text-text-muted mt-2 text-sm">A restart archives the proven rejected operation and creates a new reviewed operation against this source. The complete raw recovery proposal above stays unchanged. Unknown births cannot use this action.</p></section>}
   {targetReview && <section aria-label="Current recovery target comparison" className="border-border mt-4 rounded border p-3"><h3 className="text-text font-medium">Current recovery target</h3><p className="text-text-muted mt-2 text-sm [overflow-wrap:anywhere]">Title: {targetReview.title} · Icon: {targetReview.icon??"None"} · Cover: {targetReview.cover_image??"None"}</p><pre tabIndex={0} aria-label="Current recovery target raw body" className="bg-surface-raised text-text mt-2 max-h-64 overflow-auto p-3 text-sm [overflow-wrap:anywhere] whitespace-pre-wrap">{targetReview.content||"Empty body"}</pre><p className="text-text-muted mt-2 text-sm">Replacement uses this target's current source and the complete retained proposal shown above. Later changes are rejected. Unknown or queued target saves must be reviewed first.</p></section>}
   {error && <p role="alert" className="text-danger mt-4 text-sm">{error}</p>}
  </DialogBody><DialogFooter className="flex-wrap"><Button variant="secondary" onClick={onDownload}>Download recovery proposal</Button>
   {onOpenTarget && <Button variant="secondary" disabled={busy} onClick={onOpenTarget}>Open recovery draft in new tab</Button>}
   {onReviewRejectedBirth && <Button variant="secondary" disabled={busy} onClick={onReviewRejectedBirth}>Review rejected birth against current source</Button>}
   {onRestartRejectedBirth && <Button disabled={busy} onClick={onRestartRejectedBirth}>Restart reviewed rejected birth</Button>}
   {onReviewTarget && <Button variant="secondary" disabled={busy} onClick={onReviewTarget}>Review current recovery target</Button>}
   {onReplaceTarget && <Button disabled={busy} onClick={onReplaceTarget}>Replace reviewed recovery target</Button>}
   <Button variant="secondary" disabled={busy} onClick={onClose}>Keep retained</Button>
   <Button disabled={busy||Boolean(pendingSave)} onClick={onCreate}>{busy?"Checking original recovery…":uncertainBirth?"Retry original recovery birth":operation?.saved?"Review original recovery result":"Create reviewed private draft"}</Button>
  </DialogFooter></DialogShell>;
}
