/** Pure standalone transform. The caller must consume it inside the latest canonical
 * snapshot transaction, under its original provider/account/selection guard. */
import type { LocalVaultSnapshot, VaultPropertySchema, VaultTag, VaultWorkflowTemplate } from "./localVault";
import { buildLocalMovePreview, resolveLocalMoveIdentifiers, resolveLocalMoveMaintenance } from "./reviewedMoves";
import type { LocalMovePreview, LocalMovePreviewIntent } from "./reviewedMoves";
import { validatePropertySchema, validatePropertyValue } from "./propertyValidation";
import type { ServerMoveIdentifierChoice, ServerMoveMaintenanceChoice } from "@starlane-notes/shared-types";

export type LocalMoveMetadataChoice =
  | { kind: "map"; destination_id: string }
  | { kind: "create"; destination_id: string }
  | { kind: "preserve" }
  | { kind: "remove" };
export interface LocalReviewedMoveDecisions {
  properties: { source_id: string; choice: LocalMoveMetadataChoice }[];
  workflows: { source_id: string; choice: LocalMoveMetadataChoice }[];
  tags: { source_id: string; choice: LocalMoveMetadataChoice }[];
  relations: { value_id: string; choice: { kind: "map"; document_id: string } | { kind: "preserve" } | { kind: "remove" } }[];
  identifiers: { document_id: string; choice: ServerMoveIdentifierChoice }[];
  maintenance: { document_id: string; choice: ServerMoveMaintenanceChoice }[];
  /** Explicit detach of root's old cross-space hierarchy only; descendants never drop. */
  detach_hierarchy: { kind: "folder" | "document"; id: string; choice: "detach" }[];
  /** Old draft placement is retained in evidence, never silently redirected. */
  preserve_previous_draft_placement: boolean;
}
type EvidenceCollection = "folders" | "documents" | "drafts" | "property_schemas" | "property_values" | "tags" | "doc_tags" | "workflow_templates" | "document_workflows" | "document_workflow_events" | "assets" | "search_index";
interface ReviewedMoveRecordFields {
  id: string;
  actor_id: string;
  moved_at: string;
  intent: LocalMovePreviewIntent;
  decisions: LocalReviewedMoveDecisions;
  /** Full raw participating/incident rows; not a server capability or file receipt. */
  before: Partial<Record<EvidenceCollection, unknown[]>>;
  after: Partial<Record<EvidenceCollection, unknown[]>>;
  preserved: { collection: EvidenceCollection; records: unknown[] }[];
  removed: { collection: EvidenceCollection; records: unknown[] }[];
}
export interface GoogleReviewedMoveOrigin {
  schema_version: 1;
  provider: "google-drive";
  account_permission_id: string;
  root_id: string;
}
export type LocalReviewedMoveRecord = ReviewedMoveRecordFields & (
  | { schema_version: 1; provider: "none" }
  | { schema_version: 2; provider: "google-drive"; origin: GoogleReviewedMoveOrigin }
);
export interface GoogleReviewedMoveEnvelope {
  schema_version: 1;
  kind: "reviewed_move";
  origin: GoogleReviewedMoveOrigin;
  source_inventory_signature: string;
  intent: LocalMovePreviewIntent;
  decisions: LocalReviewedMoveDecisions;
  record: LocalReviewedMoveRecord;
  /** Exact local owned records, including available embedded bytes; not remote portability proof. */
  portable_asset_requirements: unknown[];
}
const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
function reject(message = "Reviewed move requires a fresh complete review"): never { throw new Error(message); }
function selections<T extends { source_id: string; choice: LocalMoveMetadataChoice }>(rows: T[], expected: string[]): Map<string, LocalMoveMetadataChoice> {
  const result = new Map<string, LocalMoveMetadataChoice>();
  for (const row of rows) {
    if (!expected.includes(row.source_id) || result.has(row.source_id) || !["map", "create", "preserve", "remove"].includes(row.choice.kind)) reject();
    result.set(row.source_id, row.choice);
  }
  if (result.size !== expected.length) reject("Choose map, create, preserve or remove for every metadata binding");
  return result;
}

