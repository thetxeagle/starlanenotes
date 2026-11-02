import { Button } from "@/components/ui/button";
import {
  DialogShell,
  DialogHeader,
  DialogTitle,
  DialogBody,
  DialogFooter,
  DialogCloseAction,
} from "@/components/ui/dialog-shell";
import type { DocumentDraftRecoverySnapshot } from "@/lib/documentDraftRecovery";
import type { PageCssProvenance } from "@/lib/pageCssProvenance";

export interface RecoveryReviewContent {
  title: string;
  content: string;
  synopsis: string;
  pageWidth: string;
  customCss: string;
  cssProvenance?: PageCssProvenance;
  icon?: string | null;
  coverImage?: string | null;
}

function RecoveryCopy({ label, value }: { label: string; value: RecoveryReviewContent }) {
  return (
    <section className="min-w-0" aria-label={label}>
      <h3 className="text-text mb-2 font-medium">{label}</h3>
      <dl className="text-text-muted mb-3 space-y-1 text-sm [overflow-wrap:anywhere]">
        <div>
          <dt className="inline font-medium">Title: </dt>
          <dd className="inline">{value.title || "Untitled"}</dd>
        </div>
        <div>
          <dt className="inline font-medium">Synopsis: </dt>
          <dd className="inline">{value.synopsis || "None"}</dd>
        </div>
        <div>
          <dt className="inline font-medium">Page width: </dt>
          <dd className="inline">{value.pageWidth}</dd>
        </div>
        <div>
          <dt className="inline font-medium">Icon: </dt>
          <dd className="inline">
            {value.icon === undefined ? "Not captured · keep loaded icon" : value.icon || "None"}
          </dd>
        </div>
        <div>
          <dt className="inline font-medium">Cover reference: </dt>
          <dd className="inline">
            {value.coverImage === undefined
              ? "Not captured · keep loaded cover"
              : value.coverImage || "None"}
          </dd>
        </div>
      </dl>
      <pre
        className="border-border bg-surface-raised text-text max-h-64 overflow-auto rounded border p-3 text-sm [overflow-wrap:anywhere] whitespace-pre-wrap"
        tabIndex={0}
        aria-label={`${label} content`}
      >
        {value.content || "Empty body"}
      </pre>
      {value.cssProvenance?.kind === "single" ? (
        <details className="text-text-muted mt-2 text-sm">
          <summary>Retained original CSS source</summary>
          <p className="mt-2">
            The original marker block is retained, including empty or blocked CSS. Rendering uses
            approved CSS only. Displayed text may normalize line endings; Download device copy
            preserves the retained source in JSON.
          </p>
          <pre
            className="mt-2 max-h-40 overflow-auto [overflow-wrap:anywhere] whitespace-pre-wrap"
            tabIndex={0}
            aria-label={`${label} retained original CSS source`}
          >
            {value.cssProvenance.rawBlock}
          </pre>
        </details>
      ) : value.cssProvenance?.kind === "ambiguous" ? (
        <details className="text-text-muted mt-2 text-sm">
          <summary>Retained original page source · ambiguous CSS markers</summary>
          <p className="mt-2">
            CSS marker boundaries are ambiguous, so automatic reconstruction is held. The whole
            original source is preserved. Download device copy includes that source and your edited
            body in JSON; displayed text may normalize line endings.
          </p>
          <pre
            className="mt-2 max-h-40 overflow-auto [overflow-wrap:anywhere] whitespace-pre-wrap"
            tabIndex={0}
            aria-label={`${label} retained original page source`}
          >
            {value.cssProvenance.originalSource}
          </pre>
        </details>
      ) : value.customCss ? (
        <details className="text-text-muted mt-2 text-sm">
          <summary>Rendered CSS · original source not captured</summary>
          <p className="mt-2">
            This is the stored rendering value, not an exact original CSS source.
          </p>
          <pre
            className="mt-2 max-h-40 overflow-auto [overflow-wrap:anywhere] whitespace-pre-wrap"
            tabIndex={0}
            aria-label={`${label} rendering CSS`}
          >
            {value.customCss}
          </pre>
        </details>
      ) : null}
    </section>
  );
}

