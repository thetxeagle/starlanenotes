import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { dataClient } from "@/lib/dataClient";
import type { PendingUploadSelection } from "@/lib/pendingUploadCleanup";

export function PendingUploadCleanupReview({
  spaceId,
  documentId,
}: {
  spaceId: string;
  documentId: string;
}) {
  const [rows, setRows] = useState<PendingUploadSelection[]>([]),
    [busy, setBusy] = useState(false),
    [status, setStatus] = useState(""),
    [error, setError] = useState(""),
    [confirm, setConfirm] = useState<PendingUploadSelection | null>(null);
  const generation = useRef(0),
    page = useRef({ spaceId, documentId });
  page.current = { spaceId, documentId };
  useEffect(() => {
    setRows([]);
    setConfirm(null);
    setStatus("");
    setError("");
    setBusy(false);
    const lifecycle = generation;
    return () => {
      lifecycle.current++;
    };
  }, [spaceId, documentId]);
  const guard = (sequence: number) => () => {
    if (
      sequence !== generation.current ||
      page.current.spaceId !== spaceId ||
      page.current.documentId !== documentId
    )
      throw new Error("Original page review changed; retain the original request");
  };
  const load = async () => {
    const sequence = ++generation.current,
      verify = guard(sequence);
    setBusy(true);
    setError("");
    setStatus("");
    setConfirm(null);
    setRows([]);
    try {
      const retained = dataClient.retainedPendingUploadCleanups(spaceId, documentId, verify);
      verify();
      setRows(retained.attempts);
      const discovered = await dataClient.discoverPendingUploadCleanups(
        spaceId,
        documentId,
        verify,
      );
      verify();
      const merged = new Map(retained.attempts.map((r) => [r.row.attempt_id, r]));
      for (const r of discovered.attempts) {
        const previous = merged.get(r.row.attempt_id);
        if (previous && JSON.stringify(previous.row) !== JSON.stringify(r.row))
          throw new Error("Saved original hashes differ from discovery; retain both for review");
        merged.set(r.row.attempt_id, r);
      }
      setRows([...merged.values()]);
      setStatus(
        discovered.availability === "not_available"
          ? `Server review unavailable (${discovered.reason}). This does not establish absence. Saved original requests remain retryable.`
          : merged.size
            ? "Select one original upload to review cleanup. No cleanup runs automatically."
            : "No reserved uploads or saved cleanup requests were found for this original session and page.",
      );
      if (retained.otherContextCount)
        setStatus(
          (previous) =>
            `${previous} Requests from another original context are retained separately.`,
        );
    } catch (e) {
      if (sequence === generation.current)
        setError(e instanceof Error ? e.message : "Original upload review unavailable.");
    } finally {
      if (sequence === generation.current) setBusy(false);
    }
  };
  const cleanup = async (selection: PendingUploadSelection) => {
    const sequence = ++generation.current,
      verify = guard(sequence);
    setBusy(true);
    setError("");
    setConfirm(null);
    try {
      const result = await dataClient.cleanupOriginalPendingUpload(selection, verify);
      verify();
      setStatus(
        result.outcome === "physically_completed"
          ? "Physically completed: the server proved the original owned upload paths were removed."
          : result.outcome === "pending"
            ? `Cleanup pending (${result.reason}). The original request is retained for manual retry.`
            : `Retained for review (${result.reason}). No complete physical deletion was proved.`,
      );
      if (result.outcome === "physically_completed")
        setRows((previous) =>
          previous.filter((r) => r.row.attempt_id !== selection.row.attempt_id),
        );
    } catch (e) {
      if (sequence === generation.current)
        setError(
          `${e instanceof Error ? e.message : "Cleanup acknowledgment unavailable."} The original request remains retained if dispatch began; check again before retrying.`,
        );
    } finally {
      if (sequence === generation.current) setBusy(false);
    }
  };
  return (
    <section
      aria-label="Original server upload cleanup"
      className="border-border mb-4 rounded border p-3"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-text text-sm font-medium">Interrupted server uploads</h3>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={busy}
          onClick={() => void load()}
        >
          {busy ? "Checking original request…" : "Check original uploads"}
        </Button>
      </div>
      <p className="text-text-muted mt-2 text-xs">
        Review idle original uploads for this live page. Active, moved, restored, rewritten, or
        unproved files stay retained. Deleted-page cleanup is unavailable here.
      </p>
      {status ? (
        <p role="status" className="text-text-muted mt-2 text-sm [overflow-wrap:anywhere]">
          {status}
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="text-danger mt-2 text-sm [overflow-wrap:anywhere]">
          {error}
        </p>
      ) : null}
      {rows.length ? (
        <ul className="mt-3 space-y-2">
          {rows.map((r) => (
            <li
              key={r.row.attempt_id}
              className="border-border flex flex-wrap items-center justify-between gap-2 border-t pt-2"
            >
              <div className="min-w-0 text-xs">
                <p className="text-text">
                  {r.row.producer_kind === "original_direct"
                    ? "Original direct upload"
                    : "Original replay upload"}
                </p>
                <p className="text-text-muted [overflow-wrap:anywhere]">
                  Request {r.row.original_operation_id}
                </p>
                <p className="text-text-muted [overflow-wrap:anywhere]">
                  Attempt {r.row.attempt_id}
                </p>
              </div>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                disabled={busy}
                onClick={() => setConfirm(r)}
              >
                Review cleanup
              </Button>
            </li>
          ))}
        </ul>
      ) : null}
      {confirm ? (
        <ConfirmDialog
          title="Clean up this original upload?"
          message={`This asks the original server to remove only ownership-proved idle upload files for attempt ${confirm.row.attempt_id}. Protected or unproved files remain retained. This does not retry or insert the upload.`}
          confirmLabel="Clean up original upload"
          destructive
          labelledBy="pending-upload-cleanup-confirm"
          onConfirm={() => void cleanup(confirm)}
          onCancel={() => setConfirm(null)}
        />
      ) : null}
    </section>
  );
}