/** Does not enable preview.commit_available, mutate input, queue replay, or touch files. */
export function prepareLocalReviewedMove(
  committed: LocalVaultSnapshot,
  reviewed: LocalMovePreview,
  input: { operation_id: string; actor_id: string; moved_at: string; decisions: LocalReviewedMoveDecisions },
): { snapshot: LocalVaultSnapshot; record: LocalReviewedMoveRecord } {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(input.operation_id) || !input.actor_id || !Number.isFinite(Date.parse(input.moved_at))) reject();
  if (committed.reviewed_move_records?.some((record) => record.id === input.operation_id)) reject("Move operation already exists; inspect its retained record");
  const preview = buildLocalMovePreview(committed, reviewed.intent, input.actor_id);
  if (preview.current_user_id !== reviewed.current_user_id || preview.internal_inventory_signature !== reviewed.internal_inventory_signature || preview.unavailable_participation) reject("Move source changed; review again");
  if (preview.intent.source_space_id === preview.intent.destination.space_id) reject("Use the existing same-space placement action");
  const decisions = copy(input.decisions);
  const resolvable = new Set(["external_relations_require_resolution", "external_hierarchy_requires_resolution", "property_identity_requires_resolution"]);
  if (preview.blockers.some((blocker) => !resolvable.has(blocker))) reject("Move has unavailable or invalid participating metadata");
  const inventory = preview.inventory;
  const docIds = new Set(inventory.documents.map((row) => row.id));
  const folderIds = new Set(inventory.folders.map((row) => row.id));
  const destination = preview.intent.destination.space_id;
  const detachKeys = new Set(decisions.detach_hierarchy.map((row) => JSON.stringify([row.kind, row.id])));
  if (detachKeys.size !== decisions.detach_hierarchy.length || detachKeys.size !== inventory.external_hierarchy.length || inventory.external_hierarchy.some((row) => row.boundary !== "parent_outside_closure" || !detachKeys.has(JSON.stringify([row.kind, row.id]))) || decisions.detach_hierarchy.some((row) => row.choice !== "detach")) reject("Review every old parent detachment; all descendants must remain in the move");
  if (preview.warnings.includes("draft_previous_placement_requires_review") && !decisions.preserve_previous_draft_placement) reject("Review preserved previous draft placement");
  const snapshot = copy(committed);
  const record: LocalReviewedMoveRecord = { schema_version: 1, id: input.operation_id, provider: "none", actor_id: input.actor_id, moved_at: input.moved_at, intent: copy(preview.intent), decisions, before: {}, after: {}, preserved: [], removed: [] };
  const escrow = (collection: EvidenceCollection, rows: unknown[], kind: "preserve" | "remove") => {
    if (rows.length) record[kind === "preserve" ? "preserved" : "removed"].push({ collection, records: copy(rows) });
  };
  const props = selections(decisions.properties, inventory.property_schemas.map((row) => row.id));
  const workflows = selections(decisions.workflows, inventory.workflow_templates.map((row) => row.id));
  const tags = selections(decisions.tags, inventory.tags.map((row) => row.id));
  const relationChoices = new Map(decisions.relations.map((row) => [row.value_id, row.choice]));
  if (relationChoices.size !== decisions.relations.length || relationChoices.size !== inventory.external_relations.length || inventory.external_relations.some((row) => !relationChoices.has(row.value.id))) reject("Choose every incoming and outgoing relation explicitly");
  const affectedValues = new Set(inventory.property_values.map((row) => row.id));
  const incidentValues = new Set(inventory.external_relations.map((row) => row.value.id));
  const relationOverrides = new Map<string, string>();
  const removedValues = new Set<string>();
  for (const edge of inventory.external_relations) {
    const choice = relationChoices.get(edge.value.id)!;
    if (!["map", "preserve", "remove"].includes(choice.kind)) reject();
    if (choice.kind !== "map") {
      removedValues.add(edge.value.id); escrow("property_values", [edge.value], choice.kind); continue;
    }
    const target = committed.documents.find((row) => row.id === choice.document_id);
    const expectedSpace = edge.direction === "outgoing" ? destination : committed.documents.find((row) => row.id === edge.value.document_id)?.space_id;
    const finalSpace = target && docIds.has(target.id) ? destination : target?.space_id;
    const targetDrafts = target ? (committed.drafts ?? []).filter(row => row.document_id === target.id) : [];
    const readable = target && !target.archived_at && !target.deleted_at && (target.published_at || targetDrafts.some(row => row.space_id === target.space_id && row.owner_id === input.actor_id) || (!targetDrafts.length && (!target.created_by || target.created_by === input.actor_id || (input.actor_id === "local-user" && target.created_by === "Local User"))));
    if (!target || !readable || finalSpace !== expectedSpace) reject("Choose a readable relation target in its final space");
    const propertyChoice = props.get(edge.value.property_id);
    if (edge.direction === "outgoing" && propertyChoice && ["preserve", "remove"].includes(propertyChoice.kind)) reject("A mapped relation needs an active mapped or created property");
    relationOverrides.set(edge.value.id, target.id);
  }
  const created = new Set<string>();
  const mapped = new Map<string, string>();
  const ensureCreated = (id: string, existing: { id: string }[]) => {
    if (!id.trim() || created.has(id) || existing.some((row) => row.id === id)) reject("Created metadata needs a fresh identity");
    created.add(id);
  };
  for (const source of inventory.property_schemas) {
    const choice = props.get(source.id)!;
    const rows = inventory.property_values.filter((row) => row.property_id === source.id && !removedValues.has(row.id));
    if (choice.kind === "preserve" || choice.kind === "remove") {
      escrow("property_schemas", [source], choice.kind); escrow("property_values", rows, choice.kind);
      rows.forEach((row) => removedValues.add(row.id)); continue;
    }
    let target: VaultPropertySchema;
    if (choice.kind === "create") {
      ensureCreated(choice.destination_id, snapshot.property_schemas ?? []);
      target = { ...copy(source), id: choice.destination_id, space_id: destination };
      snapshot.property_schemas = [...(snapshot.property_schemas ?? []), target];
    } else {
      const found = (snapshot.property_schemas ?? []).find((row) => row.id === choice.destination_id && row.space_id === destination);
      if (!found || found.property_type !== source.property_type) reject("Map to a compatible destination property");
      target = found;
    }
    validatePropertySchema(target);
    for (const row of rows) {
      const value = relationOverrides.get(row.id) ?? row.value;
      validatePropertyValue(target, value);
      if (target.property_type === "user" && value && value !== input.actor_id) reject("Property person unavailable");
      if (target.property_type === "relation" && value) {
        const page = snapshot.documents.find((item) => item.id === value);
        if (!page || (!docIds.has(value) && page.space_id !== destination)) reject("Relation target must belong to its final destination");
      }
    }
    mapped.set(source.id, target.id);
  }
  snapshot.property_values = (snapshot.property_values ?? []).filter((row) => !removedValues.has(row.id)).map((row) => ({ ...row, ...(affectedValues.has(row.id) ? { property_id: mapped.get(row.property_id) ?? row.property_id } : {}), ...(relationOverrides.has(row.id) ? { value: relationOverrides.get(row.id)! } : {}) }));
  const propertyKeys = snapshot.property_values.map((row) => JSON.stringify([row.document_id, row.property_id]));
  if (new Set(propertyKeys).size !== propertyKeys.length) reject("Property mappings collide on a page");
  const removedWorkflowIds = new Set<string>();
  const workflowMap = new Map<string, string>();
  for (const source of inventory.workflow_templates) {
    const choice = workflows.get(source.id)!;
    const rows = inventory.document_workflows.filter((row) => row.workflow_template_id === source.id);
    const ids = new Set(rows.map((row) => row.id));
    if (choice.kind === "preserve" || choice.kind === "remove") {
      escrow("workflow_templates", [source], choice.kind); escrow("document_workflows", rows, choice.kind);
      escrow("document_workflow_events", inventory.document_workflow_events.filter((row) => ids.has(row.document_workflow_id)), choice.kind);
      ids.forEach((id) => removedWorkflowIds.add(id)); continue;
    }
    let target: VaultWorkflowTemplate;
    if (choice.kind === "create") {
      ensureCreated(choice.destination_id, snapshot.workflow_templates ?? []);
      target = { ...copy(source), id: choice.destination_id, space_id: destination };
      snapshot.workflow_templates = [...(snapshot.workflow_templates ?? []), target];
    } else {
      const candidate = preview.mapping_candidates.workflows.find((row) => row.source_id === source.id && row.destination_id === choice.destination_id && row.compatible);
      const found = snapshot.workflow_templates?.find((row) => row.id === candidate?.destination_id);
      if (!found) reject("Map to a workflow with compatible stages and approvers");
      target = found;
    }
    workflowMap.set(source.id, target.id);
  }
  snapshot.document_workflows = (snapshot.document_workflows ?? []).filter((row) => !removedWorkflowIds.has(row.id)).map((row) => docIds.has(row.document_id) ? { ...row, workflow_template_id: workflowMap.get(row.workflow_template_id) ?? row.workflow_template_id } : row);
  snapshot.document_workflow_events = (snapshot.document_workflow_events ?? []).filter((row) => !removedWorkflowIds.has(row.document_workflow_id));
  const tagMap = new Map<string, string>();
  const removedTags = new Set<string>();
  for (const source of inventory.tags) {
    const choice = tags.get(source.id)!;
    if (choice.kind === "preserve" || choice.kind === "remove") {
      escrow("tags", [source], choice.kind); escrow("doc_tags", inventory.doc_tags.filter((row) => row.tag_id === source.id), choice.kind); removedTags.add(source.id); continue;
    }
    let target: VaultTag;
    if (choice.kind === "create") {
      ensureCreated(choice.destination_id, snapshot.tags ?? []); target = { ...copy(source), id: choice.destination_id, space_id: destination }; snapshot.tags = [...(snapshot.tags ?? []), target];
    } else {
      const found = snapshot.tags?.find((row) => row.id === choice.destination_id && row.space_id === destination);
      if (!found) reject("Select a destination tag"); target = found;
    }
    tagMap.set(source.id, target.id);
  }
  snapshot.doc_tags = (snapshot.doc_tags ?? []).filter((row) => !docIds.has(row.document_id) || !removedTags.has(row.tag_id)).map((row) => docIds.has(row.document_id) ? { ...row, tag_id: tagMap.get(row.tag_id) ?? row.tag_id } : row);
  if (new Set(snapshot.doc_tags.map((row) => JSON.stringify([row.document_id, row.tag_id]))).size !== snapshot.doc_tags.length) reject("Tag mappings collide");
  const identifiers = new Map(resolveLocalMoveIdentifiers(preview, decisions.identifiers).map((row) => [row.document_id, row]));
  const maintenance = new Map(resolveLocalMoveMaintenance(preview, decisions.maintenance).map((row) => [row.document_id, row]));
  snapshot.folders = snapshot.folders.map((row) => folderIds.has(row.id) ? { ...row, space_id: destination, ...(detachKeys.has(JSON.stringify(["folder", row.id])) ? { parent_id: undefined } : {}), ...(row.id === preview.intent.root.id && preview.intent.root.kind === "folder" ? { parent_id: preview.intent.destination.parent_id ?? undefined } : {}) } : row);
  snapshot.documents = snapshot.documents.map((row) => docIds.has(row.id) ? { ...row, human_identifier: identifiers.get(row.id)!.human_identifier, maintenance_user_id: maintenance.get(row.id)!.maintenance_user_id, maintenance_group_id: maintenance.get(row.id)!.maintenance_group_id, space_id: destination, updated_at: input.moved_at, last_updated_by: input.actor_id, ...(detachKeys.has(JSON.stringify(["document", row.id])) ? { parent_id: undefined } : {}), ...(preview.intent.root.kind === "document" ? { folder_id: preview.intent.destination.folder_id ?? undefined, ...(row.id === preview.intent.root.id ? { parent_id: preview.intent.destination.parent_id ?? undefined } : {}) } : {}) } : row);
  snapshot.drafts = (snapshot.drafts ?? []).map((row) => docIds.has(row.document_id) ? { ...row, space_id: destination } : row);
  // Ownership follows its participating page; shared/global and unrelated assets stay put.
  // URL/path/identity remain exact: relocation does not rewrite or delete any file.
  snapshot.assets = snapshot.assets.map(row => row.document_id && docIds.has(row.document_id) ? { ...row, space_id: destination } : row);
  snapshot.search_index = (snapshot.search_index ?? []).map((row) => docIds.has(row.document_id) ? { ...row, space_id: destination } : row);
  const collections: EvidenceCollection[] = ["folders", "documents", "drafts", "property_schemas", "property_values", "tags", "doc_tags", "workflow_templates", "document_workflows", "document_workflow_events", "assets", "search_index"];
  // Preserve participating rows, selected definitions and changed incident rows only.
  // Unrelated private drafts and metadata never enter this operation's evidence.
  const key = (collection: EvidenceCollection, row: unknown): string => {
    const item = row as { id?: string; document_id?: string; tag_id?: string };
    return collection === "doc_tags" ? JSON.stringify([item.document_id, item.tag_id]) : collection === "search_index" ? item.document_id! : item.id!;
  };
  for (const collection of collections) {
    const before: unknown[] = committed[collection] ?? [];
    const after: unknown[] = snapshot[collection] ?? [];
    const inventoryRows: unknown[] = inventory[collection] ?? [];
    const wanted = new Set(inventoryRows.map((row) => key(collection, row)));
    if (collection === "property_values") incidentValues.forEach((id) => wanted.add(id));
    if (collection === "property_schemas") {
      mapped.forEach((id) => wanted.add(id));
      // Incoming edges can use a definition owned outside the moving subtree.
      // Capture its exact reviewed type so future readers never infer from current schemas.
      inventory.relation_schemas.forEach((row) => wanted.add(row.id));
    }
    if (collection === "workflow_templates") workflowMap.forEach((id) => wanted.add(id));
    if (collection === "tags") tagMap.forEach((id) => wanted.add(id));
    const oldRows = new Map(before.map((row) => [key(collection, row), row]));
    const newRows = new Map(after.map((row) => [key(collection, row), row]));
    for (const id of new Set([...oldRows.keys(), ...newRows.keys()])) {
      if (JSON.stringify(oldRows.get(id)) !== JSON.stringify(newRows.get(id))) wanted.add(id);
    }
    record.before[collection] = copy(before.filter((row) => wanted.has(key(collection, row))));
    record.after[collection] = copy(after.filter((row) => wanted.has(key(collection, row))));
  }
  snapshot.reviewed_move_records = [...(snapshot.reviewed_move_records ?? []), record];
  return { snapshot, record: copy(record) };
}