export function DocumentDraftRecoveryReview({
  checkpoint,
  loaded,
  provenance,
  busy,
  error,
  replaceBlocked,
  copyBlocked,
  loadedUnavailable,
  replacingRecoveryTarget,
  restartCopy,
  onClose,
  onCopy,
  onReplace,
  onDownload,
}: {
  checkpoint: DocumentDraftRecoverySnapshot;
  loaded: RecoveryReviewContent;
  provenance: "matching" | "changed" | "legacy-unverified";
  busy: boolean;
  error: string;
  loadedUnavailable?: boolean;
  replacingRecoveryTarget?: boolean;
  restartCopy?: boolean;
  replaceBlocked?: string;
  copyBlocked?: string;
  onClose: () => void;
  onCopy: () => void;
  onReplace: () => void;
  onDownload: () => void;
}) {
  return (
    <DialogShell
      labelledBy="draft-recovery-review-title"
      describedBy="draft-recovery-review-description"
      maxWidth="xl"
      onClose={onClose}
    >
      <DialogHeader>
        <div>
          <DialogTitle id="draft-recovery-review-title">Review retained edits</DialogTitle>
          <p id="draft-recovery-review-description" className="text-text-muted mt-1 text-sm">
            {replacingRecoveryTarget
              ? "A separate recovery draft already exists. Compare its current content with your retained edits before confirming a replacement there."
              : provenance === "matching"
                ? "These edits started from the loaded draft source."
                : provenance === "changed"
                  ? "The loaded draft has changed since these device edits began."
                  : "This older checkpoint has no verified original source. Compare both copies before choosing."}{" "}
            Nothing is published by recovery.
          </p>
        </div>
        <DialogCloseAction onClick={onClose} />
      </DialogHeader>
      <DialogBody>
        <p className="text-text-muted mb-4 text-sm">
          {loadedUnavailable
            ? "The original draft is unavailable. Its captured source is shown for comparison only; it does not grant access. Published-page recovery can create a separate private draft if current write access allows it."
            : "Loaded draft is the copy returned for this review; it may be cached while offline. Replacement checks this exact source again when saved. Separate draft recovery preserves the original draft."}
        </p>
        <div className="grid min-w-0 gap-4 md:grid-cols-2">
          <RecoveryCopy
            label={
              replacingRecoveryTarget
                ? "Loaded recovery draft"
                : loadedUnavailable
                  ? "Captured original · draft unavailable"
                  : "Loaded draft"
            }
            value={loaded}
          />
          <RecoveryCopy label="Retained device edits" value={checkpoint} />
        </div>
        {checkpoint.baseDraftSource && (
          <details className="text-text-muted mt-4 text-sm">
            <summary>Original source captured on this device</summary>
            <p className="mt-2 [overflow-wrap:anywhere]">{checkpoint.baseDraftSource.title}</p>
            <pre className="mt-2 max-h-48 overflow-auto [overflow-wrap:anywhere] whitespace-pre-wrap">
              {checkpoint.baseDraftSource.content}
            </pre>
          </details>
        )}
        {error && (
          <p role="alert" className="text-danger mt-4 text-sm">
            {error}
          </p>
        )}
        {replaceBlocked && <p className="text-text-muted mt-3 text-sm">{replaceBlocked}</p>}
        {copyBlocked && <p className="text-text-muted mt-3 text-sm">{copyBlocked}</p>}
      </DialogBody>
      {restartCopy && (
        <p className="text-text-muted px-4 pb-4 text-sm">
          The original creation request was rejected. Starting a new reviewed request retains the
          old operation evidence and uses a new operation identity.
        </p>
      )}
      <DialogFooter className="flex-wrap">
        <Button variant="secondary" onClick={onDownload}>
          Download device copy
        </Button>
        <Button variant="secondary" disabled={busy || Boolean(replaceBlocked)} onClick={onReplace}>
          {replacingRecoveryTarget ? "Replace reviewed recovery draft…" : "Replace reviewed draft…"}
        </Button>
        <Button disabled={busy || Boolean(copyBlocked)} onClick={onCopy}>
          {busy
            ? "Recovering…"
            : restartCopy
              ? "Start a new reviewed recovery request"
              : "Save as a separate private draft"}
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
