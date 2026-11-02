import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  DialogShell,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogBody,
  DialogFooter,
  DialogCloseAction,
} from "@/components/ui/dialog-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { dataClient } from "@/lib/dataClient";
import { isTauri, api, captureOfflineServerContext, getApiBase } from "@/lib/api";
import { verifyOfflineReplayContext } from "@/lib/offlineReplayOrigin";
import {
  fetchExactAttachmentVersion,
  type SelectedAttachmentVersion,
  type AttachmentHistoryHead,
} from "@/lib/nativeAttachmentProduct";

import {
  prepareAttachmentReplacement,
  runAttachmentReplacement,
  retainedAttachmentReplacements,
  type ReplacementResult,
} from "@/lib/attachmentReplacementOperation";
import {
  saveBrowserAttachmentVersion,
  supportsBrowserAttachmentSave,
  browserAttachmentSaveNeedsReview,
  browserAttachmentSavePending,
  acknowledgeBrowserAttachmentSaveReview,
} from "@/lib/browserAttachmentSave";
interface Props {
  spaceId: string;
  documentId: string;
  versionId: string;
  draftId?:string;
  onClose: () => void;
  onReplaced?: (result: ReplacementResult) => void | Promise<void>;
  verifySelection?: () => void;
  captureReplacementSelection?: () => (() => void);
  replacementKind?: "image";
}
interface VersionLabel {
  version_id: string;
  version_number: number;
  original_name: string;
  byte_length: number;
  original_created_at: string;
  mime_type: string;
}
function label(selection: SelectedAttachmentVersion): VersionLabel {
  return JSON.parse(selection.rawVersion) as VersionLabel;
}

