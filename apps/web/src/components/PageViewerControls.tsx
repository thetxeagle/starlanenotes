import { useEffect, useRef, useState } from "react";
import type { PageViewerInspection, PageViewerReview } from "@starlane-notes/shared-types";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { capturePageViewerTransport, pageViewerPath, parsePageViewerInspection, parsePageViewerReview, parsePageViewerConfirmed, parsePageViewerPrincipals, createPageViewerUncertainty, loadPageViewerManagerProjection } from "@/lib/pageViewerManagement";

// This is a retained in-memory request, never a replay queue or proof of success.
const uncertainRequests = new Map<string, ReturnType<typeof createPageViewerUncertainty>>();
export function PageViewerControls({ spaceId, pageId, contextKey, verifyCurrent, onMenuOpenChange, guestsAvailable = false }: {
  spaceId: string; pageId: string; contextKey: string; verifyCurrent: () => void; onMenuOpenChange: (open: boolean) => void; guestsAvailable?: boolean;
}) {
  const live = useRef({ spaceId, pageId, contextKey, verifyCurrent });
  live.current = { spaceId, pageId, contextKey, verifyCurrent };
  const original = useRef({ spaceId, pageId, contextKey });
  const key = `${contextKey}/${spaceId}/${pageId}`;
  const current = () => {
    live.current.verifyCurrent();
    if (live.current.spaceId !== original.current.spaceId || live.current.pageId !== original.current.pageId || live.current.contextKey !== original.current.contextKey) throw new Error("Page sharing selection changed. Reopen Share page.");
  };
  const [transport] = useState(() => capturePageViewerTransport(current));
  const [inspection, setInspection] = useState<PageViewerInspection | null>(null);
  const [users, setUsers] = useState<{id: string; display_name: string; email: string}[]>([]);
  const [selected, setSelected] = useState("");
  const [catalogAvailable, setCatalogAvailable] = useState(false);
  const [review, setReview] = useState<PageViewerReview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [uncertainty] = useState(() => {
    const retained = uncertainRequests.get(key) ?? createPageViewerUncertainty();
    uncertainRequests.set(key, retained); return retained;
  });
  const [unknown, setUnknown] = useState(() => uncertainty.blocked());
  const [freshlyInspected, setFreshlyInspected] = useState(false);
  const mounted = useRef(false), generation = useRef(0), dispatching = useRef(false);
  const reportError = (e: unknown) => e instanceof Error ? e.message : "Page sharing unavailable.";
  async function inspect() {
    const request = ++generation.current; setBusy(true); setReview(null); setError("");setFreshlyInspected(false);
    try {
      current();
      const {inspection: access,users: catalog,catalog_available} = await loadPageViewerManagerProjection(
        () => transport(pageViewerPath(spaceId, pageId, "inspect"), "POST", undefined, v => parsePageViewerInspection(v, spaceId, pageId)),
        () => transport(`/spaces/${encodeURIComponent(spaceId)}/principals`, "GET", undefined, parsePageViewerPrincipals),
        current,
      );
      current(); if (!mounted.current || request !== generation.current) return;
      setInspection(access); setUsers(catalog);setCatalogAvailable(catalog_available); setSelected("");uncertainty.inspected();setFreshlyInspected(true);
    } catch (e) { if (mounted.current && request === generation.current) {setInspection(null);setUsers([]);setError(reportError(e));} }
    finally { if (mounted.current && request === generation.current) setBusy(false); }
  }
  useEffect(() => { mounted.current = true; void inspect(); return () => {mounted.current = false;generation.current++;onMenuOpenChange(false);}; /* original dialog session only */
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  async function prepare(user_id: string, action: "grant" | "revoke") {
    if (busy || unknown || dispatching.current) return;
    const request = ++generation.current; setBusy(true);setError("");setReview(null);
    const proposal = { user_id, action };
    try {
      const checked = await transport(pageViewerPath(spaceId, pageId, "review"), "POST", proposal, v => parsePageViewerReview(v, spaceId, pageId, proposal));
      current(); if (mounted.current && request === generation.current) setReview(checked);
    } catch (e) { if (mounted.current && request === generation.current) setError(reportError(e)); }
    finally { if (mounted.current && request === generation.current) setBusy(false); }
  }
  async function confirm() {
    if (!review || busy || unknown || dispatching.current) return;
    let originalReview: PageViewerReview;
    try {current();originalReview = structuredClone(review);uncertainty.dispatched(originalReview);}
    catch (reason) {setError(reportError(reason));return;}
    dispatching.current = true;setBusy(true);setError("");setFreshlyInspected(false);
    try {
      await transport(pageViewerPath(spaceId, pageId, "confirm"), "POST", {proposal:originalReview.proposal,source_digest:originalReview.source_digest,confirm:true}, v => parsePageViewerConfirmed(v, originalReview));
      current(); uncertainty.confirmed(originalReview);
      if (mounted.current) {setReview(null);setBusy(false);void inspect();}
    } catch {
      // No automatic resend, new source, or list-as-ack inference after dispatch.
      if (mounted.current) {setUnknown(true);setError("Confirmation could not be verified. Inspect current access before starting a separate review.");setBusy(false);}
    } finally { dispatching.current = false; }
  }
  return <section className="border-border space-y-3 border-t pt-4" aria-labelledby="page-viewer-title">
    <h3 id="page-viewer-title" className="text-text text-sm font-semibold">People with page access</h3>
    <p className="text-text-muted text-sm">Share this published page as viewer. {guestsAvailable && "Guests receive only explicitly shared page access. "}This does not grant space membership, drafts, editing, comments or access through private or restricted ancestors.</p>
    {error && <p role="alert" className="text-danger text-sm">{error}</p>}
    {unknown && <p role="status" className="text-text-muted text-sm">Acknowledgement unconfirmed. Your original request is retained for this session. Inspect current access; this does not confirm the earlier request. No automatic retry.</p>}
    {!unknown && uncertainty.retained().length > 0 && <p role="status" className="text-text-muted text-xs">The earlier unconfirmed request remains retained. This is a separate review.</p>}
    {unknown && freshlyInspected && <Button variant="secondary" disabled={busy} onClick={() => {
      try {current();uncertainty.separateReview();setUnknown(false);setReview(null);setSelected("");setFreshlyInspected(false);}
      catch (reason) {setError(reportError(reason));}
    }}>Start a separate review</Button>}
    {unknown && freshlyInspected && <p className="text-text-muted text-xs">This begins a new deliberate action. The earlier request remains unconfirmed and retained; it is not retried or acknowledged.</p>}
    {inspection && <ul className="space-y-2">{inspection.grants.map(g => <li key={g.user_id} className="flex items-center justify-between gap-3 text-sm"><span>{g.available ? g.display_name : "Unavailable recipient"} · Viewer</span><Button variant="secondary" disabled={busy || unknown} onClick={() => void prepare(g.user_id, "revoke")}>Review revoke</Button></li>)}</ul>}
    {inspection && !catalogAvailable && <p className="text-text-muted text-sm">Recipient selection is unavailable. Existing exact grants can still be reviewed for revoke when authorized.</p>}
    {inspection && catalogAvailable && <div className="flex items-center gap-2"><Select aria-label="Page viewer recipient" value={selected} disabled={busy || unknown} onOpenChange={onMenuOpenChange} onChange={e => setSelected(e.target.value)}><option value="">Choose a person</option>{users.map(u => <option key={u.id} value={u.id}>{u.display_name || u.email}</option>)}</Select><Button disabled={busy || unknown || !selected} onClick={() => void prepare(selected,"grant")}>Review</Button></div>}
    {review && !unknown && <div className="border-border rounded-card space-y-2 border p-3"><p className="text-text text-sm">{review.proposal.action === "grant" ? "Grant published-page viewer access to" : "Revoke page access for"} {review.recipient.display_name ?? "unavailable recipient"}?</p><p className="text-text-muted text-xs">The exact reviewed source is checked again before saving.</p><div className="flex gap-2"><Button variant="secondary" disabled={busy} onClick={() => setReview(null)}>Cancel review</Button><Button disabled={busy} onClick={() => void confirm()}>Confirm {review.proposal.action}</Button></div></div>}
    <Button variant="secondary" disabled={busy} onClick={() => void inspect()}>{busy ? "Checking…" : "Inspect current access"}</Button>
  </section>;
}
