import { useRef, useState } from "react";
import { AlertTriangle, CheckCircle2, FileArchive, Upload } from "lucide-react";
import { Button, RESPONSIVE_TOUCH_ACTION_CLASS } from "@/components/ui/button";
import { dataClient } from "@/lib/dataClient";
import {
  NOTION_IMPORT_LIMITS,
  buildNotionImportPlan,
  executeNotionImportPlan,
  finalizeNotionImportContent,
  preflightNotionExportArchive,
  type NotionImportExecutionProgress,
  type NotionImportExecutionResult,
  type NotionImportFinalizeResult,
  type NotionImportPlan,
  type NotionImportPreflight,
  type NotionImportTargetClient,
} from "@/lib/notionImport";

interface NotionImportSelection {
  file: File;
  bytes: Uint8Array;
  preflight: NotionImportPreflight;
  plan?: NotionImportPlan;
}

interface NotionImportSectionProps {
  spaceId: string;
  spaceName: string;
  client?: NotionImportTargetClient;
  onImported?: () => void | Promise<void>;
}

export function formatNotionImportReview(
  preflight: NotionImportPreflight,
  plan?: NotionImportPlan,
): string {
  const pageCount =
    plan?.documents.filter((document) => document.kind === "page").length ?? preflight.pageCount;
  const databaseCount =
    plan?.documents.filter((document) => document.kind === "database").length ??
    preflight.databaseCount;
  const folderCount = plan?.folders.length ?? 0;
  return `${pageCount} page${pageCount === 1 ? "" : "s"}, ${databaseCount} CSV database${databaseCount === 1 ? "" : "s"}, ${folderCount} folder${folderCount === 1 ? "" : "s"}, and ${preflight.assetCount} asset candidate${preflight.assetCount === 1 ? "" : "s"}.`;
}

export function formatNotionImportResult(
  execution: NotionImportExecutionResult,
  finalized: NotionImportFinalizeResult,
): string {
  const failureCount = execution.failures.length + finalized.failures.length;
  return `${execution.createdDocumentCount} draft${execution.createdDocumentCount === 1 ? "" : "s"} created, ${finalized.rewrittenPageLinkCount} page link${finalized.rewrittenPageLinkCount === 1 ? "" : "s"} repaired, ${finalized.uploadedAssetCount} asset${finalized.uploadedAssetCount === 1 ? "" : "s"} imported${failureCount ? `, ${failureCount} item${failureCount === 1 ? " needs" : "s need"} review` : ""}.`;
}

