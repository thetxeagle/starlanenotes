import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { DocumentIdentityIcon } from "@/components/DocumentIdentityIcon";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { capturePageViewerTransport, parseSharedPages } from "@/lib/pageViewerManagement";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import type { SharedPagePresentation } from "@starlane-notes/shared-types";
export default function SharedWithMePage() {
  const [pages, setPages] = useState<SharedPagePresentation[]>([]), [cursor, setCursor] = useState<string | null>(null);
  const [busy, setBusy] = useState(false), [error, setError] = useState("");
  const mounted = useRef(false), sequence = useRef(0);
  const transport = useRef<ReturnType<typeof capturePageViewerTransport> | null>(null);
  async function load(next: string | undefined) {
    if (!transport.current) return;
    const request = ++sequence.current;setBusy(true);setError("");
    try {
      const result = await transport.current("/shared-pages/inspect", "POST", next ? {cursor:next} : {}, parseSharedPages);
      if (!mounted.current || request !== sequence.current) return;
      setPages(old => {
        const combined = next ? [...old, ...result.pages] : result.pages;
        return combined.filter((p,i) => combined.findIndex(other => other.id === p.id && other.space_id === p.space_id) === i);
      });setCursor(result.next_cursor);
    } catch (reason) { if (mounted.current && request === sequence.current) { setPages([]);setCursor(null);setError(reason instanceof Error ? reason.message : "Shared pages unavailable."); } }
    finally { if (mounted.current && request === sequence.current) setBusy(false); }
  }
  useEffect(() => {
    mounted.current = true;
    if (getCurrentLaunchMode() === "server") {
      try {transport.current = capturePageViewerTransport(() => {if (!mounted.current) throw new Error("Shared pages closed.");});void load(undefined);}
      catch (reason) {setError(reason instanceof Error ? reason.message : "Shared pages unavailable.");}
    }
    return () => {mounted.current = false;sequence.current++;transport.current = null;};
  }, []);
  return <main className="mx-auto w-full max-w-4xl space-y-5 p-4 sm:p-6">
    <div className="flex items-center justify-between gap-3"><h1 className="text-text text-xl font-semibold">Shared with me</h1><Button variant="secondary" disabled={busy || !transport.current} onClick={() => void load(undefined)}>Refresh</Button></div>
    <p className="text-text-muted text-sm">Published pages shared directly with you as viewer. This list does not grant access to their spaces or private drafts.</p>
    {getCurrentLaunchMode() !== "server" && <p className="text-text-muted text-sm">Connect to a server to inspect shared pages.</p>}
    {error && <p role="alert" className="text-danger text-sm">{error}</p>}
    {busy && <p role="status" className="text-text-muted text-sm">Checking shared pages…</p>}
    {!busy && !error && getCurrentLaunchMode() === "server" && pages.length === 0 && <p className="text-text-muted text-sm">No available directly shared pages.</p>}
    <ul className="space-y-2">{pages.map(p => <li key={`${p.space_id}/${p.id}`}><Link className="focus-ring rounded-card border-border bg-surface hover:bg-hover flex items-center gap-3 border p-3" to={buildDocumentRoutePath(p.space_id,p.id)}><DocumentIdentityIcon icon={p.icon ?? undefined} /><span className="min-w-0"><span className="text-text block truncate text-sm font-medium">{p.title}</span><span className="text-text-muted text-xs">{p.human_identifier ? `${p.human_identifier} · ` : ""}Published · Viewer</span></span></Link></li>)}</ul>
    {cursor && <Button variant="secondary" disabled={busy} onClick={() => void load(cursor)}>Load more</Button>}
  </main>;
}
