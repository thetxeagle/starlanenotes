import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { readStorageIntegrity, type IntegrityObservation } from "@/lib/adminStorageIntegrity";
export function StorageIntegrityPanel() {
  const [observation, setObservation] = useState<IntegrityObservation | null>(null);
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);
  const held = useRef<AbortController | null>(null);
  useEffect(() => () => { held.current?.abort(); held.current = null; }, []);
  useEffect(() => {
    if (!observation) return;
    const timer = window.setInterval(() => {
      if (!observation.isCurrent()) { setObservation(null); setFailed(true); }
    }, 500);
    return () => window.clearInterval(timer);
  }, [observation]);
  const check = async () => {
    if (held.current) return;
    const controller = new AbortController(); held.current = controller;
    const timer = window.setTimeout(() => controller.abort(), 10000);
    setObservation(null); setFailed(false); setPending(true);
    try { const result = await readStorageIntegrity(controller.signal); if (held.current === controller && result.isCurrent()) setObservation(result); }
    catch { if (held.current === controller) setFailed(true); }
    finally { window.clearTimeout(timer); if (held.current === controller) { held.current = null; setPending(false); } }
  };
  return <section aria-labelledby="storage-integrity-title" aria-busy={pending} className="rounded-card border-border border p-3">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div><h2 id="storage-integrity-title" className="text-text text-sm font-medium">Storage integrity findings</h2><p className="text-text-muted mt-1 text-xs">Compare recorded paths with files. This check does not change or delete data.</p></div>
      <Button variant="secondary" size="sm" disabled={pending} onClick={() => void check()}>Run integrity check</Button>
    </div>
    {pending && <p role="status" className="text-text-muted mt-3 text-sm">Checking recorded storage paths…</p>}
    {failed && <p role="alert" className="text-danger mt-3 text-sm">Integrity check is unavailable. Check your connection and administrator access, then retry.</p>}
    {observation && <div className="mt-3 space-y-3 text-sm">
      <p role="status" className="text-text">{observation.report.healthy ? "No path findings in this observation." : `${observation.report.issues.length.toLocaleString()} path ${observation.report.issues.length === 1 ? "finding" : "findings"} in this observation.`}</p>
      <p className="text-text-muted">Observed <time dateTime={observation.observedAt}>{new Date(observation.observedAt).toLocaleString()}</time> in this browser. Files may have changed since this check. This is not a checksum audit, backup or verified restore.</p>
      <p className="text-text-muted">{observation.report.documents.toLocaleString()} pages · {observation.report.drafts.toLocaleString()} drafts · {observation.report.attachments.toLocaleString()} attachments checked</p>
      {observation.report.issues.length > 0 && <ul className="space-y-2">{observation.report.issues.slice(0, 100).map((issue, index) => <li key={index} className="border-border rounded-card min-w-0 border p-3 break-words">
        <p className="text-text font-medium">{issue.kind.replace(/_/g, " ")} · {issue.record_type}</p>
        <p className="text-text-muted">{issue.path}</p><p className="text-text">{issue.detail}</p>
        {issue.record_id && <p className="text-text-muted">Record {issue.record_id}</p>}
      </li>)}</ul>}
      {observation.report.issues.length > 100 && <p className="text-text-muted">Showing the first 100 findings. Review the complete administrator API observation before acting.</p>}
      <p className="text-text-muted">Review the named record and filesystem permissions with your administrator. Preserve unexpected files; this observation does not authorize cleanup.</p>
    </div>}
    {!pending && !failed && !observation && <p className="text-text-muted mt-3 text-sm">No integrity observation in this view. Run a check to inspect current recorded paths.</p>}
  </section>;
}
