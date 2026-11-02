import { useCallback, useEffect, useRef, useState } from "react";
import type {
  MoveDefinitionReview,
  ServerMovePreview,
  ServerMoveIntent,
  ServerMoveCommit,
  ServerMetadataChoice,
  ServerIncidentChoice,
  ServerMoveCommitResult,
  ServerMoveIdentifierChoice,
  ServerMoveMaintenanceChoice,
  ServerMoveViewerGrantChoice,
} from "@starlane-notes/shared-types";
import { normalizeHumanIdentifier, humanIdentifierKey } from "@/lib/humanIdentifier";
import { resolveServerMoveViewerGrants, reviewedServerMoves, prepareReviewedMove, dispatchReviewedMove, type ReviewedMoveContext, type PendingReviewedMove } from "@/lib/reviewedServerMoves";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  DialogShell,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogCloseAction,
  DialogBody,
  DialogFooter,
} from "@/components/ui/dialog-shell";
const key = (m: MoveDefinitionReview) => `${m.kind}:${String(m.source_definition.id)}`;
const message = (e: unknown) =>
  e instanceof Error ? e.message : "Move review unavailable. Retry without changing the source.";
export function createMoveDefinition(
  m: MoveDefinitionReview,
  name: string,
): Record<string, unknown> {
  const d = m.source_definition;
  const parse = (v: unknown) => (typeof v === "string" ? JSON.parse(v) : v);
  if (m.kind === "tag") return { id: crypto.randomUUID(), name, color: d.color };
  if (m.kind === "property")
    return {
      id: crypto.randomUUID(),
      name,
      property_type: d.property_type,
      options: d.options == null ? null : parse(d.options),
      sort_order: d.sort_order,
    };
  return {
    id: crypto.randomUUID(),
    name,
    description: d.description ?? null,
    stages: parse(d.stages),
    approvers: parse(d.approvers),
  };
}
export function buildMoveCommit(
  preview: ServerMovePreview,
  choices: Record<string, ServerMetadataChoice>,
  relations: Record<string, ServerIncidentChoice>,
  links: "preserve" | "disable",
  audience: boolean,
  drafts: boolean,
  identifiers: Record<string, ServerMoveIdentifierChoice> = {},
  maintenance: Record<string, ServerMoveMaintenanceChoice> = {},
  viewerGrants: Record<string, ServerMoveViewerGrantChoice> = {},
): ServerMoveCommit {
  const viewerGrantDecisions = resolveServerMoveViewerGrants(preview, viewerGrants);
  const maintenanceDecisions = Object.entries(maintenance).map(([document_id, choice]) => {
    const page = preview.inventory.pages?.find((p) => p.id === document_id);
    if (!page) throw new Error("Maintenance decision refers to an unavailable page.");
    if (choice.kind !== "preserve" && !page.maintenance_edit_allowed)
      throw new Error("You cannot change this page's maintenance assignment.");
    if (
      choice.kind === "user" &&
      !preview.inventory.maintenance_targets?.users.some((u) => u.id === choice.user_id)
    )
      throw new Error("Select a current destination person.");
    if (
      choice.kind === "group" &&
      !preview.inventory.maintenance_targets?.groups.some((g) => g.id === choice.group_id)
    )
      throw new Error("Select a current destination team.");
    return { document_id, choice };
  });
  for (const page of preview.inventory.pages ?? [])
    if (
      page.maintenance_resolution_required &&
      (!maintenance[page.id] || maintenance[page.id].kind === "preserve")
    )
      throw new Error("Map or clear maintenance assignments unavailable in the destination.");
  const identifierKeys = new Set<string>();
  const identifierDecisions = Object.entries(identifiers).map(([document_id, choice]) => {
    const page = preview.inventory.pages?.find((p) => p.id === document_id);
    if (!page) throw new Error("Identifier decision refers to an unavailable page.");
    if (choice.kind !== "preserve" && !page.identifier_edit_allowed)
      throw new Error("You cannot change this page identifier.");
    if (choice.kind === "rename") {
      const value = normalizeHumanIdentifier(choice.value);
      if (!value) throw new Error("Use Clear to remove an identifier.");
      return { document_id, choice: { kind: "rename" as const, value } };
    }
    return { document_id, choice };
  });
  for (const page of preview.inventory.pages ?? []) {
    const choice = identifiers[page.id];
    if (page.identifier_resolution_required && (!choice || choice.kind === "preserve"))
      throw new Error("Rename or clear every conflicting page identifier.");
    const value =
      choice?.kind === "clear"
        ? null
        : choice?.kind === "rename"
          ? choice.value
          : page.human_identifier;
    const key = humanIdentifierKey(value ?? null);
    if (key && identifierKeys.has(key)) throw new Error("Moved page identifiers must be unique.");
    if (key) identifierKeys.add(key);
  }
  const metadata = preview.inventory.metadata ?? [];
  const crossing = (preview.inventory.relations ?? []).filter((r) => r.crosses_subtree);
  if (
    crossing.some(
      (r) =>
        relations[r.assignment_id]?.kind === "map" &&
        !(relations[r.assignment_id] as { target_id: string }).target_id,
    )
  )
    throw new Error("Choose a destination for every mapped relationship.");
  if (metadata.some((m) => !choices[key(m)]) || crossing.some((r) => !relations[r.assignment_id]))
    throw new Error("Choose how to handle every metadata item and crossing relationship.");
  const decisions = metadata.map((m) => {
    let choice = choices[key(m)];
    if (
      m.kind === "property" &&
      m.source_definition.property_type === "relation" &&
      (choice.kind === "map" || choice.kind === "create")
    ) {
      const values = { ...choice.values };
      for (const edge of crossing.filter(
        (e) =>
          e.property_id === m.source_definition.id &&
          preview.inventory.pages?.some((p) => p.id === e.from_document_id),
      )) {
        const decision = relations[edge.assignment_id];
        if (decision.kind !== "map")
          throw new Error(
            "A mapped relation property needs a mapped destination target for every outgoing relationship.",
          );
        values[edge.assignment_id] = decision.target_id;
      }
      choice = { ...choice, values };
    }
    return {
      family: m.kind,
      source_id: String(m.source_definition.id),
      choice,
    };
  });
  return {
    fingerprint: preview.fingerprint,
    ...(viewerGrantDecisions.length ? { viewer_grant_decisions: viewerGrantDecisions } : {}),
    ...(maintenanceDecisions.length ? { maintenance_decisions: maintenanceDecisions } : {}),
    ...(identifierDecisions.length ? { identifier_decisions: identifierDecisions } : {}),
    decisions,
    incident_relations: crossing.map((r) => ({
      assignment_id: r.assignment_id,
      choice: relations[r.assignment_id],
    })),
    public_links: links,
    confirm_audience_change: audience,
    confirm_draft_space_change: drafts,
  };
}
/** Only the explicit relation-resolution marker can be discharged by complete choices. */
export function blockingMoveReviewReasons(preview: ServerMovePreview, complete: boolean): string[] {
  return preview.blockers.filter(blocker => blocker !== "external_relations_require_resolution" || !complete);
}
export function hasBlockingMoveReview(preview: ServerMovePreview, complete: boolean): boolean {
  return blockingMoveReviewReasons(preview, complete).length > 0;
}
export interface ReviewedMoveDialogProps {
  sourceSpaceId: string;
  context: ReviewedMoveContext;
  intent: ServerMoveIntent;
  title: string;
  destinationName: string;
  onClose: () => void;
  onCommitted: (result: ServerMoveCommitResult) => Promise<void>;
}
export function ReviewedMoveDialog({
  sourceSpaceId,
  context,
  intent,
  title,
  destinationName,
  onClose,
  onCommitted,
}: ReviewedMoveDialogProps) {
  const [confirming, setConfirming] = useState(false);
  const [committed, setCommitted] = useState<ServerMoveCommitResult | null>(null);
  const [preview, setPreview] = useState<ServerMovePreview | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [choices, setChoices] = useState<Record<string, ServerMetadataChoice>>({});
  const [relations, setRelations] = useState<Record<string, ServerIncidentChoice>>({});
  const [identifiers, setIdentifiers] = useState<Record<string, ServerMoveIdentifierChoice>>({});
  const [maintenance, setMaintenance] = useState<Record<string, ServerMoveMaintenanceChoice>>({});
  const [viewerGrants, setViewerGrants] = useState<Record<string, ServerMoveViewerGrantChoice>>({});
  const [openGrantMenus, setOpenGrantMenus] = useState<Set<string>>(() => new Set());
  const [links, setLinks] = useState<"preserve" | "disable">("preserve");
  const [audience, setAudience] = useState(false);
  const [drafts, setDrafts] = useState(false);
  const [clock, setClock] = useState(Date.now());
  const owned = useRef<string | null>(null);
  const commitRunning = useRef(false);
  const mounted = useRef(true);
  const requestId = useRef(0);
  const pending = useRef<PendingReviewedMove | null>(null);
  const originalTarget = useRef({ sourceSpaceId, intent, context });
  const currentTarget = useRef({ sourceSpaceId, intent, context });
  currentTarget.current = { sourceSpaceId, intent, context };
  const verifyOriginal = () => {
    context.verify();
    const original = originalTarget.current; const current = currentTarget.current;
    if (!mounted.current || original.sourceSpaceId !== current.sourceSpaceId || original.intent !== current.intent || original.context !== current.context) throw new Error("Original move selection changed. Retain its request for review.");
  };
  const load = useCallback(async () => {
    if (pending.current) {
      setError(
        "The previous move outcome is unconfirmed. Retry that same request before creating a new review.",
      );
      return;
    }
    const n = ++requestId.current;
    setBusy(true);
    setError("");
    try {
      context.verify();
      if (originalTarget.current.context !== context || originalTarget.current.intent !== intent || originalTarget.current.sourceSpaceId !== sourceSpaceId) throw new Error("Original move selection changed. Reopen its review.");
      if (owned.current) {
        await reviewedServerMoves.cancel(owned.current, context);
        owned.current = null;
      }
      const p = await reviewedServerMoves.preview(sourceSpaceId, intent, context);
      if (n !== requestId.current) {
        await reviewedServerMoves.cancel(p.preview_id, context);
        return;
      }
      context.verify();
      owned.current = p.preview_id;
      setPreview(p);
      setChoices({});
      setRelations({});
      setIdentifiers({});
      setMaintenance({});
      setViewerGrants({});
      setOpenGrantMenus(new Set());
      setAudience(false);
      setDrafts(false);
    } catch (e) {
      if (n === requestId.current) setError(message(e));
    } finally {
      if (n === requestId.current) setBusy(false);
    }
  }, [sourceSpaceId, intent, context]);
  useEffect(() => {
    mounted.current = true;
    const sequence = requestId;
    void load();
    const timer = window.setInterval(() => setClock(Date.now()), 1000);
    return () => {
      mounted.current = false;
      sequence.current++;
      window.clearInterval(timer);
    };
  }, [load]);
  const close = async () => {
    if (busy) return;
    if (committed) {
      try { verifyOriginal(); onClose(); } catch (e) { setError(message(e)); }
      return;
    }
    if (pending.current) {
      setError(
        "The move outcome is unconfirmed. Retry the same request; cancelling cannot undo a committed move.",
      );
      return;
    }
    setBusy(true);
    try {
      verifyOriginal();
      if (owned.current) await reviewedServerMoves.cancel(owned.current, context);
      owned.current = null;
      verifyOriginal(); onClose();
    } catch (e) {
      setError(`Cancellation was not confirmed. ${message(e)}`);
      setBusy(false);
    }
  };
  const submit = async () => {
    if (!preview || commitRunning.current) return;
    commitRunning.current = true;
    setBusy(true);
    setError("");
    try {
      if (committed) {
        verifyOriginal(); await onCommitted(committed);
        verifyOriginal(); onClose();
        return;
      }
      verifyOriginal();
      if (!pending.current)
        pending.current = prepareReviewedMove(context, preview.preview_id, buildMoveCommit(
          preview, choices, relations, links, audience, drafts, identifiers, maintenance, viewerGrants,
        ));
      const result = await dispatchReviewedMove(pending.current);
      verifyOriginal();
      pending.current = null;
      owned.current = null;
      setCommitted(result);
      await onCommitted(result);
      verifyOriginal(); onClose();
    } catch (e) {
      // Any dispatched rejection may follow an earlier committed attempt.
      // Keep the exact original preview/request; never infer absence from4xx.
      if (mounted.current) setError(message(e));
    } finally {
      commitRunning.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  const choose = (m: MoveDefinitionReview, kind: string) => {
    const id = key(m);
    try {
      const choice: ServerMetadataChoice | undefined =
        kind === "preserve"
          ? { kind: "preserve" }
          : kind === "remove"
            ? { kind: "remove", confirm: true }
            : kind === "create"
              ? {
                  kind: "create",
                  definition: createMoveDefinition(m, String(m.source_definition.name)),
                  values: {},
                  stages: {},
                }
              : kind.startsWith("map:")
                ? {
                    kind: "map",
                    destination_id: kind.slice(4),
                    values: {},
                    stages: {},
                  }
                : undefined;
      setChoices((v) => {
        const next = { ...v };
        if (choice) next[id] = choice;
        else delete next[id];
        return next;
      });
    } catch (e) {
      setError(message(e));
    }
  };
  const expired = !!preview && Date.parse(preview.expires_at) <= clock;
  let complete = false;
  if (preview) {
    try {
      buildMoveCommit(
        preview,
        choices,
        relations,
        links,
        audience,
        drafts,
        identifiers,
        maintenance,
        viewerGrants,
      );
      complete = true;
    } catch {}
  }
  if (confirming)
    return (
      <ConfirmDialog
        labelledBy="confirm-reviewed-move"
        title="Move reviewed subtree?"
        message="The move changes the space audience and selected active metadata. Page viewer grants follow your explicit preserve or remove choices. Historical metadata remains read-only evidence; draft ownership stays separate."
        destructive={
          Object.values(choices).some((c) => c.kind === "remove") ||
          Object.values(relations).some((c) => c.kind === "remove") ||
          Object.values(viewerGrants).some((c) => c.kind === "remove")
        }
        confirmLabel="Move"
        onCancel={() => setConfirming(false)}
        onConfirm={() => {
          setConfirming(false);
          void submit();
        }}
      />
    );
  return (
    <DialogShell
      labelledBy="reviewed-move-title"
      describedBy="reviewed-move-description"
      maxWidth="xl"
      closeOnEscape={openGrantMenus.size === 0}
      onClose={() => void close()}
    >
      <DialogHeader>
        <div>
          <DialogTitle id="reviewed-move-title">Review cross-space move</DialogTitle>
          <DialogDescription id="reviewed-move-description">
            {title} → {destinationName}. Review keeps the source unchanged.
          </DialogDescription>
        </div>
        <DialogCloseAction onClick={() => void close()} disabled={busy} />
      </DialogHeader>
      <DialogBody className="space-y-4 overflow-y-auto">
        <p role="status" className="text-text-muted text-sm">
          {committed
            ? "Move committed. Refresh the workspace to see its current location."
            : busy
              ? "Loading current move review…"
              : preview
                ? preview.inventory.pages
                  ? `${preview.inventory.pages.length} pages and ${preview.inventory.folders?.length ?? 0} folders in this move.`
                  : "Subtree inventory unavailable."
                : "No current review available."}
        </p>
        {error && (
          <p role="alert" className="text-danger text-sm">
            {error}
          </p>
        )}
        {preview && (
          <>
            <p id="move-readiness" className="text-text-muted text-sm">
              {preview.commit_unavailable_reason}
            </p>
            {(expired || preview.stale) && (
              <p role="alert" className="text-warning text-sm">
                This review has expired or changed. Review again before moving.
              </p>
            )}
            {blockingMoveReviewReasons(preview, complete).length > 0 && (
              <div className="text-warning text-sm">
                <p>Move blocked:</p>
                <ul className="list-inside list-disc">
                  {blockingMoveReviewReasons(preview, complete).map((b) => (
                    <li key={b}>{b.replace(/_/g, " ")}</li>
                  ))}
                </ul>
              </div>
            )}
            {preview.inventory.pages && (
              <p className="text-text-muted text-sm">
                Assets: {preview.inventory.attachments ?? 0} attachments;{" "}
                {preview.inventory.drawings ?? 0} drawings. History:{" "}
                {preview.inventory.history_versions ?? 0} revisions;{" "}
                {preview.inventory.workflow_history_events ?? 0} workflow events. Draft bodies are
                not included in this review.
              </p>
            )}
            {(preview.inventory.viewer_grants ?? []).map(grant => {
              const page = preview.inventory.pages?.find(p => p.id === grant.document_id);
              const label = grant.recipient.available ? grant.recipient.display_name ?? "Unnamed recipient" : "Unavailable recipient";
              return <fieldset key={`viewer-grant-${grant.grant_id}`} className="border-border space-y-2 border p-3">
                <legend className="max-w-full px-2 break-all">Page viewer · {page?.title ?? "Page"}</legend>
                <p className="text-text-muted text-sm">{label}. Preserve access in the destination or explicitly remove this grant. Draft sharing and inherited access stay separate.</p>
                {!grant.preserve_allowed && <p className="text-text-muted text-xs">This grant cannot be preserved in the destination under your current authority and access rules.</p>}
                {!grant.remove_allowed && <p className="text-text-muted text-xs">You do not have current authority to remove this grant.</p>}
                <Select aria-label={`Page viewer action for ${page?.title ?? "Page"} · ${label}`}
                  value={viewerGrants[grant.grant_id]?.kind ?? ""} disabled={busy || !!pending.current || (!grant.preserve_allowed && !grant.remove_allowed)}
                  onOpenChange={open => setOpenGrantMenus(old => {
                    const next = new Set(old); if (open) next.add(grant.grant_id); else next.delete(grant.grant_id); return next;
                  })}
                  onChange={event => setViewerGrants(old => {
                    const next = {...old};
                    if (event.target.value === "preserve") next[grant.grant_id] = {kind:"preserve"};
                    else if (event.target.value === "remove") next[grant.grant_id] = {kind:"remove",confirm:true};
                    else delete next[grant.grant_id];
                    return next;
                  })}>
                  <option value="">Choose how to handle this viewer grant</option>
                  {grant.preserve_allowed && <option value="preserve">Preserve viewer grant</option>}
                  {grant.remove_allowed && <option value="remove">Remove viewer grant</option>}
                </Select>
              </fieldset>;
            })}
            {(preview.inventory.pages ?? [])
              .filter(
                (page) =>
                  page.maintenance_user_id !== undefined || page.maintenance_group_id !== undefined,
              )
              .map((page) => {
                const choice = maintenance[page.id] ?? { kind: "preserve" as const };
                const targets = preview.inventory.maintenance_targets;
                const current = page.maintenance_user_id
                  ? (targets?.users.find((u) => u.id === page.maintenance_user_id)?.name ??
                    "Assigned person")
                  : page.maintenance_group_id
                    ? (targets?.groups.find((g) => g.id === page.maintenance_group_id)?.name ??
                      "Assigned team")
                    : "Unassigned";
                return (
                  <fieldset
                    key={`maintenance-${page.id}`}
                    className="border-border space-y-2 border p-3"
                  >
                    <legend className="max-w-full px-2 break-all">
                      Maintenance · {page.title}
                    </legend>
                    <p className="text-text-muted text-sm">
                      Current: {current}. Responsibility does not change access or draft ownership.
                    </p>
                    {page.maintenance_resolution_required && (
                      <p role="status" className="text-warning text-sm">
                        This assignment cannot be preserved in the destination. Select a person or
                        team, or clear it.
                      </p>
                    )}
                    {!page.maintenance_edit_allowed && (
                      <p className="text-text-muted text-sm">
                        You do not have permission to change this assignment.
                      </p>
                    )}
                    <Select
                      aria-label={`Maintenance action for ${page.title}`}
                      value={choice.kind}
                      disabled={busy || !!pending.current}
                      onChange={(event) =>
                        setMaintenance((old) => ({
                          ...old,
                          [page.id]:
                            event.target.value === "user"
                              ? { kind: "user", user_id: "" }
                              : event.target.value === "group"
                                ? { kind: "group", group_id: "" }
                                : { kind: event.target.value as "clear" | "preserve" },
                        }))
                      }
                    >
                      <option value="preserve">Preserve assignment</option>
                      <option value="clear" disabled={!page.maintenance_edit_allowed}>
                        Clear assignment
                      </option>
                      <option
                        value="user"
                        disabled={!page.maintenance_edit_allowed || !targets?.users.length}
                      >
                        Select person
                      </option>
                      <option
                        value="group"
                        disabled={
                          !page.maintenance_edit_allowed ||
                          !targets?.groups_available ||
                          !targets.groups.length
                        }
                      >
                        Select team
                      </option>
                    </Select>
                    {choice.kind === "user" && (
                      <Select
                        aria-label={`Destination maintenance person for ${page.title}`}
                        value={choice.user_id}
                        disabled={busy || !!pending.current}
                        onChange={(e) =>
                          setMaintenance((old) => ({
                            ...old,
                            [page.id]: { kind: "user", user_id: e.target.value },
                          }))
                        }
                      >
                        <option value="">Select a person</option>
                        {targets?.users.map((u) => (
                          <option key={u.id} value={u.id}>
                            {u.name}
                          </option>
                        ))}
                      </Select>
                    )}
                    {choice.kind === "group" && (
                      <Select
                        aria-label={`Destination maintenance team for ${page.title}`}
                        value={choice.group_id}
                        disabled={busy || !!pending.current}
                        onChange={(e) =>
                          setMaintenance((old) => ({
                            ...old,
                            [page.id]: { kind: "group", group_id: e.target.value },
                          }))
                        }
                      >
                        <option value="">Select a team</option>
                        {targets?.groups.map((g) => (
                          <option key={g.id} value={g.id}>
                            {g.name}
                          </option>
                        ))}
                      </Select>
                    )}
                  </fieldset>
                );
              })}
            {(preview.inventory.pages ?? [])
              .filter((page) => page.human_identifier || page.identifier_resolution_required)
              .map((page) => {
                const choice = identifiers[page.id] ?? {
                  kind: "preserve" as const,
                };
                return (
                  <fieldset key={page.id} className="border-border space-y-2 border p-3">
                    <legend className="max-w-full px-2 break-all">Identifier · {page.title}</legend>
                    <p className="text-text-muted text-sm">
                      Current: {page.human_identifier ?? "Unassigned"}. Stable page links keep the
                      same page ID.
                    </p>
                    {page.identifier_resolution_required && (
                      <p role="status" className="text-warning text-sm">
                        This identifier conflicts in the destination. Rename or clear it before
                        moving.
                      </p>
                    )}
                    {!page.identifier_edit_allowed && (
                      <p className="text-text-muted text-sm">
                        You do not have permission to change this identifier.
                      </p>
                    )}
                    <Select
                      aria-label={`Identifier action for ${page.title}`}
                      value={choice.kind}
                      disabled={busy || !!pending.current}
                      onChange={(event) =>
                        setIdentifiers((old) => ({
                          ...old,
                          [page.id]:
                            event.target.value === "rename"
                              ? {
                                  kind: "rename",
                                  value: page.human_identifier ?? "",
                                }
                              : {
                                  kind: event.target.value as "clear" | "preserve",
                                },
                        }))
                      }
                    >
                      <option value="preserve">Preserve identifier</option>
                      <option value="rename" disabled={!page.identifier_edit_allowed}>
                        Rename identifier
                      </option>
                      <option value="clear" disabled={!page.identifier_edit_allowed}>
                        Clear identifier
                      </option>
                    </Select>
                    {choice.kind === "rename" && (
                      <Input
                        aria-label={`New identifier for ${page.title}`}
                        value={choice.value}
                        maxLength={64}
                        disabled={busy || !!pending.current}
                        onChange={(event) =>
                          setIdentifiers((old) => ({
                            ...old,
                            [page.id]: {
                              kind: "rename",
                              value: event.target.value,
                            },
                          }))
                        }
                      />
                    )}
                  </fieldset>
                );
              })}
            <section aria-label="Metadata choices" className="space-y-3">
              {(preview.inventory.metadata ?? []).map((m) => {
                const c = choices[key(m)];
                const selection = c?.kind === "map" ? `map:${c.destination_id}` : (c?.kind ?? "");
                return (
                  <fieldset
                    key={key(m)}
                    className="border-border rounded-panel min-w-0 space-y-2 border p-3"
                  >
                    <legend className="text-text max-w-full px-1 text-sm font-medium [overflow-wrap:anywhere]">
                      {String(m.source_definition.name)} · {m.kind}
                    </legend>
                    <p className="text-text-muted text-xs">
                      {m.assignment_count} assignments. Preserve keeps inactive evidence; Remove
                      also retains evidence and removes active assignments.
                    </p>
                    <Select
                      aria-label={`Handle ${String(m.source_definition.name)}`}
                      value={selection}
                      disabled={busy || !!pending.current}
                      onChange={(e) => choose(m, e.target.value)}
                    >
                      <option value="">Choose an action</option>
                      {[...m.candidates, ...m.resolution_candidates].map((d) => (
                        <option key={d.id} value={`map:${d.id}`}>
                          {`Map to ${d.name}${d.compatibility === "requires_explicit_relation_targets" ? " (resolve every target)" : ""}`}
                        </option>
                      ))}
                      {m.create_allowed && (
                        <option value="create">Create a destination definition</option>
                      )}
                      <option value="preserve">Preserve inactive metadata</option>
                      <option value="remove">Remove active metadata (retain evidence)</option>
                    </Select>
                    {c?.kind === "create" && (
                      <Input
                        aria-label={`New ${m.kind} name`}
                        value={String(c.definition.name)}
                        disabled={busy || !!pending.current}
                        onChange={(e) =>
                          setChoices((v) => ({
                            ...v,
                            [key(m)]: {
                              ...c,
                              definition: {
                                ...c.definition,
                                name: e.target.value,
                              },
                            },
                          }))
                        }
                      />
                    )}{" "}
                    {(c?.kind === "map" || c?.kind === "create") &&
                      (m.assignments ?? []).map((a) => (
                        <label key={a.id} className="text-text-muted block text-xs">
                          Value for{" "}
                          {preview.inventory.pages?.find((p) => p.id === a.document_id)?.title ??
                            "page"}
                          <Input
                            value={c.values[a.id] ?? a.value ?? ""}
                            disabled={busy || !!pending.current}
                            onChange={(e) =>
                              setChoices((v) => ({
                                ...v,
                                [key(m)]: {
                                  ...c,
                                  values: {
                                    ...c.values,
                                    [a.id]: e.target.value,
                                  },
                                },
                              }))
                            }
                          />
                        </label>
                      ))}
                  </fieldset>
                );
              })}
            </section>
            {preview.inventory.relation_targets_unavailable_reason && (
              <p className="text-warning text-sm">
                {preview.inventory.relation_targets_unavailable_reason}
              </p>
            )}
            {(preview.inventory.relations ?? [])
              .filter((r) => r.crosses_subtree)
              .map((r) => (
                <fieldset
                  key={r.assignment_id}
                  className="border-border rounded-panel min-w-0 border p-3"
                >
                  <legend className="text-text px-1 text-sm">Crossing relationship</legend>
                  <Select
                    aria-label={`Relationship ${r.assignment_id}`}
                    value={relations[r.assignment_id]?.kind ?? ""}
                    disabled={busy || !!pending.current}
                    onChange={(e) => {
                      const kind = e.target.value;
                      if (!kind) {
                        setRelations((v) => {
                          const next = { ...v };
                          delete next[r.assignment_id];
                          return next;
                        });
                        return;
                      }
                      setRelations((v) => ({
                        ...v,
                        [r.assignment_id]:
                          kind === "map"
                            ? { kind: "map", target_id: "" }
                            : kind === "remove"
                              ? { kind: "remove", confirm: true }
                              : { kind: "preserve" },
                      }));
                    }}
                  >
                    <option value="">Choose an action</option>
                    <option value="map">Map target</option>
                    <option value="preserve">Preserve inactive evidence</option>
                    <option value="remove">Remove relationship (retain evidence)</option>
                  </Select>
                  {relations[r.assignment_id]?.kind === "map" && (
                    <Select
                      aria-label="Destination relationship target"
                      disabled={busy || !!pending.current}
                      value={(relations[r.assignment_id] as { target_id: string }).target_id}
                      onChange={(e) =>
                        setRelations((v) => ({
                          ...v,
                          [r.assignment_id]: {
                            kind: "map",
                            target_id: e.target.value,
                          },
                        }))
                      }
                    >
                      <option value="">Choose a visible page</option>
                      {[
                        ...(preview.inventory.pages ?? []),
                        ...(preview.inventory.relation_targets ?? []),
                      ].map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.title}
                        </option>
                      ))}
                    </Select>
                  )}
                </fieldset>
              ))}
            <section aria-label="Access consequences" className="space-y-2 text-sm">
              <p className="text-text-muted">
                Space audience: {preview.access_consequences.space_audience_gained} gained;{" "}
                {preview.access_consequences.space_audience_lost} lost. This changes no draft owner
                or explicit draft grant.
              </p>
              <label className="text-text block">
                Public links
                <Select
                  aria-label="Public links"
                  value={links}
                  disabled={busy || !!pending.current}
                  onChange={(e) => setLinks(e.target.value as "preserve" | "disable")}
                >
                  <option value="preserve">Preserve existing intentional links</option>
                  <option value="disable">Disable links on moved pages</option>
                </Select>
              </label>
              <label className="text-text flex items-start gap-2">
                <input
                  type="checkbox"
                  checked={audience}
                  disabled={busy || !!pending.current}
                  onChange={(e) => setAudience(e.target.checked)}
                />
                I reviewed the destination audience change.
              </label>
              <label className="text-text flex items-start gap-2">
                <input
                  type="checkbox"
                  checked={drafts}
                  disabled={busy || !!pending.current}
                  onChange={(e) => setDrafts(e.target.checked)}
                />
                I reviewed draft participation in the destination space.
              </label>
            </section>
            <p className="text-text-muted text-xs">
              Preserved metadata is retained as inactive evidence. Inspect it read-only in page
              Properties after a move; restoration is not available.
            </p>
          </>
        )}
      </DialogBody>
      <DialogFooter>
        <Button
          variant="secondary"
          disabled={busy || !!pending.current}
          onClick={() => void load()}
        >
          Review again
        </Button>
        <Button variant="secondary" disabled={busy} onClick={() => void close()}>
          Cancel
        </Button>
        <Button
          disabled={
            busy ||
            !preview ||
            (!pending.current &&
              !committed &&
              (!preview.commit_available ||
                expired ||
                preview.stale ||
                hasBlockingMoveReview(preview, complete) ||
                !complete ||
                !audience ||
                !drafts))
          }
          aria-describedby="move-readiness"
          onClick={() => (pending.current || committed ? void submit() : setConfirming(true))}
        >
          {committed ? "Refresh moved pages" : pending.current ? "Retry same move" : "Move"}
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
