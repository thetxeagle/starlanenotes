import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  createCurrentNativeArchiveReexport,
  inspectRetainedNativeArchiveDestination,
  retainedNativeArchiveOutcomes,
  reviewRetainedNativeArchiveDestination,
} from "@/lib/nativeCompleteVaultArchive";

type Inspection = Awaited<ReturnType<typeof inspectRetainedNativeArchiveDestination>>;
const stateLabel = (state: string) => ({
  published_verified: "Published archive bytes verified.",
  sealed_unpublished: "Complete archive staged; original destination is absent.",
  unresolved_output_absent: "Complete archive staged; original publication is unresolved and destination is absent.",
  published_output_missing: "Previously published destination is missing; retain the original evidence.",
  incomplete: "Original archive is incomplete; retain the evidence.",
  conflict: "Destination conflicts with original evidence; retain both files.",
}[state] ?? "Original archive outcome requires review.");

export function NativeArchiveOutcomeReviewControl({ busy }: { busy: boolean }) {
  const [rows, setRows] = useState<ReturnType<typeof retainedNativeArchiveOutcomes>>([]);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [acting, setActing] = useState<string | null>(null);
  const actionHeld = useRef(false);
  const [inspection, setInspection] = useState<{ attempt: string; value: Inspection } | null>(null);
  const refresh = useCallback(() => {
    try { setRows(retainedNativeArchiveOutcomes()); }
    catch (e) { setError(e instanceof Error ? e.message : "Retained archive evidence unavailable"); }
  }, []);
  useEffect(() => { refresh(); }, [busy, refresh]);

  const act = async (attempt: string, run: () => Promise<void>) => {
    if (busy || actionHeld.current) return;
    actionHeld.current = true;
    setActing(attempt); setError(null); setMessage(null);
    try { await run(); refresh(); }
    catch (e) { setError(e instanceof Error ? e.message : "Original archive outcome remains unknown; retain evidence"); refresh(); }
    finally { actionHeld.current = false; setActing(null); }
  };
  if (!rows.length && !error && !message) return null;
  return <div aria-busy={acting !== null} className="border-border mt-3 space-y-3 border-t pt-3">
    {rows.length > 0 && <p className="text-text-muted text-sm">Inspect the original named destination. Review does not publish or remove files. A new export uses a new chooser and operation; original evidence remains retained.</p>}
    {rows.map((row) => {
      const name = `${row.parent}/${row.leaf}`;
      const inspected = inspection?.attempt === row.attempt ? inspection.value : null;
      return <div key={row.attempt} className="space-y-2">
        <p className="text-text min-w-0 break-words text-sm">{name}</p>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="secondary" disabled={busy || acting !== null} onClick={() => void act(row.attempt, async () => {
            const value = await inspectRetainedNativeArchiveDestination(row.attempt, name);
            setInspection({ attempt: row.attempt, value });
          })}>Inspect archive outcome</Button>
          <Button size="sm" variant="secondary" disabled={busy || acting !== null} onClick={() => void act(row.attempt, async () => {
            await reviewRetainedNativeArchiveDestination(row.attempt, name);
            setInspection(null); setMessage("Named destination review recorded. Original files remain unchanged.");
          })}>Reviewed destination files</Button>
        </div>
        {acting === row.attempt && <p role="status" className="text-text-muted text-sm">Checking original archive evidence…</p>}
        {inspected && <div className="space-y-2">
          {!inspected.currentMatches && <p className="text-warning text-sm">The current vault revision differs. Review the original outcome; a new export cannot reuse this sealed observation.</p>}
          {inspected.operations.length === 0 && <p className="text-warning text-sm">No matching native journal was proved. Inspect the named destination manually; no publication authority is available.</p>}
          {inspected.operations.map((report) => <div key={report.operation} className="space-y-1">
            <p className="text-text text-sm">{stateLabel(report.state)}</p>
            <p className="text-text-muted break-words text-xs">Original operation {report.operation}</p>
            {inspected.currentMatches && ["sealed_unpublished", "unresolved_output_absent"].includes(report.state) && <>
              <p className="text-text-muted text-sm">Choose a new destination. Its complete bytes must match this sealed archive; this does not retry the original publication.</p>
              <Button size="sm" variant="secondary" disabled={busy || acting !== null} onClick={() => void act(row.attempt, async () => {
                const result = await createCurrentNativeArchiveReexport(row.attempt, report.operation);
                setMessage(result.cancelled ? "New export cancelled. Original evidence remains retained." : "New archive created with a new operation. Original destination evidence still requires review.");
              })}>Create a new export</Button>
            </>}
          </div>)}
        </div>}
      </div>;
    })}
    {message && <p role="status" className="text-text text-sm">{message}</p>}
    {error && <p role="alert" className="text-danger text-sm">{error}</p>}
  </div>;
}
