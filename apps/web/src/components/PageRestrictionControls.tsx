import { useEffect, useId, useRef, useState } from "react";
import { dataClient } from "@/lib/dataClient";
import { normalizeRestrictionProposal } from "@/lib/pageRestrictionManagement";
import type { RestrictionContextGuard, RestrictionInspection, RestrictionPreview, RestrictionProposal } from "@/lib/pageRestrictionManagement";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";

type Phase = "loading" | "ready" | "reviewing" | "confirming";
export function restrictionProposalFromInspection(inspection: RestrictionInspection): RestrictionProposal {
  return { mode: inspection.policy.mode, allowlist: {
    users: inspection.policy.allowlist.users.map((entry) => entry.id),
    groups: inspection.policy.allowlist.groups.map((entry) => entry.id),
  } };
}
export function restrictionSelectionLabel(inspection: RestrictionInspection | null, kind: "users" | "groups", id: string): string {
  const entry = kind === "users" ? inspection?.eligible_recipients.users.find((candidate) => candidate.id === id) : inspection?.eligible_recipients.groups.find((candidate) => candidate.id === id);
  return entry ? ("display_name" in entry ? entry.display_name || "Unnamed person" : entry.name || "Unnamed group") : `Unavailable ${kind === "users" ? "person" : "group"}: ${id}`;
}
export function restrictionReviewIsCurrent(review: { response: RestrictionPreview; generation: number }, generation: number, now = Date.now()): boolean {
  return review.generation === generation && Date.parse(review.response.expires_at) > now;
}
/** Restriction state is independent of the ordinary page View report. No role-name inference. */
export function PageRestrictionControls({ spaceId, pageId, contextKey, accessRefreshVersion, assertContext, onConfirmed, onMenuOpenChange }: {
  spaceId: string; pageId: string; contextKey: string; accessRefreshVersion: number;
  assertContext: RestrictionContextGuard; onConfirmed: () => void;
  onMenuOpenChange?: (open: boolean) => void;
}) {
  const headingId = useId();
  const modeId = useId();
  const peopleId = useId();
  const groupsId = useId();
  const mounted = useRef(false);
  const original = useRef({ spaceId, pageId, contextKey });
  const sequence = useRef(0);
  const generation = useRef(0);
  const dirty = useRef(false);
  const [menus, setMenus] = useState({ mode: false, person: false, group: false });
  const menuOwnerRef = useRef(onMenuOpenChange);
  menuOwnerRef.current = onMenuOpenChange;
  useEffect(() => { menuOwnerRef.current?.(menus.mode || menus.person || menus.group); }, [menus]);
  useEffect(() => () => { menuOwnerRef.current?.(false); }, []);
  function menuChanged(kind: keyof typeof menus, open: boolean) {
    setMenus((current) => current[kind] === open ? current : { ...current, [kind]: open });
  }
  const [inspection, setInspection] = useState<RestrictionInspection | null>(null);
  const [proposal, setProposal] = useState<RestrictionProposal | null>(null);
  const [review, setReview] = useState<{ response: RestrictionPreview; proposal: RestrictionProposal; generation: number } | null>(null);
  const [phase, setPhase] = useState<Phase>("loading");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [reopen, setReopen] = useState(false);
  const [person, setPerson] = useState("");
  const [group, setGroup] = useState("");
  const busy = phase !== "ready";
  function invalidateReview() { generation.current++; setReview(null); }
  function fail(reason: unknown, unknownOutcome = false) {
    if (!mounted.current) return;
    setInspection(null); invalidateReview(); setPerson(""); setGroup("");
    try { assertContext(); }
    catch {
      setProposal(null); dirty.current = false; setReopen(true); setNotice("");
      setError("Account or page changed. Close and reopen Share page."); setPhase("ready"); return;
    }
    setError(unknownOutcome
      ? "Confirmation outcome is unknown. Inspect current restrictions before reviewing again."
      : reason instanceof Error ? reason.message : "Unable to inspect restrictions. Refresh to retry.");
    setPhase("ready");
  }
  async function load(preserveEdits = true, keepConfirmedNotice = false) {
    const request = ++sequence.current;
    invalidateReview(); setInspection(null); setPerson(""); setGroup(""); setError("");
    if (!keepConfirmedNotice) setNotice("");
    setPhase("loading");
    try {
      assertContext();
      const next = await dataClient.inspectPageRestrictions(spaceId, pageId, assertContext);
      assertContext();
      if (!mounted.current || request !== sequence.current) return false;
      setInspection(next);
      if (!preserveEdits || !dirty.current) { setProposal(restrictionProposalFromInspection(next)); dirty.current = false; }
      setPhase("ready"); return true;
    } catch (reason) { if (mounted.current && request === sequence.current) fail(reason); return false; }
  }
  useEffect(() => {
    mounted.current = true; void load();
    return () => { mounted.current = false; sequence.current++; generation.current++; };
    // Original request context is fixed until the dialog is closed and reopened.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (spaceId !== original.current.spaceId || pageId !== original.current.pageId || contextKey !== original.current.contextKey) {
      sequence.current++; generation.current++; setInspection(null); setProposal(null); setReview(null);
      setPerson(""); setGroup(""); setNotice(""); setError("Account or page changed. Close and reopen Share page.");
      setReopen(true); setPhase("ready");
    }
  }, [spaceId, pageId, contextKey]);
  useEffect(() => {
    generation.current++; setReview(null);
    setPhase((current) => current === "reviewing" ? "ready" : current);
  }, [accessRefreshVersion]);
  useEffect(() => {
    if (!review) return;
    const timer = window.setInterval(() => {
      if (Date.parse(review.response.expires_at) <= Date.now()) {
        generation.current++; setReview(null); setInspection(null);
        setPhase((current) => current === "confirming" ? current : "ready");
        setNotice("Restriction review expired. Inspect and review again before confirming.");
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [review]);
  function edit(next: RestrictionProposal) {
    try { assertContext(); } catch (reason) { fail(reason); return; }
    dirty.current = true; invalidateReview(); setProposal(next); setError(""); setNotice("");
  }
  function choose(kind: "users" | "groups", id: string) {
    try { assertContext(); } catch (reason) { fail(reason); return; }
    if (kind === "users") setPerson(id); else setGroup(id);
  }
  function useCurrent() {
    try { assertContext(); } catch (reason) { fail(reason); return; }
    if (!inspection) return;
    invalidateReview(); dirty.current = false; setProposal(restrictionProposalFromInspection(inspection)); setError(""); setNotice("");
  }
  function remove(kind: "users" | "groups", id: string) {
    if (!proposal) return;
    edit({ ...proposal, allowlist: { ...proposal.allowlist, [kind]: proposal.allowlist[kind].filter((value) => value !== id) } });
  }
  function add(kind: "users" | "groups", id: string) {
    if (!proposal || !inspection || !id || proposal.allowlist[kind].includes(id)) return;
    const catalog = kind === "users" ? inspection.eligible_recipients.users : inspection.eligible_recipients.groups;
    if (!catalog.some((entry) => entry.id === id)) return;
    edit({ ...proposal, allowlist: { ...proposal.allowlist, [kind]: [...proposal.allowlist[kind], id] } });
    if (kind === "users") setPerson(""); else setGroup("");
  }
  async function prepareReview() {
    if (!proposal || !inspection || busy || reopen) return;
    const request = ++sequence.current;
    invalidateReview(); const edited = generation.current;
    setPhase("reviewing"); setError(""); setNotice("");
    try {
      assertContext(); const submitted = normalizeRestrictionProposal(proposal);
      const response = await dataClient.previewPageRestrictions(spaceId, pageId, submitted, assertContext);
      assertContext();
      if (!mounted.current || request !== sequence.current || edited !== generation.current) return;
      if (Date.parse(response.expires_at) <= Date.now()) throw new Error("Restriction review expired. Inspect and review again.");
      setReview({ response, proposal: submitted, generation: edited }); setPhase("ready");
    } catch (reason) { if (mounted.current && request === sequence.current) fail(reason); }
  }
  async function confirm() {
    if (!review || !inspection?.can_apply || busy || reopen) return;
    const submitted = review;
    if (!restrictionReviewIsCurrent(submitted, generation.current)) {
      invalidateReview(); setInspection(null); setNotice("Restriction review expired. Inspect and review again before confirming."); return;
    }
    const request = ++sequence.current;
    // A dispatched token is never retried automatically, even after a lost response.
    setReview(null); generation.current++; setPhase("confirming"); setError(""); setNotice("");
    try {
      assertContext();
      await dataClient.applyPageRestrictions(spaceId, pageId, submitted.response, submitted.proposal.mode, assertContext);
      assertContext();
      if (!mounted.current || request !== sequence.current) return;
    } catch (reason) {
      if (mounted.current && request === sequence.current) fail(reason, true);
      return;
    }
    setInspection(null); dirty.current = false;
    setNotice("Restriction change confirmed. Refreshing current policy and access…");
    // Ordinary View may still be unavailable; it must never prevent this management load.
    onConfirmed();
    const refreshed = await load(false, true);
    if (!mounted.current) return;
    try { assertContext(); }
    catch (reason) { fail(reason); return; }
    setNotice(refreshed ? "Restriction change confirmed. Current access may change; inspect again before another change." : "Restriction change confirmed; current policy could not be refreshed. Inspect again before another change.");
  }
  const unavailable = proposal?.mode === "restricted" && proposal.allowlist.users.some((id) => !inspection?.eligible_recipients.users.some((entry) => entry.id === id)) ||
    proposal?.mode === "restricted" && proposal.allowlist.groups.some((id) => !inspection?.eligible_recipients.groups.some((entry) => entry.id === id));
  let valid = false;
  if (proposal) { try { normalizeRestrictionProposal(proposal); valid = true; } catch { /* Invalid stored selections require explicit correction. */ } }
  const count = proposal ? proposal.allowlist.users.length + proposal.allowlist.groups.length : 0;
  return (
    <section aria-labelledby={headingId} className="mt-5 space-y-3 border-t border-border pt-4">
      <h3 id={headingId} className="text-text text-sm font-semibold">Page restriction</h3>
      <p className="text-text-muted text-sm">Restrictions limit existing access. They do not grant access or share private drafts.</p>
      {phase !== "ready" && <p role="status" className="text-text-muted text-sm">{phase === "confirming" ? "Confirming restriction…" : phase === "reviewing" ? "Reviewing proposed restriction…" : "Inspecting current restriction…"}</p>}
      {error && <p role="alert" className="text-danger text-sm">{error}</p>}
      {notice && <p role="status" className="text-text-muted text-sm">{notice}</p>}
      {inspection && <p className="text-text-muted text-xs">Checked {new Date(inspection.checked_at).toLocaleString()}. This is a point-in-time check.</p>}
      {proposal && !reopen && <>
        <label htmlFor={modeId} className="block text-sm font-medium text-text">Local policy</label>
        <Select id={modeId} onOpenChange={(open) => menuChanged("mode", open)} value={proposal.mode} disabled={busy || !inspection} onChange={(event) => {
          const next = event.target.value;
          if (next === "inherited" || next === "restricted") edit({ mode: next, allowlist: next === "inherited" ? { users: [], groups: [] } : proposal.allowlist });
        }}><option value="inherited">Inherited access</option><option value="restricted">Restricted access</option></Select>
        {proposal.mode === "inherited" ? <p className="text-text-muted text-sm">This removes the local restriction. Ancestor restrictions and space blocks still apply.</p> : <>
          {(["users", "groups"] as const).map((kind) => <div key={kind} className="space-y-2">
            <p className="text-sm font-medium text-text">{kind === "users" ? "People" : "Groups"}</p>
            <ul className="max-h-40 space-y-2 overflow-y-auto">
              {proposal.allowlist[kind].map((id) => {
                const label = restrictionSelectionLabel(inspection, kind, id);
                return <li key={id} className="flex min-w-0 items-start justify-between gap-3 text-sm"><span className="min-w-0 break-words text-text">{label}</span><Button variant="ghost" size="sm" disabled={busy || !inspection} onClick={() => remove(kind, id)} aria-label={`Remove ${label}`}>Remove</Button></li>;
              })}
            </ul>
            {proposal.allowlist[kind].length === 0 && <p className="text-text-muted text-sm">No {kind === "users" ? "people" : "groups"} selected.</p>}
          </div>)}
          <div className="space-y-2">
            <label htmlFor={peopleId} className="block text-sm font-medium text-text">Add person</label>
            <div className="flex min-w-0 gap-2"><Select id={peopleId} onOpenChange={(open) => menuChanged("person", open)} value={person} disabled={busy || !inspection || count >= 100} onChange={(event) => choose("users", event.target.value)}><option value="">Choose a current participant</option>{inspection?.eligible_recipients.users.filter((entry) => !proposal.allowlist.users.includes(entry.id)).map((entry) => <option key={entry.id} value={entry.id}>{entry.display_name || "Unnamed person"}</option>)}</Select><Button variant="secondary" disabled={busy || !inspection || !person || count >= 100} onClick={() => add("users", person)}>Add</Button></div>
            <label htmlFor={groupsId} className="block text-sm font-medium text-text">Add group</label>
            <div className="flex min-w-0 gap-2"><Select id={groupsId} onOpenChange={(open) => menuChanged("group", open)} value={group} disabled={busy || !inspection || count >= 100} onChange={(event) => choose("groups", event.target.value)}><option value="">Choose a scoped group</option>{inspection?.eligible_recipients.groups.filter((entry) => !proposal.allowlist.groups.includes(entry.id)).map((entry) => <option key={entry.id} value={entry.id}>{entry.name || "Unnamed group"}</option>)}</Select><Button variant="secondary" disabled={busy || !inspection || !group || count >= 100} onClick={() => add("groups", group)}>Add</Button></div>
          </div>
          {unavailable && <p className="text-text-muted text-sm">Remove or replace unavailable selections explicitly before reviewing this restricted policy.</p>}
        </>}
        {inspection && !inspection.can_apply && <p className="text-text-muted text-sm">This connection can review restrictions but cannot confirm changes.</p>}
        {review && <div className="space-y-2 border-t border-border pt-3">
          <p className="text-sm text-text">{review.response.scope_notice}</p>
          <p className="text-text-muted text-sm">Reviewed policy: {review.proposal.mode === "inherited" ? "Inherited access" : "Restricted access"}. Only currently readable pages are listed below; this is not a complete audience.</p>
          <ul className="max-h-32 space-y-1 overflow-y-auto text-sm text-text-muted">{review.response.readable_pages.map((page) => <li key={page.document_id} className="break-words">Page {page.document_id}: {page.public_link_configured ? page.public_link_policy_after ? "Public-link policy allows viewing" : "Public-link policy prevents viewing" : "No public link configured"}</li>)}</ul>
          <p className="text-text-muted text-xs">Review expires {new Date(review.response.expires_at).toLocaleTimeString()}.</p>
          <Button disabled={busy || !inspection?.can_apply || Date.parse(review.response.expires_at) <= Date.now()} onClick={() => void confirm()}>{review.proposal.mode === "inherited" ? "Confirm inherited access" : "Confirm restriction"}</Button>
        </div>}
      </>}
      {!reopen && <div className="flex flex-wrap gap-2">
        <Button variant="secondary" disabled={busy} onClick={() => void load()}>Refresh restrictions</Button>
        {inspection && proposal && <><Button variant="secondary" disabled={busy} onClick={useCurrent}>Use current policy</Button><Button disabled={busy || !valid || unavailable} onClick={() => void prepareReview()}>Review change</Button></>}
      </div>}
    </section>
  );
}