/** History opens by immutable block pin, never by current filename or an inferred family. */
export function AttachmentHistoryDialog({
  spaceId,
  documentId,
  versionId,
  draftId,
  onClose,
  onReplaced,
  verifySelection,
  captureReplacementSelection,
  replacementKind,
}: Props) {
  const id = useId(),
    generation = useRef(0),
    [records, setRecords] = useState<SelectedAttachmentVersion[]>([]),
    [total, setTotal] = useState(0),
    [currentHead,setCurrentHead]=useState<AttachmentHistoryHead|null>(null),
    [after, setAfter] = useState<number | null>(null),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [busy, setBusy] = useState(false),
    [replacement, setReplacement] = useState<string | null>(null),
    [retained, setRetained] = useState<Array<{ id: string; name: string; state: string }>>([]),
    [preview, setPreview] = useState<{ name: string; text: string; truncated: boolean } | null>(
      null,
    );
  const paging = useRef({ records, after });
  paging.current = { records, after };
  const load = useCallback(
    async (append: boolean) => {
      const prior = paging.current;
      const sequence = ++generation.current;
      setBusy(true);
      setError("");
      setNotice("");
      setPreview(null);
      if (!append) {
        setRecords([]);
        setAfter(null);
        setTotal(0);
        setCurrentHead(null);
      }
      try {
        const context = captureOfflineServerContext();
        const verify = () => {
          verifyOfflineReplayContext(context, getApiBase(), api.getToken());
          verifySelection?.();
        };
        verify();
        const selected = await fetchExactAttachmentVersion(
          { space_id: spaceId, document_id: documentId },
          versionId,
        );
        verify();
        const logicalId = JSON.parse(selected.rawVersion).logical_attachment_id as string;
        const pending = await retainedAttachmentReplacements(selected,draftId);
        verify();
        if (sequence !== generation.current) return;
        setRetained(pending);
        const page = await dataClient.listAttachmentVersionHistory(
          spaceId,
          documentId,
          logicalId,
          append ? (prior.after ?? 0) : 0,
        );
        verify();
        if (sequence !== generation.current) return;
        const rows = append ? [...prior.records, ...page.records] : page.records;
        const ids = new Set(rows.map((row) => label(row).version_id));
        if (ids.size !== rows.length)
          throw new Error("History changed while paging; reload attachment history.");
        setRecords(rows);
        setTotal(page.total);
        setCurrentHead(page.currentHead??null);
        setAfter(page.nextAfter);
      } catch (cause) {
        if (sequence === generation.current) {
          setError(cause instanceof Error ? cause.message : "Attachment history unavailable.");
          if (append) setAfter(null);
        }
      } finally {
        if (sequence === generation.current) setBusy(false);
      }
    },
    [spaceId, documentId, versionId, draftId, verifySelection],
  );
  useEffect(() => {
    const lifecycle = generation;
    void load(false);
    return () => {
      lifecycle.current++;
    };
  }, [load]);
  const act = async (row: SelectedAttachmentVersion, kind: "save" | "preview") => {
    const sequence = ++generation.current;
    setBusy(true);
    setError("");
    setNotice("");
    setPreview(null);
    try {
      verifySelection?.();
      if (kind === "save") {
        if (isTauri()) await dataClient.saveSelectedAttachmentVersion(spaceId, documentId, row);
        else await saveBrowserAttachmentVersion(row);
      } else {
        const result = await dataClient.previewSelectedAttachmentVersionText(
          spaceId,
          documentId,
          row,
        );
        if (sequence === generation.current)
          setPreview({ name: label(row).original_name, ...result });
      }
      verifySelection?.();
    } catch (cause) {
      if (sequence === generation.current)
        setError(cause instanceof Error ? cause.message : "Selected attachment action failed.");
    } finally {
      if (sequence === generation.current) setBusy(false);
    }
  };
  const replace = async (row: SelectedAttachmentVersion, file: File) => {
    if (busy) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      verifySelection?.();
      const verifyTarget = captureReplacementSelection?.();
      verifyTarget?.();
      if (replacementKind === "image" && !file.type.startsWith("image/"))
        throw new Error("Choose an image for this attachment replacement.");
      if (!currentHead||label(row).version_id!==currentHead.version_id||label(row).version_number!==currentHead.revision)
        throw new Error("Reload history and choose its current version.");
      const operation = await prepareAttachmentReplacement(row, currentHead.revision, file,draftId);
      setReplacement(operation);
      verifySelection?.();
      verifyTarget?.();
      const result = await runAttachmentReplacement(operation);
      verifySelection?.();
      verifyTarget?.();
      await onReplaced?.(result);
      setReplacement(null);
      await load(false);
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Replacement outcome unknown; retain original operation.",
      );
    } finally {
      setBusy(false);
    }
  };
  const retryReplacement = async (id = replacement) => {
    if (!id || busy) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      verifySelection?.();
      const verifyTarget = captureReplacementSelection?.();
      verifyTarget?.();
      const result = await runAttachmentReplacement(id);
      verifySelection?.();
      verifyTarget?.();
      await onReplaced?.(result);
      setReplacement(null);
      await load(false);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Replacement retry refused.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <DialogShell labelledBy={id} describedBy={`${id}-description`} onClose={onClose} maxWidth="lg">
      <DialogHeader>
        <div>
          <DialogTitle id={id}>Attachment history</DialogTitle>
          <DialogDescription id={`${id}-description`}>
            Choose an immutable version. Saving a file leaves the page and its published attachment
            unchanged.
          </DialogDescription>
        </div>
        <DialogCloseAction onClick={onClose} />
      </DialogHeader>
      <DialogBody className="space-y-3">
        <p className="text-text-muted text-sm" role="status">
          {busy ? "Loading selected attachment…" : `${records.length} of ${total} versions`}
        </p>
        {notice ? (
          <p role="status" className="text-success text-sm">
            {notice}
          </p>
        ) : null}
        {error ? (
          <div role="alert" className="space-y-2">
            <p className="text-danger text-sm">{error}</p>
            <Button variant="secondary" disabled={busy} onClick={() => void load(false)}>
              Reload history
            </Button>
          </div>
        ) : null}
        {!busy && !error && !records.length ? (
          <p className="text-text-muted text-sm">No attachment versions are available.</p>
        ) : null}
        <ol className="divide-border divide-y">
          {records.map((row) => {
            const v = label(row);
            return (
              <li key={v.version_id} className="space-y-2 py-3">
                <div className="min-w-0">
                  <p className="text-text font-medium break-words">{v.original_name}</p>
                  <p className="text-text-muted text-sm">
                    Version {v.version_number}
                    {v.version_id === versionId ? " · This page’s selected version" : ""} ·{" "}
                    {v.byte_length.toLocaleString()} bytes
                  </p>
                  <p className="text-text-muted text-xs break-words">{v.original_created_at}</p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={
                      busy ||
                      (!isTauri() && !supportsBrowserAttachmentSave()) ||
                      (!isTauri() && browserAttachmentSaveNeedsReview(row))
                    }
                    onClick={() => void act(row, "save")}
                  >
                    Save version {v.version_number}
                  </Button>
                  {v.mime_type.startsWith("text/") ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy || !isTauri()}
                      onClick={() => void act(row, "preview")}
                    >
                      Preview text
                    </Button>
                  ) : null}
                  {!isTauri() && browserAttachmentSaveNeedsReview(row) ? (
                    <>
                      <p className="text-warning text-sm break-words">
                        Save outcome unknown:{" "}
                        {browserAttachmentSavePending(row)
                          .map((record) => record.destinationName)
                          .join(", ")}
                        . Inspect these destinations before another Save.
                      </p>
                      <Button
                        variant="secondary"
                        size="sm"
                        disabled={busy}
                        onClick={() => {
                          try {
                            acknowledgeBrowserAttachmentSaveReview(row);
                            setError("");
                            setNotice(
                              "Previous destination review recorded. Choose another Save explicitly.",
                            );
                          } catch (cause) {
                            setError(cause instanceof Error ? cause.message : "Review refused.");
                          }
                        }}
                      >
                        I inspected all previous Save destinations
                      </Button>
                    </>
                  ) : null}
                </div>
              </li>
            );
          })}
        </ol>
        {!isTauri() ? (
          <p className="text-text-muted text-sm">
            Browser Save requires direct file streaming support. Receipt-backed text preview
            requires the desktop app.
          </p>
        ) : null}
        {retained.length && !replacement ? (
          <section aria-label="Retained replacements" className="space-y-2">
            <h3 className="text-text font-medium">Retained replacements</h3>
            {retained.map((item) => (
              <div key={item.id} className="space-y-1">
                <p className="text-text-muted text-sm break-words">
                  {item.name} · {item.state} · {item.id}
                </p>
                <Button
                  variant="secondary"
                  disabled={busy}
                  onClick={() => void retryReplacement(item.id)}
                >
                  Review original replacement
                </Button>
              </div>
            ))}
          </section>
        ) : null}
        {replacement ? (
          <div className="space-y-2">
            <p className="text-warning text-sm">
              Replacement {replacement} is retained. Retry its original file and target; do not
              upload another copy.
            </p>
            <Button variant="secondary" disabled={busy} onClick={() => void retryReplacement()}>
              Retry original replacement
            </Button>
          </div>
        ) : null}
        {onReplaced &&
        !replacement &&
        !retained.length &&
        records.some((row) => label(row).version_id === currentHead?.version_id) ? (
          <div className="space-y-2">
            <label htmlFor={`${id}-replacement`} className="text-text text-sm">
              Replace current file
            </label>
            <Input
              id={`${id}-replacement`}
              type="file"
              accept={replacementKind === "image" ? "image/*" : undefined}
              disabled={busy}
              onChange={(event) => {
                const file = event.currentTarget.files?.[0];
                event.currentTarget.value = "";
                const current = records.find((row) => label(row).version_id === currentHead?.version_id);
                if (file && current) void replace(current, file);
              }}
            />
            <p className="text-text-muted text-sm">
              Previous bytes stay in attachment history. Your draft selects the replacement after
              acknowledgement; publishing remains a separate action.
            </p>
          </div>
        ) : null}
        {after !== null ? (
          <Button variant="secondary" disabled={busy} onClick={() => void load(true)}>
            Load more versions
          </Button>
        ) : null}
        {preview ? (
          <section aria-label={`Preview of ${preview.name}`} className="space-y-2">
            <h3 className="text-text font-medium">{preview.name}</h3>
            {preview.truncated ? (
              <p className="text-warning text-sm">
                Preview limited to the first 64 KiB. Save this version for the complete file.
              </p>
            ) : null}
            <pre className="bg-surface text-text border-border max-h-64 overflow-auto rounded border p-3 text-sm break-words whitespace-pre-wrap">
              {preview.text}
            </pre>
          </section>
        ) : null}
      </DialogBody>
      <DialogFooter>
        <Button variant="secondary" onClick={onClose}>
          Close
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
