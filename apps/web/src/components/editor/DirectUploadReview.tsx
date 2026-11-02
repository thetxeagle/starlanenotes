import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { api, captureOfflineServerContext, getApiBase } from "@/lib/api";
import { verifyOfflineReplayContext } from "@/lib/offlineReplayOrigin";
import {
  retainedDirectUploads,
  reviewRetainedDirectUpload,
  markDirectUploadAdopted,
  type RetainedDirectUpload,
} from "@/lib/directUploadReview";
import type { AttachmentRecord } from "@/lib/dataClient";
export function DirectUploadReview({
  spaceId,
  documentId,
  draftId,
  onUse,
  referencedVersionIds,
}: {
  spaceId: string;
  documentId: string;
  draftId?:string;
  onUse: (attachment: AttachmentRecord) => Promise<void>;
  referencedVersionIds: () => ReadonlySet<string>;
}) {
  const [rows, setRows] = useState<RetainedDirectUpload[]>([]),
    [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const references = useRef(referencedVersionIds);
  references.current = referencedVersionIds;
  const generation = useRef(0);
  const ownedContext = useRef<ReturnType<typeof captureOfflineServerContext> | null>(null);
  const load = useCallback(async () => {
    const sequence = ++generation.current;
    setLoading(true);
    setError("");
    try {
      const context = captureOfflineServerContext();
      verifyOfflineReplayContext(context, getApiBase(), api.getToken());
      const records = await retainedDirectUploads({ space_id: spaceId, document_id: documentId,draft_id:draftId }, references.current());
      verifyOfflineReplayContext(context, getApiBase(), api.getToken());
      if (sequence === generation.current) {
        ownedContext.current = context;
        setRows(records);
      }
    } catch (error) {
      if (sequence === generation.current)
        setError(error instanceof Error ? error.message : "Retained uploads unavailable.");
    } finally {
      if (sequence === generation.current) setLoading(false);
    }
  }, [spaceId, documentId,draftId]);
  useEffect(() => {
    const lifecycle = generation;
    void load();
    return () => {
      lifecycle.current++;
    };
  }, [load]);
  const handleUseOriginal = async (row: RetainedDirectUpload) => {
    const sequence = ++generation.current;
    setBusy(true);
    setError("");
    try {
      const context = captureOfflineServerContext();
      verifyOfflineReplayContext(context, getApiBase(), api.getToken());
      const attachment = await reviewRetainedDirectUpload(
        { space_id: spaceId, document_id: documentId,draft_id:draftId },
        row.operationId,
      );
      verifyOfflineReplayContext(context, getApiBase(), api.getToken());
      if (sequence !== generation.current) return;
      await onUse(attachment);
      verifyOfflineReplayContext(context, getApiBase(), api.getToken());
      await markDirectUploadAdopted(
        { space_id: spaceId, document_id: documentId,draft_id:draftId },
        row.operationId,
      );
      if (sequence === generation.current) {
        setBusy(false);
        await load();
      }
    } catch (error) {
      if (sequence === generation.current)
        setError(error instanceof Error ? error.message : "Original upload review refused.");
    } finally {
      if (sequence === generation.current) setBusy(false);
    }
  };
  if (ownedContext.current) {
    try {
      verifyOfflineReplayContext(ownedContext.current, getApiBase(), api.getToken());
    } catch {
      return (
        <section
          className="rounded-card border-border space-y-2 border p-3"
          aria-label="Retained direct uploads"
        >
          <p className="text-text-muted text-sm">
            Original upload context changed. Reload to review uploads belonging to the current page
            session.
          </p>
          <Button
            size="sm"
            variant="secondary"
            disabled={busy || loading}
            onClick={() => void load()}
          >
            Reload retained uploads
          </Button>
        </section>
      );
    }
  }
  if (!rows.length && !loading && !error) return null;
  return (
    <section
      className="rounded-card border-border space-y-3 border p-3"
      aria-label="Retained direct uploads"
    >
      <h3 className="text-text text-sm font-medium">Retained uploads on this device</h3>
      <p className="text-text-muted text-sm">
        Review the original operation before using its acknowledged file on this page. A retry keeps
        the original file and operation ID.
      </p>
      {loading ? (
        <p className="text-text-muted text-sm" role="status">
          Checking retained uploads…
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="text-danger text-sm">
          {error}
        </p>
      ) : null}
      <ul className="space-y-3">
        {rows.map((row) => (
          <li className="space-y-2" key={row.operationId}>
            <p className="text-text text-sm break-words">{row.name}</p>
            <p className="text-text-muted text-xs break-all">
              {row.size} bytes · {row.state} · {row.operationId}
            </p>
            <Button
              size="sm"
              variant="secondary"
              disabled={busy || loading}
              onClick={() => void handleUseOriginal(row)}
            >
              {(row.state === "acknowledged" || row.state === "adopted")
                ? "Use acknowledged upload"
                : row.state === "prepared"
                  ? "Resume original upload"
                  : "Review and retry original upload"}
            </Button>
          </li>
        ))}
      </ul>
      <Button size="sm" variant="ghost" disabled={busy || loading} onClick={() => void load()}>
        Reload retained uploads
      </Button>
    </section>
  );
}
