import { Button } from "@/components/ui/button";
import {
  DialogShell,
  DialogHeader,
  DialogTitle,
  DialogBody,
  DialogFooter,
  DialogCloseAction,
} from "@/components/ui/dialog-shell";

export function DocumentDraftPublicationReview({
  publication,
  pagePublished,
  busy,
  error,
  onConfirm,
  onCancel,
  cancelLabel = "Keep draft",
  confirmBlocked,
}: {
  publication: { title: string; content: string; icon: string | null; cover_image: string | null };
  pagePublished: boolean;
  busy: boolean;
  error?: string;
  onConfirm: () => void;
  onCancel: () => void;
  cancelLabel?: string;
  confirmBlocked?: string;
}) {
  return (
    <DialogShell
      labelledBy="draft-publication-review-title"
      describedBy="draft-publication-review-description"
      maxWidth="lg"
      onClose={() => {
        if (!busy) onCancel();
      }}
    >
      <DialogHeader>
        <DialogTitle id="draft-publication-review-title">Publish this saved draft?</DialogTitle>
        <DialogCloseAction disabled={busy} onClick={onCancel} />
      </DialogHeader>
      <DialogBody>
        <p id="draft-publication-review-description" className="text-text-muted mb-4 text-sm">
          {pagePublished
            ? "Replace the published revision with the saved draft shown below."
            : "Publish the first page revision from the saved draft shown below."}{" "}
          People with existing page access can read it. Publishing does not grant access. Later
          draft or page changes will be rejected.
        </p>
        <dl className="text-text-muted mb-3 space-y-1 text-sm [overflow-wrap:anywhere]">
          <div>
            <dt className="inline font-medium">Title: </dt>
            <dd className="inline">{publication.title}</dd>
          </div>
          <div>
            <dt className="inline font-medium">Icon: </dt>
            <dd className="inline">{publication.icon || "None"}</dd>
          </div>
          <div>
            <dt className="inline font-medium">Cover reference: </dt>
            <dd className="inline">{publication.cover_image || "None"}</dd>
          </div>
        </dl>
        <pre
          tabIndex={0}
          aria-label="Saved publication source"
          className="border-border bg-surface-raised text-text max-h-80 overflow-auto rounded border p-3 text-sm [overflow-wrap:anywhere] whitespace-pre-wrap"
        >
          {publication.content || "Empty body"}
        </pre>
        {error && (
          <p role="alert" className="text-danger mt-4 text-sm">
            {error}
          </p>
        )}
        {confirmBlocked && <p className="text-text-muted mt-3 text-sm">{confirmBlocked}</p>}
      </DialogBody>
      <DialogFooter>
        <Button variant="secondary" disabled={busy} onClick={onCancel}>
          {cancelLabel}
        </Button>
        <Button disabled={busy || Boolean(confirmBlocked)} onClick={onConfirm}>
          {busy ? "Publishing…" : "Publish reviewed draft"}
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
