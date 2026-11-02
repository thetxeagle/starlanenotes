import { useCallback, useContext, useEffect, useId, useRef, useState } from "react";
import { AuthContext } from "@/hooks/useAuth";
import { dataClient } from "@/lib/dataClient";
import { api, getApiBase, isTauri } from "@/lib/api";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { getStorageSelection } from "@/lib/storageProviders";
import type { LocalReviewedMoveRecord } from "@/lib/localReviewedMove";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";

type Target = { spaceId: string; documentId: string; accountId: string | null };
const unavailable = () => new Error("Preserved metadata is unavailable. Inspect again in the original workspace.");
function origin() {
  return JSON.stringify([getCurrentLaunchMode(), getStorageSelection(), getApiBase(), isTauri(), api.getToken(), api.getRefreshToken(), typeof localStorage === "undefined" ? null : localStorage.getItem("access_token"), typeof localStorage === "undefined" ? null : localStorage.getItem("refresh_token")]);
}

const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const nonempty = (value: unknown): value is string => typeof value === "string" && !!value.trim();
/** Display admission only: preserve unsupported imported history, never infer choices. */
function hasDisplayableDecisions(record: unknown): boolean {
  if (!object(record) || !object(record.decisions)) return false;
  const decisions = record.decisions;
  if (typeof decisions.preserve_previous_draft_placement !== "boolean") return false;
  const families = ["properties", "tags", "workflows", "relations", "identifiers", "maintenance", "detach_hierarchy"] as const;
  return families.every(family => {
    const rows = decisions[family];
    if (!Array.isArray(rows)) return false;
    return rows.every((row: unknown) => {
      if (!object(row)) return false;
      if (family === "detach_hierarchy") return nonempty(row.id) && (row.kind === "folder" || row.kind === "document") && row.choice === "detach";
      const id = family === "relations" ? row.value_id : family === "identifiers" || family === "maintenance" ? row.document_id : row.source_id;
      if (!nonempty(id) || !object(row.choice)) return false;
      const choice = row.choice;
      if (family === "identifiers") return choice.kind === "preserve" || choice.kind === "clear" || (choice.kind === "rename" && nonempty(choice.value));
      if (family === "maintenance") return choice.kind === "preserve" || choice.kind === "clear" || (choice.kind === "user" && nonempty(choice.user_id)) || (choice.kind === "group" && nonempty(choice.group_id));
      if (family === "relations") return choice.kind === "preserve" || choice.kind === "remove" || (choice.kind === "map" && nonempty(choice.document_id));
      return choice.kind === "preserve" || choice.kind === "remove" || ((choice.kind === "map" || choice.kind === "create") && nonempty(choice.destination_id));
    });
  });
}

/** Memory-only context fence; eligibility belongs to the actual local reader. */
export async function readLocalPreservedMetadata(
  original: Target, current: () => Target, selectionCurrent: () => boolean,
): Promise<{ records: LocalReviewedMoveRecord[]; assertCurrent: () => void }> {
  const captured = origin();
  const assertCurrent = () => {
    const selected = current();
    if (selected.spaceId !== original.spaceId || selected.documentId !== original.documentId || selected.accountId !== original.accountId || captured !== origin() || !selectionCurrent()) throw unavailable();
  };
  assertCurrent();
  let records: LocalReviewedMoveRecord[];
  try {
    records = await dataClient.inspectLocalReviewedMoveRecords(original.spaceId, original.documentId, () => {
      try { assertCurrent(); return true; } catch { return false; }
    });
  } catch { assertCurrent(); throw unavailable(); }
  assertCurrent();
  if (!Array.isArray(records) || !records.every(hasDisplayableDecisions) || new Set(records.map(row => row.id)).size !== records.length) throw unavailable();
  return { records, assertCurrent };
}

