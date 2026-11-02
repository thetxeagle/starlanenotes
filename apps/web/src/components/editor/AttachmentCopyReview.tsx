import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { api, captureOfflineServerContext, getApiBase } from "@/lib/api";
import { verifyOfflineReplayContext } from "@/lib/offlineReplayOrigin";
import {
  retainedAttachmentCopies,
  adoptReviewedAttachmentCopy,
  type AttachmentCopyOperation,
} from "@/lib/attachmentCopyOperation";
import type { AttachmentRecord } from "@/lib/dataClient";
/** Original target-scoped operations remain reviewable after response loss or selection changes. */
export function AttachmentCopyReview({
  spaceId,
  documentId,
  draftId,
  onUse,
  referencedVersionIds,
}: {
  spaceId: string;
  documentId: string;
  draftId: string;
  onUse: (attachment: AttachmentRecord) => Promise<void>;
  referencedVersionIds: () => ReadonlySet<string>;
}) {
  const [rows, setRows] = useState<AttachmentCopyOperation[]>([]),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const generation = useRef(0),
    lock = useRef(false),
    references = useRef(referencedVersionIds),
    ownedContext = useRef<ReturnType<typeof captureOfflineServerContext> | null>(null);
  references.current = referencedVersionIds;
  useEffect(() => {
    const lifecycle = generation;
    const sequence = ++lifecycle.current;
    setRows([]);
    setError("");
    setBusy(false);
    ownedContext.current = null;
    void (async () => {
      try {
        const context = captureOfflineServerContext();
        const found = await retainedAttachmentCopies({ spaceId, documentId, draftId });
        verifyOfflineReplayContext(context, getApiBase(), api.getToken());
        if (sequence === generation.current) {
          ownedContext.current = context;
          setRows(found);
        }
      } catch (e) {
        if (sequence === generation.current)
          setError(e instanceof Error ? e.message : "Retained copies unavailable.");
      }
    })();
    return () => {
      lifecycle.current++;
    };
  }, [spaceId, documentId, draftId]);
  const handleUseOriginal = async (row: AttachmentCopyOperation) => {
    if (lock.current) return;
    lock.current = true;
    const sequence = generation.current;
    setBusy(true);
    setError("");
    try {
      const context = captureOfflineServerContext(),
        verify = () => {
          verifyOfflineReplayContext(context, getApiBase(), api.getToken());
          if (sequence !== generation.current)
            throw new Error("Original copied-file selection changed.");
        };
      verify();
      await adoptReviewedAttachmentCopy(
        row.id,
        { spaceId, documentId, draftId },
        {
          verifySelection: verify,
          isReferenced: (id) => references.current().has(id),
          onUse,
        },
      );
      // A successful guarded insertion can unmount this dialog itself. Its
      // original durable adoption still completes; stale UI never commits.
      if (sequence === generation.current) {
        const found = await retainedAttachmentCopies({ spaceId, documentId, draftId });
        verify();
        setRows(found);
      }
    } catch (e) {
      if (sequence === generation.current)
        setError(e instanceof Error ? e.message : "Original copy could not be used.");
    } finally {
      lock.current = false;
      if (sequence === generation.current) setBusy(false);
    }
  };
  if (ownedContext.current) {
    try {
      verifyOfflineReplayContext(ownedContext.current, getApiBase(), api.getToken());
    } catch {
      return (
        <p role="status" className="text-text-muted text-sm">
          Original copy context changed. Reopen this dialog to review the current account’s copies.
        </p>
      );
    }
  }
  if (!rows.length && !error) return null;
  return (
    <section
      className="rounded-card border-border space-y-3 border p-3"
      aria-label="Retained attachment copies"
    >
      <h3 className="text-text text-sm font-medium">Retained copies for this draft</h3>
      <p className="text-text-muted text-sm">
        A retry keeps the original source version, destination draft, and operation ID. A completed
        copy is inserted only when you choose Use.
      </p>
      {error ? (
        <p role="alert" className="text-danger text-sm">
          {error}
        </p>
      ) : null}
      <ul className="space-y-3">
        {rows.map((row) => (
          <li className="space-y-2" key={row.id}>
            <p className="text-text text-sm break-words">
              {JSON.parse(row.source.rawSource).original_name}
            </p>
            <p className="text-text-muted text-xs break-all">
              {row.id} ·{" "}
              {row.state === "acknowledged"
                ? "Copied; use not yet recorded"
                : "Original outcome requires review"}
            </p>
            <Button
              size="sm"
              variant="secondary"
              disabled={busy}
              onClick={() => void handleUseOriginal(row)}
            >
              {row.state === "acknowledged" ? "Use copied file" : "Retry original copy and use"}
            </Button>
          </li>
        ))}
      </ul>
    </section>
  );
}
