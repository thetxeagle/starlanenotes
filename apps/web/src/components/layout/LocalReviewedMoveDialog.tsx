import { useEffect, useMemo, useRef, useState } from "react";
import { nativeJsonValueEquals } from "@/lib/nativeLocalDatabase";
import type { LocalReviewedMoveSession as LocalMoveDialogSession } from "@/lib/dataClient";
import type { LocalMovePreview, LocalMoveMappingCandidate } from "@/lib/reviewedMoves";
import type { LocalReviewedMoveDecisions, LocalMoveMetadataChoice } from "@/lib/localReviewedMove";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { DialogShell, DialogHeader, DialogTitle, DialogDescription, DialogCloseAction, DialogBody, DialogFooter } from "@/components/ui/dialog-shell";

type CommitResult = Awaited<ReturnType<LocalMoveDialogSession["commit"]>>;
const copy = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const message = (error: unknown) => error instanceof Error ? error.message : "Move outcome is unconfirmed. Keep the original review.";
const emptyDecisions = (): LocalReviewedMoveDecisions => ({ properties: [], workflows: [], tags: [], relations: [], identifiers: [], maintenance: [], detach_hierarchy: [], preserve_previous_draft_placement: false });

/** A relation's old external target is deliberately resolved separately below.
 * This only exposes the definition choice; session.validate remains the final gate. */
export function localMoveMappingCandidateAvailable(preview: LocalMovePreview, family: "properties" | "workflows" | "tags", candidate: LocalMoveMappingCandidate): boolean {
  if (candidate.compatible) return true;
  if (family !== "properties" || preview.unavailable_participation || !candidate.reasons.length || candidate.reasons.some(reason => reason !== "relation_target_unavailable_in_destination")) return false;
  const source = preview.inventory.property_schemas.find(row => row.id === candidate.source_id);
  const target = preview.inventory.destination.property_schemas.find(row => row.id === candidate.destination_id);
  if (source?.property_type !== "relation" || target?.property_type !== "relation") return false;
  const finalIds = new Set([...preview.inventory.documents, ...preview.inventory.destination.documents].map(row => row.id));
  const outsideValues = preview.inventory.property_values.filter(row => row.property_id === source.id && !!row.value && !finalIds.has(row.value));
  return outsideValues.length > 0 && outsideValues.every(value => preview.inventory.external_relations.some(edge => edge.direction === "outgoing" && edge.value.id === value.id && edge.value.property_id === value.property_id && edge.value.value === value.value));
}

/** Consumed by the dialog: one operation/request, no re-enrollment after error. */
export function createLocalMoveSubmission(
  session: LocalMoveDialogSession,
  current: () => boolean,
  identity: () => { operation_id: string; moved_at: string } = () => ({ operation_id: crypto.randomUUID(), moved_at: new Date().toISOString() }),
) {
  let pending: Parameters<LocalMoveDialogSession["commit"]>[0] | null = null;
  let confirmed: CommitResult | null = null;
  let running = false;
  const verify = () => {
    if (!current()) throw new Error("The original move selection changed. Reopen the current workspace for inspection.");
    session.verifyCurrent();
  };
  return {
    locked: () => pending !== null,
    confirmed: () => confirmed !== null,
    cancel: () => { if (running || pending) throw new Error("The move outcome needs review. Cancelling cannot undo a committed move."); },
    async submit(decisions: LocalReviewedMoveDecisions): Promise<CommitResult | undefined> {
      if (running) return undefined;
      running = true;
      try {
        verify();
        if (confirmed) return confirmed;
        if (!pending) {
          session.validate(decisions);
          pending = { ...identity(), decisions: copy(decisions) };
        }
        // Pass a detached copy so a consumer cannot alter the retained retry.
        const result = await session.commit(copy(pending));
        verify();
        if (result?.canonical_committed !== true || result.record?.id !== pending.operation_id
          || result.record.schema_version !== (session.provider === "google-drive" ? 2 : 1) || result.record.provider !== (session.provider ?? "none") || (session.provider === "google-drive" && result.provider_status !== "pending_provider") || result.record.actor_id !== session.preview.current_user_id
          || result.record.moved_at !== pending.moved_at
          || !nativeJsonValueEquals(result.record.intent, session.preview.intent)
          || !nativeJsonValueEquals(result.record.decisions, pending.decisions))
          throw new Error("Move outcome is unconfirmed. Retain the original operation for inspection.");
        confirmed = result;
        return confirmed;
      } finally { running = false; }
    },
  };
}