const fields: Record<string, readonly string[]> = {
  documents: ["id", "space_id", "folder_id", "parent_document_id", "parent_id", "identifier", "human_identifier", "maintenance_user_id", "maintenance_group_id"],
  folders: ["id", "space_id", "parent_id", "name", "icon", "sort_order"],
  property_schemas: ["id", "space_id", "name", "property_name", "property_type", "options", "position", "sort_order"],
  property_values: ["id", "document_id", "property_id", "value"],
  tags: ["id", "space_id", "name", "color"],
  doc_tags: ["document_id", "tag_id"],
  workflow_templates: ["id", "space_id", "name", "description", "steps", "stages", "definition", "approvers", "created_by", "created_at", "updated_at"],
  document_workflows: ["id", "document_id", "workflow_template_id", "stage_id", "current_stage_id", "status", "created_by", "created_at", "updated_at"],
  document_workflow_events: ["id", "document_id", "instance_id", "document_workflow_id", "from_stage_id", "to_stage_id", "event_type", "action", "created_by", "created_at"],
  assets: ["id", "space_id", "document_id", "size", "mime_type"],
};
const privateField = /body|content|markdown|draft|acl|permission|share|token|secret|credential|password|session|payload/i;
function safeValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(safeValue);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).filter(([key]) => !privateField.test(key)).map(([key, child]) => [key, safeValue(child)]));
  return value;
}
/** Display projection only: never render raw page/draft/ACL/token payloads. */
export function projectLocalMoveMetadata(collection: string, rows: unknown[]): unknown[] {
  const allowed = fields[collection];
  if (!allowed) return [];
  return rows.filter((row): row is Record<string, unknown> => !!row && typeof row === "object" && !Array.isArray(row)).map(row => Object.fromEntries(allowed.filter(key => Object.prototype.hasOwnProperty.call(row, key)).map(key => [key, safeValue(row[key])])));
}

export function LocalPreservedMetadataEvidence({ record }: { record: LocalReviewedMoveRecord }) {
  if (!hasDisplayableDecisions(record)) return <p className="text-text-muted text-sm">Preserved metadata is unavailable. Inspect again in the original workspace.</p>;
  const groups: { label: string; collection: string; rows: unknown[] }[] = [];
  for (const [label, inventory] of [["Before move", record.before], ["Expected after move", record.after]] as const) {
    for (const [collection, rows] of Object.entries(inventory)) {
      const projected = projectLocalMoveMetadata(collection, rows ?? []);
      if (projected.length) groups.push({ label, collection, rows: projected });
    }
  }
  for (const [label, entries] of [["Preserved inactive", record.preserved], ["Removed from active metadata", record.removed]] as const) {
    for (const entry of entries) {
      const projected = projectLocalMoveMetadata(entry.collection, entry.records);
      if (projected.length) groups.push({ label, collection: entry.collection, rows: projected });
    }
  }
  const choices = [
    ...record.decisions.properties.map(row => ({ family: "Property", id: row.source_id, choice: row.choice.kind })),
    ...record.decisions.tags.map(row => ({ family: "Tag", id: row.source_id, choice: row.choice.kind })),
    ...record.decisions.workflows.map(row => ({ family: "Workflow", id: row.source_id, choice: row.choice.kind })),
    ...record.decisions.relations.map(row => ({ family: "Relation", id: row.value_id, choice: row.choice.kind })),
    ...record.decisions.identifiers.map(row => ({ family: "Identifier", id: row.document_id, choice: row.choice.kind })),
    ...record.decisions.maintenance.map(row => ({ family: "Responsibility", id: row.document_id, choice: row.choice.kind })),
    ...record.decisions.detach_hierarchy.map(row => ({ family: "Previous parent", id: row.id, choice: row.choice })),
  ];
  return <div className="space-y-2">
    {record.schema_version === 2 && <p role="status" className="text-warning text-sm">Committed locally in the original Google workspace; provider publication and application remain pending. This history is not a remote acknowledgment.</p>}
    <p className="text-text-muted text-xs">Historical local move metadata. Read-only; restoration is unavailable. Expected values describe the move outcome, not current page values. Page and draft bodies and access records are not displayed.</p>
    {choices.map((row, index) => <p key={`${row.family}/${row.id}/${index}`} className="text-text-muted break-words text-xs">{row.family} {row.id}: {row.choice}</p>)}
    {!groups.length && <p className="text-text-muted text-sm">No displayable metadata is available for this operation.</p>}
    {groups.map((group, index) => <details key={`${group.label}/${group.collection}/${index}`} className="border-border rounded-input border p-2">
      <summary className="focus-ring cursor-pointer text-sm">{group.collection.replace(/_/g, " ")} — {group.label}</summary>
      <pre className="text-text-muted mt-2 [overflow-wrap:anywhere] break-words whitespace-pre-wrap text-xs">{JSON.stringify(group.rows, null, 2)}</pre>
    </details>)}
  </div>;
}

