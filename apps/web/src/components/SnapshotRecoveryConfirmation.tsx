import { Button } from "@/components/ui/button";
import { DialogShell, DialogHeader, DialogTitle, DialogBody, DialogFooter, DialogCloseAction } from "@/components/ui/dialog-shell";
import { LoadingState } from "@/components/ui/loading-state";

export interface SnapshotRecoveryContent {
  title: string;
  content: string;
  icon: string | null;
  cover_image: string | null;
}
export type RecoveryComparison =
  | { status: "loading" }
  | { status: "unavailable"; reason: string }
  | { status: "ready"; title: string; content: string };
const text = (value: string | null) => value === null ? "None" : value || "Empty text";

export function SnapshotRecoveryConfirmation({ draft, comparison, loading, busy, error, originalRequest, onConfirm, onClose }: {
  draft: SnapshotRecoveryContent | null;
  comparison: RecoveryComparison;
  loading: boolean;
  busy: boolean;
  error?: string;
  originalRequest?: boolean;
  onConfirm(): void;
  onClose(): void;
}) {
  return <DialogShell labelledBy="snapshot-recovery-title" maxWidth="xl" onClose={onClose}>
    <DialogHeader><DialogTitle id="snapshot-recovery-title">Recover into a private draft</DialogTitle><DialogCloseAction onClick={onClose} /></DialogHeader>
    <DialogBody className="min-h-0 space-y-4 overflow-y-auto">
      <p className="text-text-muted text-sm">Create a separate draft owned by you. Your published page and existing drafts stay in place. Publishing is a separate action.</p>
      {originalRequest && <p className="text-text-muted text-sm">This is your saved original recovery request. Confirmation checks its reviewed source again; it does not select a newer revision.</p>}
      {error && <p role="alert" className="text-text-muted text-sm">{error}</p>}
      {loading ? <LoadingState label="Reviewing recovery source" /> : draft && <>
        <section aria-label="Selected revision for recovery" className="space-y-2">
          <h3 className="text-text text-sm font-medium">Selected revision</h3>
          <dl className="text-text-muted text-sm [overflow-wrap:anywhere]">
            <div><dt className="inline font-medium">Title: </dt><dd className="inline">{text(draft.title)}</dd></div>
            <div><dt className="inline font-medium">Icon: </dt><dd className="inline">{text(draft.icon)}</dd></div>
            <div><dt className="inline font-medium">Cover reference: </dt><dd className="inline">{text(draft.cover_image)}</dd></div>
          </dl>
          <Source label="Selected portable Markdown" value={draft.content} />
        </section>
        <details className="text-text-muted text-sm">
          <summary className="cursor-pointer">Compare with the current published page</summary>
          <div className="mt-2 space-y-2">
            {comparison.status === "loading" ? <LoadingState label="Loading current published comparison" /> : comparison.status === "unavailable" ? <p>{comparison.reason}</p> : <>
              <p className="text-text font-medium">{comparison.title}</p>
              <p>Current stored Markdown includes its storage header. It is shown for comparison and is not changed by recovery.</p>
              <Source label="Current published Markdown" value={comparison.content} />
            </>}
          </div>
        </details>
        <p className="text-text-muted text-sm">Recovery copies the selected title, Markdown, icon and cover reference. It does not apply historical properties or access grants, or copy attachment and drawing bytes. References may be unavailable under current access or storage.</p>
      </>}
      <p className="text-text-muted text-sm">After confirmation, open the recovered draft in a new tab to keep this editor and its saved-on-device work in place.</p>
    </DialogBody>
    <DialogFooter>
      <Button onClick={onConfirm} disabled={loading || busy || !draft || Boolean(error)}>{busy ? "Confirming recovery…" : "Create private draft"}</Button>
      <Button variant="secondary" onClick={onClose}>Close</Button>
    </DialogFooter>
  </DialogShell>;
}
function Source({ label, value }: { label: string; value: string }) {
  return <pre aria-label={label} className="border-border bg-surface-alt text-text max-h-64 select-text overflow-auto whitespace-pre-wrap rounded-input border p-3 text-xs [overflow-wrap:anywhere]">{value}</pre>;
}