export function localMoveReadiness(session: LocalMoveDialogSession, decisions: LocalReviewedMoveDecisions, confirmed: boolean): string | null {
  if (!confirmed) return "Confirm that you reviewed the complete subtree and every metadata choice.";
  try { session.verifyCurrent(); session.validate(copy(decisions)); return null; }
  catch (error) { return message(error); }
}

/** Keyed ownership: closing one menu cannot release another menu's Escape. */
export function updateLocalMoveMenuOwnership(current: ReadonlySet<string>, key: string, open: boolean): Set<string> {
  const next = new Set(current);
  if (open) next.add(key); else next.delete(key);
  return next;
}

interface Props {
  session: LocalMoveDialogSession;
  title: string;
  destinationName: string;
  onClose(): void;
  onCommitted(result: CommitResult): Promise<void> | void;
}

export function LocalReviewedMoveDialog({ session, title, destinationName, onClose, onCommitted }: Props) {
  const original = useRef(session);
  const originalLabels = useRef({ title, destinationName });
  const live = useRef(session);
  live.current = session;
  const mounted = useRef(true);
  const [submission] = useState(() => createLocalMoveSubmission(original.current, () => mounted.current && live.current === original.current));
  const preview = original.current.preview;
  const [decisions, setDecisions] = useState(emptyDecisions);
  const [whole, setWhole] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [openMenus, setOpenMenus] = useState<ReadonlySet<string>>(() => new Set());
  const menuOwnership = (key: string) => (open: boolean) => {
    setOpenMenus(current => updateLocalMoveMenuOwnership(current, key, open));
  };
  const [confirmation, setConfirmation] = useState<{ title: string; apply(): void; remove: boolean } | null>(null);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const locked = busy || submission.locked();
  const choiceReadiness = useMemo(() => {
    if (session !== original.current) return "The original move selection changed. Close and inspect the current workspace.";
    return localMoveReadiness(original.current, decisions, true);
  }, [session, decisions]);
  const readiness = whole ? choiceReadiness : "Confirm that you reviewed the complete subtree and every metadata choice.";
  const resolvedReasons = new Set(["external_relations_require_resolution", "external_hierarchy_requires_resolution", "property_identity_requires_resolution"]);
  const requirements = preview.blockers.filter(reason => choiceReadiness !== null || !resolvedReasons.has(reason));
  const close = () => {
    if (busy) return;
    if (submission.confirmed()) { onClose(); return; }
    try { submission.cancel(); onClose(); } catch (cause) { setError(message(cause)); }
  };
  const submit = async () => {
    setBusy(true); setError("");
    try {
      const result = await submission.submit(decisions);
      if (!result) return;
      await onCommitted(result);
      original.current.verifyCurrent();
      if (mounted.current && live.current === original.current) onClose();
    } catch (cause) { if (mounted.current) setError(submission.confirmed() ? `${original.current.provider === "google-drive" ? "Move is committed locally; Google provider publication remains pending." : "Move is committed."} Refresh was not confirmed: ${message(cause)}` : message(cause)); }
    finally { if (mounted.current) setBusy(false); }
  };
  const change = (apply: () => void, remove: boolean, name: string) => {
    if (locked) return;
    if (remove) setConfirmation({ title: `Remove ${name}?`, remove: true, apply });
    else apply();
  };
  const metadata = (family: "properties" | "workflows" | "tags", id: string, choice: LocalMoveMetadataChoice) => {
    setDecisions(old => ({ ...old, [family]: [...old[family].filter(row => row.source_id !== id), { source_id: id, choice }] }));
  };
  const definitions = [
    { family: "properties" as const, label: "Property", source: preview.inventory.property_schemas, destination: preview.inventory.destination.property_schemas },
    { family: "workflows" as const, label: "Workflow", source: preview.inventory.workflow_templates, destination: preview.inventory.destination.workflow_templates },
    { family: "tags" as const, label: "Tag", source: preview.inventory.tags, destination: preview.inventory.destination.tags },
  ];
  const pageLabel = (id: string) => preview.inventory.documents.find(page => page.id === id)?.title || "Moved page";
  return <>
    <DialogShell labelledBy="local-move-title" describedBy="local-move-description" maxWidth="xl" onClose={close} closeOnEscape={openMenus.size === 0}>
      <DialogHeader>
        <div className="min-w-0">
          <DialogTitle id="local-move-title">{original.current.provider === "google-drive" ? "Review Google workspace move" : "Review local move"}</DialogTitle>
          <DialogDescription id="local-move-description" className="break-words">{originalLabels.current.title} → {originalLabels.current.destinationName}. {original.current.provider === "google-drive" ? "Move the complete subtree locally; Google publication remains pending." : "Move the complete subtree in this standalone vault."}</DialogDescription>
        </div>
        <DialogCloseAction label="Close local move review" onClick={close} disabled={busy} />
      </DialogHeader>
      <DialogBody className="space-y-4">
        <p className="text-text-muted text-sm">Reviewed preview: {preview.inventory.documents.length} pages · {preview.inventory.folders.length} folders · {preview.inventory.drafts.length} drafts · {preview.inventory.assets.length} assets. IDs, bodies and file references stay intact.</p>
        {preview.unavailable_participation && <p role="alert" className="text-danger text-sm">Some participating content is unavailable. A new complete review is required.</p>}
        {requirements.length > 0 && <section aria-label="Move review requirements"><ul className="list-disc space-y-1 pl-5 text-sm text-text-muted">{requirements.map(reason => <li key={reason}>{reason.replace(/_/g, " ")}</li>)}</ul></section>}
        {definitions.map(group => group.source.map(source => {
          const chosen = decisions[group.family].find(row => row.source_id === source.id)?.choice;
          const candidates = group.destination.filter(target => group.family === "tags" || preview.mapping_candidates[group.family].some(candidate => candidate.source_id === source.id && candidate.destination_id === target.id && localMoveMappingCandidateAvailable(preview, group.family, candidate)));
          return <fieldset key={`${group.family}:${source.id}`} className="border-border space-y-2 border p-3">
            <legend className="max-w-full break-words px-2 text-sm">{group.label} · {source.name}</legend>
            <Select onOpenChange={menuOwnership(`${group.family}:${source.id}:action`)} aria-label={`${group.label} action for ${source.name}`} value={chosen?.kind ?? ""} disabled={locked} onChange={event => {
              const kind = event.target.value;
              if (!["map", "create", "preserve", "remove"].includes(kind)) return;
              const choice: LocalMoveMetadataChoice = kind === "map" ? { kind, destination_id: "" } : kind === "create" ? { kind, destination_id: crypto.randomUUID() } : { kind: kind as "preserve" | "remove" };
              change(() => metadata(group.family, source.id, choice), kind === "remove", source.name);
            }}>
              <option value="">Choose a disposition</option>
              {candidates.length > 0 && <option value="map">Map to compatible metadata</option>}
              <option value="create">Create a destination copy</option>
              <option value="preserve">Preserve as inactive evidence</option>
              <option value="remove">Remove active assignment</option>
            </Select>
            {chosen?.kind === "map" && <Select onOpenChange={menuOwnership(`${group.family}:${source.id}:destination`)} aria-label={`Destination ${group.label.toLowerCase()} for ${source.name}`} disabled={locked} value={chosen.destination_id} onChange={event => metadata(group.family, source.id, { kind: "map", destination_id: event.target.value })}>
              <option value="">Choose compatible destination</option>{candidates.map(target => <option key={target.id} value={target.id}>{target.name}</option>)}
            </Select>}
            {chosen?.kind === "create" && <p className="text-text-muted text-xs">Copies this definition and its options or stages with a new identity. Final validation must accept every value and assignment.</p>}
          </fieldset>;
        }))}
        {preview.inventory.external_relations.map(edge => {
          const choice = decisions.relations.find(row => row.value_id === edge.value.id)?.choice;
          let targets: readonly { id: string; title: string }[] = [];
          try { targets = original.current.relationTargets(edge.value.id); } catch { /* No unproven target fallback. */ }
          const label = `${edge.direction === "incoming" ? "Incoming" : "Outgoing"} relationship ${edge.value.id}`;
          const set = (value: LocalReviewedMoveDecisions["relations"][number]["choice"]) => setDecisions(old => ({ ...old, relations: [...old.relations.filter(row => row.value_id !== edge.value.id), { value_id: edge.value.id, choice: value }] }));
          return <fieldset key={edge.value.id} className="border-border space-y-2 border p-3"><legend className="max-w-full break-words px-2 text-sm">{label}</legend>
            <Select onOpenChange={menuOwnership(`relation:${edge.value.id}:action`)} aria-label={`${label} action`} disabled={locked} value={choice?.kind ?? ""} onChange={event => {
              const kind = event.target.value;
              if (kind !== "map" && kind !== "preserve" && kind !== "remove") return;
              change(() => set(kind === "map" ? { kind, document_id: "" } : { kind }), kind === "remove", "relationship");
            }}><option value="">Choose a disposition</option>{targets.length > 0 && <option value="map">Map to a readable page</option>}<option value="preserve">Preserve inactive evidence</option><option value="remove">Remove relationship</option></Select>
            {choice?.kind === "map" && <Select onOpenChange={menuOwnership(`relation:${edge.value.id}:target`)} aria-label={`${label} target`} value={choice.document_id} disabled={locked} onChange={event => set({ kind: "map", document_id: event.target.value })}><option value="">Choose a final-space page</option>{targets.map(target => <option key={target.id} value={target.id}>{target.title}</option>)}</Select>}
            {!targets.length && <p className="text-text-muted text-xs">No proven readable mapping target is available. Preserve or remove this relationship.</p>}
          </fieldset>;
        })}
        {preview.inventory.identifiers.map(page => {
          const choice = decisions.identifiers.find(row => row.document_id === page.document_id)?.choice;
          const set = (value: LocalReviewedMoveDecisions["identifiers"][number]["choice"]) => setDecisions(old => ({ ...old, identifiers: [...old.identifiers.filter(row => row.document_id !== page.document_id), { document_id: page.document_id, choice: value }] }));
          return <fieldset key={`identifier:${page.document_id}`} className="border-border space-y-2 border p-3"><legend className="max-w-full break-words px-2 text-sm">Identifier · {pageLabel(page.document_id)}</legend>
            <p className="text-text-muted text-sm">{page.human_identifier ?? "Unassigned"}{page.resolution_required ? ". Conflicts in the destination; rename or clear explicitly." : ""}</p>
            <Select onOpenChange={menuOwnership(`identifier:${page.document_id}:action`)} aria-label={`Identifier action for ${pageLabel(page.document_id)}`} value={choice?.kind ?? (page.resolution_required ? "" : "preserve")} disabled={locked} onChange={event => { const kind = event.target.value; if (kind === "rename") set({ kind, value: "" }); else if (kind === "preserve" || kind === "clear") set({ kind }); }}><option value="">Choose identifier action</option><option value="preserve">Preserve identifier</option><option value="rename">Rename identifier</option><option value="clear">Clear identifier</option></Select>
            {choice?.kind === "rename" && <Input aria-label={`New identifier for ${pageLabel(page.document_id)}`} value={choice.value} disabled={locked} onChange={event => set({ kind: "rename", value: event.target.value })} />}
          </fieldset>;
        })}
        {preview.inventory.maintenance.map(page => {
          const choice = decisions.maintenance.find(row => row.document_id === page.document_id)?.choice;
          const set = (value: LocalReviewedMoveDecisions["maintenance"][number]["choice"]) => setDecisions(old => ({ ...old, maintenance: [...old.maintenance.filter(row => row.document_id !== page.document_id), { document_id: page.document_id, choice: value }] }));
          const targets = preview.inventory.destination_maintenance_targets;
          const users = targets?.users ?? [];
          const groups = targets?.groups.filter(group => group.managed_space_id === null || group.managed_space_id === preview.intent.destination.space_id) ?? [];
          const document = preview.inventory.documents.find(row => row.id === page.document_id);
          const editable = document && !document.locked && !document.archived_at && !document.deleted_at;
          return <fieldset key={`maintenance:${page.document_id}`} className="border-border space-y-2 border p-3"><legend className="max-w-full break-words px-2 text-sm">Maintenance · {pageLabel(page.document_id)}</legend>
            <p className="text-text-muted text-sm">{page.maintenance_user_id ? "Assigned person" : page.maintenance_group_id ? "Assigned team" : "Unassigned"}. Responsibility does not grant access.{page.resolution_required ? " Map or clear the unresolved assignment." : ""}</p>
            <Select onOpenChange={menuOwnership(`maintenance:${page.document_id}:action`)} aria-label={`Maintenance action for ${pageLabel(page.document_id)}`} disabled={locked || !editable} value={choice?.kind ?? (page.resolution_required ? "" : "preserve")} onChange={event => { const kind = event.target.value; if (kind === "user") set({ kind, user_id: "" }); else if (kind === "group") set({ kind, group_id: "" }); else if (kind === "preserve" || kind === "clear") set({ kind }); }}><option value="">Choose maintenance action</option><option value="preserve">Preserve assignment</option><option value="clear">Clear assignment</option>{users.length > 0 && <option value="user">Assign person</option>}{groups.length > 0 && <option value="group">Assign team</option>}</Select>
            {choice?.kind === "user" && <Select onOpenChange={menuOwnership(`maintenance:${page.document_id}:person`)} aria-label={`Maintenance person for ${pageLabel(page.document_id)}`} disabled={locked} value={choice.user_id} onChange={event => set({ kind: "user", user_id: event.target.value })}><option value="">Choose catalog person</option>{users.map(user => <option key={user.id} value={user.id}>{user.name}</option>)}</Select>}
            {choice?.kind === "group" && <Select onOpenChange={menuOwnership(`maintenance:${page.document_id}:team`)} aria-label={`Maintenance team for ${pageLabel(page.document_id)}`} disabled={locked} value={choice.group_id} onChange={event => set({ kind: "group", group_id: event.target.value })}><option value="">Choose catalog team</option>{groups.map(group => <option key={group.id} value={group.id}>{group.name}</option>)}</Select>}
            {!editable && <p className="text-text-muted text-xs">This page's assignment cannot be edited.</p>}
          </fieldset>;
        })}
        {preview.inventory.external_hierarchy.length > 0 && <section aria-label="Previous hierarchy" className="space-y-2 text-sm"><p className="text-warning">The moved root must detach from its previous parent. Every descendant remains in this move.</p>{preview.inventory.external_hierarchy.map(edge => <label key={`${edge.kind}:${edge.id}`} className="flex min-h-11 items-start gap-2 text-text"><Input type="checkbox" className="mt-1 h-4 min-h-0 w-4 shrink-0 p-0 accent-primary" disabled={locked} checked={decisions.detach_hierarchy.some(row => row.kind === edge.kind && row.id === edge.id)} onChange={event => setDecisions(old => ({ ...old, detach_hierarchy: event.target.checked ? [...old.detach_hierarchy, { kind: edge.kind, id: edge.id, choice: "detach" }] : old.detach_hierarchy.filter(row => row.kind !== edge.kind || row.id !== edge.id) }))} />I reviewed the previous {edge.kind} parent detachment.</label>)}</section>}
        {preview.warnings.includes("draft_previous_placement_requires_review") && <label className="flex min-h-11 items-start gap-2 text-sm text-text"><Input type="checkbox" className="mt-1 h-4 min-h-0 w-4 shrink-0 p-0 accent-primary" disabled={locked} checked={decisions.preserve_previous_draft_placement} onChange={event => setDecisions(old => ({ ...old, preserve_previous_draft_placement: event.target.checked }))} />Keep previous draft placement as historical evidence. Drafts stay with their moved pages.</label>}
        <label className="flex min-h-11 items-start gap-2 text-sm text-text"><Input type="checkbox" className="mt-1 h-4 min-h-0 w-4 shrink-0 p-0 accent-primary" disabled={locked} checked={whole} onChange={event => setWhole(event.target.checked)} />I reviewed the complete subtree, metadata choices and destination placement.</label>
        {original.current.provider === "google-drive" && <p role="status" className="text-warning text-sm">This move commits to this browser Google workspace and retains one complete provider request. Remote publication and received application are unavailable; manual sync holds the request for review.</p>}
        <p className="text-text-muted text-xs">Preserve retains inactive raw evidence; Remove detaches active assignments and retains removal evidence. This review does not restore metadata or change file bytes.</p>
        {readiness && !submission.locked() && <p role="status" className="text-text-muted text-sm">{readiness}</p>}
        {error && <p role="alert" className="text-danger text-sm">{error}</p>}
        {submission.locked() && !submission.confirmed() && <p className="text-warning text-sm">Keep this original review open. Retry the same operation; do not start another move. Restart-safe pending request recovery is not supplied by this dialog.</p>}
      </DialogBody>
      <DialogFooter><Button type="button" variant="secondary" disabled={busy} onClick={close}>{submission.confirmed() ? "Close" : "Cancel"}</Button><Button type="button" disabled={busy || (!submission.locked() && readiness !== null)} onClick={() => submission.locked() ? void submit() : setConfirmation({ title: "Move the complete subtree?", remove: false, apply: () => void submit() })}>{busy ? "Moving…" : submission.confirmed() ? "Retry refresh" : submission.locked() ? "Retry same move" : "Move"}</Button></DialogFooter>
    </DialogShell>
    {confirmation && <ConfirmDialog title={confirmation.title} message={confirmation.remove ? "The active assignment will be removed. Its raw historical evidence remains retained; restoration is unavailable." : (original.current.provider === "google-drive" ? "Commit these exact choices locally to this browser Google workspace. Provider publication remains pending and manual sync is held while the complete move request is retained. Files and page identities remain intact." : "Apply these exact choices to every participating page, folder and draft in the destination. Files and page identities remain intact.")} labelledBy="local-move-confirmation" destructive={confirmation.remove} confirmLabel={confirmation.remove ? "Remove assignment" : "Move subtree"} onCancel={() => setConfirmation(null)} onConfirm={() => { const apply = confirmation.apply; setConfirmation(null); apply(); }} />}
  </>;
}