export function LocalPagePreservedMetadata({ spaceId, documentId, selectionCurrent = () => true }: { spaceId: string; documentId: string; selectionCurrent?: () => boolean }) {
  const accountId = useContext(AuthContext)?.user?.id ?? null;
  const current = useRef({ spaceId, documentId, accountId }); current.current = { spaceId, documentId, accountId };
  const selection = useRef(selectionCurrent); selection.current = selectionCurrent;
  const generation = useRef(0);
  const accepted = useRef<{ records: LocalReviewedMoveRecord[]; assertCurrent: () => void } | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [records, setRecords] = useState<LocalReviewedMoveRecord[]>([]);
  const [selected, setSelected] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const selectId = useId();
  const selectionKey = JSON.stringify(getStorageSelection());
  const clear = useCallback(() => { accepted.current = null; setRecords([]); setSelected(""); }, []);
  useEffect(() => { generation.current++; clear(); setExpanded(false); setBusy(false); setError(""); }, [spaceId, documentId, accountId, selectionKey, clear]);
  useEffect(() => () => { generation.current++; }, []);
  useEffect(() => {
    const check = () => { try { accepted.current?.assertCurrent(); } catch { generation.current++; clear(); setBusy(false); setError("Workspace changed. Inspect again."); } };
    window.addEventListener("storage", check); window.addEventListener("focus", check);
    return () => { window.removeEventListener("storage", check); window.removeEventListener("focus", check); };
  }, [clear]);
  async function inspect(operation?: string) {
    const sequence = ++generation.current; clear(); setBusy(true); setError("");
    const original = { ...current.current };
    try {
      const next = await readLocalPreservedMetadata(original, () => current.current, () => sequence === generation.current && selection.current());
      next.assertCurrent(); if (sequence !== generation.current) return;
      if (operation && !next.records.some(row => row.id === operation)) throw unavailable();
      accepted.current = next; setRecords(next.records); setSelected(operation ?? "");
    } catch { if (sequence === generation.current) { clear(); setError("Preserved metadata is unavailable. Inspect again in the original workspace."); } }
    finally { if (sequence === generation.current) setBusy(false); }
  }
  const record = records.find(row => row.id === selected);
  let reportCurrent = true; try { accepted.current?.assertCurrent(); } catch { reportCurrent = false; }
  return <section className="border-border mt-3 border-t pt-3" aria-label="Preserved metadata">
    <Button variant="ghost" size="sm" aria-expanded={expanded} onClick={() => { if (expanded) { generation.current++; clear(); setExpanded(false); setBusy(false); setError(""); } else { setExpanded(true); void inspect(); } }}>Preserved metadata</Button>
    {expanded && <div className="mt-2 space-y-3">
      <p className="text-text-muted text-xs">Inspect stored evidence from a past reviewed move. This point-in-time inspection does not restore metadata or grant access.</p>
      <Button variant="secondary" size="sm" disabled={busy} onClick={() => void inspect()}>Refresh operations</Button>
      {busy && <p role="status" className="text-text-muted text-sm">Loading preserved metadata…</p>}
      {error && <p role="alert" className="text-destructive text-sm">{error}</p>}
      {!reportCurrent && <p role="alert" className="text-destructive text-sm">Workspace changed. Inspect again.</p>}
      {reportCurrent && records.length > 0 && <><label htmlFor={selectId} className="text-text block text-xs">Move operation</label><Select id={selectId} value={selected} disabled={busy} onChange={event => { if (event.target.value) void inspect(event.target.value); }}><option value="">Select an operation</option>{records.map(row => <option key={row.id} value={row.id}>{`${row.moved_at} — ${row.id}`}</option>)}</Select></>}
      {reportCurrent && !busy && !error && !records.length && <p className="text-text-muted text-sm">No preserved move metadata is available for this page.</p>}
      {reportCurrent && record && <LocalPreservedMetadataEvidence record={record} />}
    </div>}
  </section>;
}