/** Same pure relocation as standalone, with explicitly different Google evidence. */
export function prepareGoogleReviewedMove(
  committed: LocalVaultSnapshot, reviewed: LocalMovePreview,
  input: Parameters<typeof prepareLocalReviewedMove>[2], origin: GoogleReviewedMoveOrigin,
): { snapshot: LocalVaultSnapshot; record: LocalReviewedMoveRecord; envelope: GoogleReviewedMoveEnvelope } {
  if (origin.schema_version !== 1 || origin.provider !== "google-drive" || !origin.account_permission_id.trim() || !origin.root_id.trim()) reject("Original Google move enrollment is unavailable");
  const prepared = prepareLocalReviewedMove(committed, reviewed, input);
  const record: LocalReviewedMoveRecord = { ...prepared.record, schema_version: 2, provider: "google-drive", origin: copy(origin) };
  prepared.snapshot.reviewed_move_records = [...(committed.reviewed_move_records ?? []), record];
  const envelope: GoogleReviewedMoveEnvelope = {
    schema_version: 1, kind: "reviewed_move", origin: copy(origin),
    source_inventory_signature: reviewed.internal_inventory_signature,
    intent: copy(record.intent), decisions: copy(record.decisions), record: copy(record),
    portable_asset_requirements: copy(record.before.assets ?? []),
  };
  return { snapshot: prepared.snapshot, record, envelope };
}
