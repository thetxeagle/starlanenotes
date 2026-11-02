import { snapshotRecoveryUnavailableReason, SnapshotRecoveryUnavailableError } from "@/lib/publicationSnapshotRecovery";
import { Button } from "@/components/ui/button";
import { DialogShell, DialogHeader, DialogTitle, DialogBody, DialogFooter, DialogCloseAction } from "@/components/ui/dialog-shell";
import { LoadingState } from "@/components/ui/loading-state";
import type { PublicationSnapshotDetail, PublicationSnapshotRaw } from "@/lib/documentPublishedHistory";

const field = (value: string | null) => value === null ? "None" : value === "" ? "Empty text" : value;
export function PublishedSnapshotPreview({ detail, raw, loading, rawLoading, error, onClose, onReadRaw, onDownloadRaw, onRecover }: {
  detail: PublicationSnapshotDetail | null;
  raw: PublicationSnapshotRaw | null;
  loading: boolean;
  rawLoading: boolean;
  error?: string;
  onRecover?(): void;
  onClose(): void;
  onReadRaw(): void;
  onDownloadRaw(): void;
}) {
  const recoveryReason = detail ? snapshotRecoveryUnavailableReason(detail) : null;
  return <DialogShell labelledBy="published-snapshot-preview-title" maxWidth="xl" onClose={onClose}>
    <DialogHeader><DialogTitle id="published-snapshot-preview-title">Published snapshot</DialogTitle><DialogCloseAction onClick={onClose} /></DialogHeader>
    <DialogBody className="min-h-0 space-y-4 overflow-y-auto">
      {loading ? <LoadingState label="Loading published snapshot" /> : error ? <p role="alert" className="text-text-muted text-sm">{error}</p> : detail && <>
        <p className="text-text-muted text-sm">Read-only history. Previewing does not change this page, its drafts, properties, or audience.</p>
        {onRecover && <p className="text-text-muted text-sm">{recoveryReason ? new SnapshotRecoveryUnavailableError(recoveryReason).message : "Recovery creates a separate private draft from the captured title, body, icon and cover reference. Properties, audience and asset bytes are not copied. The current editor is kept unchanged."}</p>}
        {detail.kind === "baseline" && <p className="text-text-muted text-sm">Previous published page captured before replacement. Properties and asset references were observed at capture; the original publisher is not recorded by this capture.</p>}
        {detail.binding === "legacy_content_v1" && <p className="text-text-muted text-sm">Earlier publication binding: comparison observations were captured at commit and were not part of the original prepared source.</p>}
        <dl className="text-text space-y-1 text-sm [overflow-wrap:anywhere]">
          <div><dt className="inline font-medium">Title: </dt><dd className="inline">{field(detail.content.title)}</dd></div>
          <div><dt className="inline font-medium">Icon: </dt><dd className="inline">{field(detail.content.icon)}</dd></div>
          <div><dt className="inline font-medium">Cover reference: </dt><dd className="inline">{field(detail.content.cover_reference)}</dd></div>
          <div><dt className="inline font-medium">Published: </dt><dd className="inline">{detail.published_at}</dd></div>
          <div><dt className="inline font-medium">Captured: </dt><dd className="inline">{detail.captured_at}</dd></div>
          <div><dt className="inline font-medium">Captured by: </dt><dd className="inline">{detail.captured_by}</dd></div>
        </dl>
        {detail.content.extraction === "raw_only" ? <section aria-label="Preserved raw source" className="space-y-2">
          <p className="text-text-muted text-sm">The legacy header could not be proven. Original bytes are preserved; title and body are not reconstructed from this header.</p>
          {raw ? raw.text === null ? <p className="text-text-muted text-sm">These bytes are not valid UTF-8 text. Download the preserved file to keep the exact source.</p> : <SnapshotSource label="Preserved raw Markdown" value={raw.text} /> : <Button variant="secondary" disabled={rawLoading} onClick={onReadRaw}>{rawLoading ? "Loading preserved source…" : "View preserved source"}</Button>}
        </section> : <SnapshotSource label="Captured portable Markdown" value={detail.content.portable_markdown ?? ""} />}
        <details className="text-text-muted space-y-2 text-sm">
          <summary className="cursor-pointer">Properties at capture · comparison only</summary>
          <p className="mt-2">Captured schema and stored values are shown for comparison. They are not applied to the current page.</p>
          {detail.comparisons.properties_schema.map((schema) => {
            const value = detail.comparisons.property_values.find((row) => row.property_id === schema.id);
            return <div key={schema.id} className="border-border mt-2 rounded border p-2 [overflow-wrap:anywhere]">
              <p className="text-text font-medium">{schema.name || "Unnamed property"}</p><p>Type: {schema.property_type} · ID: {schema.id}</p>
              <SnapshotSource label={`Captured ${schema.name || schema.id} value`} value={value ? field(value.value) : "No captured value"} />
              {schema.options_raw !== null && <SnapshotSource label={`Captured ${schema.name || schema.id} options`} value={schema.options_raw} />}
            </div>;
          })}
          {detail.comparisons.property_values.filter((value) => !detail.comparisons.properties_schema.some((schema) => schema.id === value.property_id)).map((value) => <div key={value.property_id} className="mt-2"><p>Unresolved property: {value.property_id}</p><SnapshotSource label="Unresolved captured property value" value={field(value.value)} /></div>)}
          {detail.comparisons.properties_schema.length === 0 && detail.comparisons.property_values.length === 0 && <p>No properties were captured.</p>}
        </details>
        <details className="text-text-muted space-y-2 text-sm [overflow-wrap:anywhere]">
          <summary className="cursor-pointer">Referenced assets · bytes not included</summary>
          <p className="mt-2">Availability is not checked. Downloading assets requires current authorization and existing bytes. This snapshot does not retain their file contents.</p>
          {detail.comparisons.attachments.map((asset) => <p key={asset.id} className="mt-2">{asset.original_name} · {asset.mime_type} · {asset.size_bytes} bytes at capture · reference {asset.id}</p>)}
          {detail.comparisons.drawings.map((drawing) => <p key={drawing.id} className="mt-2">{drawing.filename} · drawing reference {drawing.id}</p>)}
          {detail.comparisons.attachments.length === 0 && detail.comparisons.drawings.length === 0 && <p>No asset records were captured.</p>}
        </details>
        <details className="text-text-muted text-sm [overflow-wrap:anywhere]"><summary className="cursor-pointer">Source proof</summary><p className="mt-2">Snapshot: {detail.snapshot_digest}</p><p>Raw SHA-256: {detail.content.raw_sha256}</p><p>Raw length: {detail.content.raw_byte_length} bytes</p><p>Capture operation: {detail.capture_operation_id}</p><p>Captured space: {detail.captured_space_id}</p></details>
      </>}
    </DialogBody>
    <DialogFooter>{onRecover && detail && !loading && !error && !recoveryReason && <Button disabled={rawLoading} onClick={onRecover}>Recover into a private draft</Button>}{detail && !loading && !error && <Button variant="secondary" disabled={rawLoading} onClick={onDownloadRaw}>Download preserved Markdown</Button>}<Button variant="secondary" onClick={onClose}>Close</Button></DialogFooter>
  </DialogShell>;
}
export function SnapshotSource({label,value}:{label:string;value:string}) {
  return <pre aria-label={label} tabIndex={0} className="border-border bg-surface-raised text-text mt-2 max-h-80 overflow-auto rounded border p-3 text-sm [overflow-wrap:anywhere] whitespace-pre-wrap">{value}</pre>;
}
