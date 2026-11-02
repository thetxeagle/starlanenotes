import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
  DialogShell,
  DialogHeader,
  DialogTitle,
  DialogBody,
  DialogFooter,
  DialogCloseAction,
} from "@/components/ui/dialog-shell";
import { validatePageCssProvenance, type RetainedPageCssSource } from "@/lib/pageCssProvenance";

/** Association is checked during render, so old source never flashes on a new route. */
export function selectPageCssRecoverySource(
  source: RetainedPageCssSource | undefined,
  spaceId: string,
  documentId: string,
): RetainedPageCssSource | undefined {
  if (
    !source ||
    !spaceId ||
    !documentId ||
    source.spaceId !== spaceId ||
    source.documentId !== documentId ||
    (source.draftId !== undefined && (typeof source.draftId !== "string" || !source.draftId)) ||
    !validatePageCssProvenance(source.provenance) ||
    source.provenance.kind === "absent"
  )
    return undefined;
  return source;
}

/** Exact strings are encoded as JSON; displayed textarea line endings may differ. */
export function buildPageCssRecoveryExport(source: RetainedPageCssSource) {
  if (!selectPageCssRecoverySource(source, source.spaceId, source.documentId))
    throw new Error("Retained page CSS source is unavailable");
  return {
    format: "starlane-page-css-source",
    spaceId: source.spaceId,
    documentId: source.documentId,
    ...(source.draftId === undefined ? {} : { draftId: source.draftId }),
    ...source.provenance,
  };
}

export function PageCssRecoveryContent({
  source,
  onClose,
}: {
  source: RetainedPageCssSource;
  onClose: () => void;
}) {
  const [error, setError] = useState("");
  const provenance = source.provenance;
  if (
    !selectPageCssRecoverySource(source, source.spaceId, source.documentId) ||
    provenance.kind === "absent"
  )
    return null;
  const original = provenance.kind === "single" ? provenance.rawBlock : provenance.originalSource;
  const download = () => {
    let url: string | undefined;
    try {
      url = URL.createObjectURL(
        new Blob([JSON.stringify(buildPageCssRecoveryExport(source), null, 2)], {
          type: "application/json",
        }),
      );
      const link = document.createElement("a");
      link.href = url;
      link.download = "retained-page-css-source.json";
      link.click();
      setError("");
    } catch {
      setError("The source could not be downloaded. Keep this review open and try again.");
    } finally {
      if (url) URL.revokeObjectURL(url);
    }
  };
  return (
    <DialogShell
      labelledBy="page-css-recovery-title"
      describedBy="page-css-recovery-description"
      maxWidth="lg"
      onClose={onClose}
    >
      <DialogHeader>
        <div>
          <DialogTitle id="page-css-recovery-title">Retained page CSS</DialogTitle>
          <p id="page-css-recovery-description" className="text-text-muted mt-1 text-sm">
            Existing CSS is retained for compatibility. This source is read-only; viewing or
            downloading it does not change the page.
          </p>
        </div>
        <DialogCloseAction label="Close retained CSS" onClick={onClose} />
      </DialogHeader>
      <DialogBody>
        <p className="text-text-muted mb-3 text-sm">
          {provenance.kind === "ambiguous"
            ? "CSS marker boundaries are ambiguous. The whole original page source is retained; automatic reconstruction remains paused."
            : "This is the original CSS marker block, including empty, blocked or oversized CSS. Only approved CSS is used for rendering."}
        </p>
        <label
          htmlFor="page-css-recovery-source"
          className="text-text mb-2 block text-sm font-medium"
        >
          {provenance.kind === "single" ? "Original CSS marker block" : "Original page source"}
        </label>
        <Textarea
          id="page-css-recovery-source"
          aria-label={
            provenance.kind === "single"
              ? "Retained original CSS source"
              : "Retained original page source"
          }
          value={original}
          readOnly
          spellCheck={false}
          rows={12}
          className="max-h-96 resize-y font-mono"
        />
        <p className="text-text-muted mt-3 text-sm">
          The displayed text may normalize line endings. Download the JSON copy to preserve the
          exact original source, including CRLF and NUL characters.
        </p>
        {error && (
          <p role="alert" className="text-danger mt-3 text-sm">
            {error}
          </p>
        )}
      </DialogBody>
      <DialogFooter>
        <Button variant="secondary" onClick={onClose}>
          Close
        </Button>
        <Button onClick={download}>Download original JSON</Button>
      </DialogFooter>
    </DialogShell>
  );
}
