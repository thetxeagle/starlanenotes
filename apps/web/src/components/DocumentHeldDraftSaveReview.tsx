import { Button } from "@/components/ui/button";
import { DialogShell, DialogHeader, DialogTitle, DialogBody, DialogFooter, DialogCloseAction } from "@/components/ui/dialog-shell";
import type { HeldDraftQueueReview } from "@/lib/heldDraftQueue";

/** Deliberate review of retained source-bound draft saves; never publishes. */
export function DocumentHeldDraftSaveReview({ review, busy, error, retryBlocked, onRetry, onDownload, onKeep, onSeparateRecovery }: {
  review: HeldDraftQueueReview;
  busy: boolean;
  error?: string;
  retryBlocked?: string;
  onRetry: () => void;
  onDownload: () => void;
  onKeep: () => void;
  onSeparateRecovery?: () => void;
}) {
  const original = review.original.body as Record<string, unknown>;
  const current = review.source.draft;
  const sourceChanged = original.source_digest !== review.source.draft_source_digest;
  const blocked = retryBlocked || (sourceChanged ? "The current draft differs from the original save source. Keep or download the request, or review a separate recovery copy." : undefined);
  const uncertain = review.original.replayState === "unknown";
  const originalField = (key: string) => {
    const value = original[key];
    if (typeof value !== "string") return "Not sent · preserve the existing field";
    if ((key === "icon" || key === "cover_image") && !value.trim()) return "Clear reference";
    if (key === "content" && value === "") return "Empty body";
    if (key === "title" && !value.trim()) return "Empty title · review the original request";
    return value;
  };
  return <DialogShell labelledBy="held-draft-review-title" describedBy="held-draft-review-description" maxWidth="xl" onClose={() => { if (!busy) onKeep(); }}>
    <DialogHeader>
      <div>
        <DialogTitle id="held-draft-review-title">Review original queued save</DialogTitle>
        <p id="held-draft-review-description" className="text-text-muted mt-1 text-sm">
          {uncertain ? "The original save outcome is uncertain." : "The original save is held for review."} Compare its exact requested changes with the current draft. This does not publish anything.
        </p>
      </div>
      <DialogCloseAction disabled={busy} onClick={onKeep} />
    </DialogHeader>
    <DialogBody>
      <p className="text-text-muted mb-4 text-sm">A matching source permits only a deliberate retry of the original request. It does not prove an earlier uncertain save committed. Unlocking and reconnecting do not resume this request.</p>
      <div className="grid min-w-0 gap-4 md:grid-cols-2">
        <section aria-label="Original queued changes" className="min-w-0">
          <h3 className="text-text mb-2 font-medium">Original queued changes</h3>
          <dl className="text-text-muted mb-3 space-y-1 text-sm [overflow-wrap:anywhere]">
            <div><dt className="inline font-medium">Title: </dt><dd className="inline">{originalField("title")}</dd></div>
            <div><dt className="inline font-medium">Icon: </dt><dd className="inline">{originalField("icon")}</dd></div>
            <div><dt className="inline font-medium">Cover reference: </dt><dd className="inline">{originalField("cover_image")}</dd></div>
          </dl>
          <pre tabIndex={0} aria-label="Original queued body" className="border-border bg-surface-raised text-text max-h-64 overflow-auto rounded border p-3 text-sm [overflow-wrap:anywhere] whitespace-pre-wrap">{originalField("content")}</pre>
        </section>
        <section aria-label="Current reviewed draft" className="min-w-0">
          <h3 className="text-text mb-2 font-medium">Current reviewed draft</h3>
          <dl className="text-text-muted mb-3 space-y-1 text-sm [overflow-wrap:anywhere]">
            <div><dt className="inline font-medium">Title: </dt><dd className="inline">{current.title || "Untitled"}</dd></div>
            <div><dt className="inline font-medium">Icon: </dt><dd className="inline">{current.icon || "None"}</dd></div>
            <div><dt className="inline font-medium">Cover reference: </dt><dd className="inline">{current.cover_image || "None"}</dd></div>
          </dl>
          <pre tabIndex={0} aria-label="Current reviewed body" className="border-border bg-surface-raised text-text max-h-64 overflow-auto rounded border p-3 text-sm [overflow-wrap:anywhere] whitespace-pre-wrap">{current.content || "Empty body"}</pre>
        </section>
      </div>
      {blocked && <p className="text-text-muted mt-4 text-sm">{blocked}</p>}
      {error && <p role="alert" className="text-danger mt-4 text-sm">{error}</p>}
    </DialogBody>
    <DialogFooter className="flex-wrap">
      <Button variant="secondary" disabled={busy} onClick={onDownload}>Download original request</Button>
      {onSeparateRecovery && <Button variant="secondary" disabled={busy} onClick={onSeparateRecovery}>Review separate recovery copy</Button>}
      <Button variant="secondary" disabled={busy} onClick={onKeep}>Keep held</Button>
      <Button disabled={busy || Boolean(blocked)} onClick={onRetry}>{busy ? "Checking original save…" : "Retry reviewed original save"}</Button>
    </DialogFooter>
  </DialogShell>;
}
