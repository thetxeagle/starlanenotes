import { Button } from "@/components/ui/button";
import { LoadingState } from "@/components/ui/loading-state";
import type { PublicationSnapshotSummary } from "@/lib/documentPublishedHistory";
export type PublicationHistoryStatus = "idle" | "loading" | "ready" | "denied" | "offline" | "unavailable";
export function DocumentPublicationHistory({status,records,onRetry,onPreview}:{
  status:PublicationHistoryStatus;
  records:PublicationSnapshotSummary[];
  onRetry():void;
  onPreview(summary:PublicationSnapshotSummary):void;
}) {
  return <section aria-label="Published snapshots" className="grid gap-2">
    <h3 className="text-text-muted text-xs font-medium uppercase">Published snapshots</h3>
    {status === "idle" || status === "loading" ? <LoadingState label="Loading published snapshots" /> : status === "denied" ? <p role="status" className="text-text-muted text-sm">Published snapshot history requires current Edit access to this page.</p> : status === "offline" || status === "unavailable" ? <div role="alert" className="text-text-muted grid gap-2 text-sm"><p>{status === "offline" ? "Reconnect to verify current history access." : "Published snapshots are unavailable."}</p><Button size="sm" variant="secondary" className="justify-self-start" onClick={onRetry}>Retry snapshots</Button></div> : records.length === 0 ? <p className="text-text-muted text-sm">No published snapshots have been captured yet. Legacy change records may still be available below.</p> : <>
      <p className="text-text-muted text-xs">Snapshots preserve captured page source. Preview does not restore or publish.</p>
      {records.map((snapshot) => <div key={snapshot.id} className="rounded-input border-border min-w-0 border p-2 [overflow-wrap:anywhere]">
        <p className="text-text text-sm font-medium">{snapshot.content.title || "Untitled"}</p>
        <p className="text-text-muted mt-1 text-xs">{snapshot.kind === "baseline" ? "Previous published page captured before replacement" : "Published revision"}</p>
        <p className="text-text-muted mt-1 text-xs">Published: {snapshot.published_at}</p>
        {snapshot.kind === "baseline" && <p className="text-text-muted mt-1 text-xs">Captured: {snapshot.captured_at}</p>}
        <Button variant="secondary" size="sm" className="mt-2" onClick={() => onPreview(snapshot)}>Preview snapshot</Button>
      </div>)}
    </>}
  </section>;
}