export function NotionImportSection({
  spaceId,
  spaceName,
  client = dataClient,
  onImported,
}: NotionImportSectionProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [selection, setSelection] = useState<NotionImportSelection | null>(null);
  const [reading, setReading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [progress, setProgress] = useState<NotionImportExecutionProgress | null>(null);
  const [progressPhase, setProgressPhase] = useState("");
  const [result, setResult] = useState<{
    execution: NotionImportExecutionResult;
    finalized: NotionImportFinalizeResult;
  } | null>(null);
  const [error, setError] = useState("");

  const inspectFile = async (file: File) => {
    setReading(true);
    setSelection(null);
    setResult(null);
    setProgress(null);
    setError("");
    try {
      if (file.size > NOTION_IMPORT_LIMITS.archiveBytes) {
        throw new Error("The selected ZIP exceeds Starlane's 256 MB browser import limit.");
      }
      const bytes = new Uint8Array(await file.arrayBuffer());
      const preflight = await preflightNotionExportArchive(file.name, bytes);
      const plan =
        preflight.status === "ready"
          ? await buildNotionImportPlan(file.name, bytes, preflight)
          : undefined;
      setSelection({ file, bytes, preflight, plan });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Unable to inspect this Notion export.");
    } finally {
      setReading(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  const runImport = async () => {
    if (!selection?.plan || selection.preflight.status !== "ready" || importing) return;
    setImporting(true);
    setResult(null);
    setError("");
    try {
      setProgressPhase("Creating folders and drafts");
      const execution = await executeNotionImportPlan(client, spaceId, selection.plan, setProgress);
      setProgressPhase("Repairing links and assets");
      setProgress(null);
      const finalized = await finalizeNotionImportContent(
        client,
        spaceId,
        selection.plan,
        selection.bytes,
        execution,
        setProgress,
      );
      setResult({ execution, finalized });
      await onImported?.();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "The Notion import could not finish.");
    } finally {
      setImporting(false);
      setProgress(null);
      setProgressPhase("");
    }
  };

  const allFailures = result ? [...result.execution.failures, ...result.finalized.failures] : [];
  const reviewWarnings = selection
    ? [...new Set([...selection.preflight.warnings, ...(selection.plan?.warnings ?? [])])]
    : [];
  const progressPercent = progress?.total
    ? Math.round((progress.completed / progress.total) * 100)
    : 0;

  return (
    <section className="rounded-card border-border bg-surface border">
      <div className="border-border flex items-start gap-3 border-b px-4 py-3">
        <FileArchive size={17} className="text-text-muted mt-0.5 shrink-0" aria-hidden="true" />
        <div className="min-w-0">
          <h2 className="text-text text-base font-semibold">Import from Notion</h2>
          <p className="text-text-muted mt-0.5 text-xs leading-5">
            Export from Notion as Markdown &amp; CSV with subpages, then review the ZIP before
            Starlane creates anything. Imported pages remain drafts until you publish them.
          </p>
        </div>
      </div>

      <div className="space-y-4 p-4">
        <input
          ref={fileInputRef}
          type="file"
          accept=".zip,application/zip"
          className="sr-only"
          aria-label="Choose Notion Markdown and CSV ZIP"
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) void inspectFile(file);
          }}
        />

        <div
          className="rounded-card border-border bg-bg flex min-h-32 flex-col items-center justify-center border border-dashed px-4 py-6 text-center"
          onDragOver={(event) => event.preventDefault()}
          onDrop={(event) => {
            event.preventDefault();
            const file = event.dataTransfer.files?.[0];
            if (file && !reading && !importing) void inspectFile(file);
          }}
        >
          <Upload size={20} className="text-text-muted" aria-hidden="true" />
          <p className="text-text mt-2 text-sm font-medium">
            {reading ? "Inspecting export…" : "Choose or drop the Notion ZIP"}
          </p>
          <p className="text-text-muted mt-1 max-w-lg text-xs leading-5">
            The browser importer accepts ZIP files up to 256 MB and never writes before the review
            below is ready.
          </p>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            className={`mt-3 ${RESPONSIVE_TOUCH_ACTION_CLASS}`}
            disabled={reading || importing}
            onClick={() => fileInputRef.current?.click()}
          >
            <Upload size={14} />
            {selection ? "Choose another ZIP" : "Choose ZIP"}
          </Button>
        </div>

        {error && (
          <div
            role="alert"
            className="rounded-input border-danger/30 bg-danger/10 text-danger border px-3 py-2 text-sm"
          >
            {error}
          </div>
        )}

        {selection && (
          <div className="rounded-card border-border divide-border divide-y border">
            <div className="flex flex-wrap items-start justify-between gap-3 px-4 py-3">
              <div className="min-w-0">
                <p className="text-text truncate text-sm font-medium">{selection.file.name}</p>
                <p className="text-text-muted mt-0.5 text-xs">
                  {formatFileSize(selection.file.size)} expanded to{" "}
                  {formatFileSize(selection.preflight.totalExpandedBytes)}
                </p>
              </div>
              <span
                className={`rounded px-2 py-1 text-xs font-medium ${
                  selection.preflight.status === "ready"
                    ? "bg-success/10 text-success"
                    : "bg-danger/10 text-danger"
                }`}
              >
                {selection.preflight.status === "ready" ? "Ready for review" : "Blocked"}
              </span>
            </div>

            <div className="px-4 py-3">
              <p className="text-text text-sm font-medium">Import review</p>
              <p className="text-text-muted mt-1 text-sm leading-6">
                {formatNotionImportReview(selection.preflight, selection.plan)}
              </p>
              {selection.plan && selection.plan.skippedPaths.length > 0 && (
                <p className="text-warning mt-2 text-xs">
                  {selection.plan.skippedPaths.length} empty or oversized item
                  {selection.plan.skippedPaths.length === 1 ? "" : "s"} will be skipped.
                </p>
              )}
            </div>

            {(selection.preflight.blockers.length > 0 || reviewWarnings.length > 0) && (
              <div className="px-4 py-3">
                <div className="flex items-center gap-2">
                  <AlertTriangle
                    size={15}
                    className={selection.preflight.blockers.length ? "text-danger" : "text-warning"}
                    aria-hidden="true"
                  />
                  <p className="text-text text-sm font-medium">
                    {selection.preflight.blockers.length ? "Import blockers" : "Review notes"}
                  </p>
                </div>
                <ul className="text-text-muted mt-2 list-disc space-y-1 pl-5 text-xs leading-5">
                  {[...selection.preflight.blockers, ...reviewWarnings].slice(0, 8).map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              </div>
            )}

            <div className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
              <p className="text-text-muted text-xs">
                Existing pages are never overwritten. Matching folders are reused; title collisions
                receive a Notion import suffix.
              </p>
              <Button
                type="button"
                size="sm"
                className={RESPONSIVE_TOUCH_ACTION_CLASS}
                disabled={
                  !selection.plan ||
                  selection.plan.documents.length === 0 ||
                  selection.preflight.status !== "ready" ||
                  importing
                }
                onClick={() => void runImport()}
              >
                {importing ? "Importing…" : `Import into ${spaceName}`}
              </Button>
            </div>
          </div>
        )}

        {importing && (
          <div
            className="rounded-card border-border border px-4 py-3"
            role="status"
            aria-live="polite"
          >
            <div className="flex items-center justify-between gap-3 text-xs">
              <span className="text-text font-medium">{progressPhase}</span>
              <span className="text-text-muted">
                {progress ? `${progress.completed} / ${progress.total}` : "Starting…"}
              </span>
            </div>
            <div
              className="bg-surface-raised mt-2 h-1.5 overflow-hidden rounded"
              role="progressbar"
              aria-label={progressPhase}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={progressPercent}
            >
              <div
                className="bg-primary h-full transition-[width] duration-150"
                style={{ width: `${progressPercent}%` }}
              />
            </div>
            {progress?.label && (
              <p className="text-text-muted mt-2 truncate text-xs">{progress.label}</p>
            )}
          </div>
        )}

        {result && (
          <div
            className={`rounded-card border px-4 py-3 ${
              allFailures.length
                ? "border-warning/30 bg-warning/10"
                : "border-success/30 bg-success/10"
            }`}
            role="status"
          >
            <div className="flex items-start gap-2">
              {allFailures.length ? (
                <AlertTriangle
                  size={16}
                  className="text-warning mt-0.5 shrink-0"
                  aria-hidden="true"
                />
              ) : (
                <CheckCircle2
                  size={16}
                  className="text-success mt-0.5 shrink-0"
                  aria-hidden="true"
                />
              )}
              <div className="min-w-0">
                <p className="text-text text-sm font-medium">
                  {allFailures.length ? "Import finished with review items" : "Import complete"}
                </p>
                <p className="text-text-muted mt-1 text-sm leading-6">
                  {formatNotionImportResult(result.execution, result.finalized)}
                </p>
              </div>
            </div>
            {allFailures.length > 0 && (
              <ul className="text-danger mt-3 list-disc space-y-1 pl-5 text-xs leading-5">
                {allFailures.slice(0, 8).map((failure) => (
                  <li key={`${failure.kind}-${failure.key}-${failure.reason}`}>
                    {failure.label}: {failure.reason}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </div>
    </section>
  );
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
