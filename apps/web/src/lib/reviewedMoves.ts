import { buildPageIndex, createPageTraversalBudget } from "./pageHierarchy";
import { buildFolderIndex, createFolderTraversalBudget, getFolderAncestors, getFolderSubtree } from "./folderHierarchy";
/** Pure standalone-local move inventory. This module grants no relocation or server authority. */
import type {
  LocalVaultSnapshot,
  VaultDocument,
  VaultDocumentDraft,
  VaultPropertySchema,
  VaultPropertyValue,
  VaultWorkflowTemplate,
} from "./localVault";
import { normalizeHumanIdentifier, humanIdentifierKey } from "./humanIdentifier";
import {
  normalizeDocumentMaintenanceFields,
  normalizeDocumentMaintenanceCatalogs,
} from "./documentMaintenance";
import type { DocumentMaintenanceTargets, DocumentMaintenanceCatalog } from "./documentMaintenance";
import type {
  ServerMoveMaintenanceChoice,
  ServerMoveIdentifierChoice,
} from "@starlane-notes/shared-types";
import { validatePropertySchema, validatePropertyValue } from "./propertyValidation";

export interface LocalMovePreviewIntent {
  source_space_id: string;
  root: { kind: "document" | "folder"; id: string };
  destination: {
    space_id: string;
    folder_id: string | null;
    parent_id: string | null;
  };
}
export interface LocalMoveMappingCandidate {
  source_id: string;
  destination_id: string;
  compatible: boolean;
  reasons: string[];
}
export interface LocalMovePreview {
  intent: LocalMovePreviewIntent;
  /** Standalone local identity; caller input grants no server permissions. */
  current_user_id: string;
  /** A new fully accessible preview is required; choices cannot waive this flag. */
  unavailable_participation: boolean;
  /** INTERNAL ONLY: may contain readable raw content. Never render or send as server authority.
   * Recompute synchronously against canonical data; a signature never overrides blockers. */
  internal_inventory_signature: string;
  inventory: {
    folders: LocalVaultSnapshot["folders"];
    documents: VaultDocument[];
    drafts: VaultDocumentDraft[];
    property_schemas: VaultPropertySchema[];
    property_values: VaultPropertyValue[];
    tags: NonNullable<LocalVaultSnapshot["tags"]>;
    doc_tags: NonNullable<LocalVaultSnapshot["doc_tags"]>;
    workflow_templates: VaultWorkflowTemplate[];
    document_workflows: NonNullable<LocalVaultSnapshot["document_workflows"]>;
    document_workflow_events: NonNullable<LocalVaultSnapshot["document_workflow_events"]>;
    assets: LocalVaultSnapshot["assets"];
    search_index: NonNullable<LocalVaultSnapshot["search_index"]>;
    source_ancestors: {
      folders: LocalVaultSnapshot["folders"];
      documents: VaultDocument[];
    };
    asset_references: {
      asset_id: string;
      document_id: string;
      draft_id?: string;
    }[];
    relation_schemas: VaultPropertySchema[];
    external_relations: {
      direction: "incoming" | "outgoing";
      value: VaultPropertyValue;
    }[];
    external_hierarchy: {
      kind: "folder" | "document";
      id: string;
      boundary: string;
    }[];
    /** Renderable queue identity summary; raw payloads belong only in the internal signature. */
    queued_operations: {
      id: string;
      entity_type: string;
      action: string;
      queued_at: string;
    }[];
    maintenance: {
      document_id: string;
      maintenance_user_id: string | null;
      maintenance_group_id: string | null;
      resolution_required: boolean;
    }[];
    maintenance_catalogs: DocumentMaintenanceCatalog[];
    destination_maintenance_targets: DocumentMaintenanceTargets | null;
    identifiers: {
      document_id: string;
      human_identifier: string | null;
      resolution_required: boolean;
    }[];
    /** Internal local namespace evidence, including archive/trash; never server authority. */
    destination_identifier_rows: {
      id: string;
      human_identifier: string | null;
    }[];
    destination: {
      space: LocalVaultSnapshot["spaces"][number];
      folders: LocalVaultSnapshot["folders"];
      documents: VaultDocument[];
      property_schemas: VaultPropertySchema[];
      tags: NonNullable<LocalVaultSnapshot["tags"]>;
      workflow_templates: VaultWorkflowTemplate[];
    };
  };
  mapping_candidates: {
    properties: LocalMoveMappingCandidate[];
    workflows: LocalMoveMappingCandidate[];
  };
  blockers: string[];
  warnings: string[];
  commit_available: false;
}

const lexical = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);

function canonical(value: unknown): string {
  const ordered = (item: unknown): unknown => {
    if (Array.isArray(item)) return item.map(ordered);
    if (item && typeof item === "object")
      return Object.fromEntries(
        Object.entries(item)
          .filter(([, v]) => v !== undefined)
          .sort(([a], [b]) => lexical(a, b))
          .map(([key, v]) => [key, ordered(v)]),
      );
    return item;
  };
  return JSON.stringify(ordered(value));
}
function sorted<T>(records: readonly T[]): T[] {
  return [...records].sort((a, b) => lexical(canonical(a), canonical(b)));
}
function detached<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}
function templates(records: readonly VaultWorkflowTemplate[]): VaultWorkflowTemplate[] {
  return sorted(
    records.map((item) => ({
      ...item,
      stages: sorted(item.stages),
      approvers: sorted(item.approvers),
    })),
  );
}
function errorMessage(action: () => void): string[] {
  try {
    action();
    return [];
  } catch (cause) {
    return [cause instanceof Error ? cause.message : "Invalid metadata"];
  }
}

export function buildLocalMovePreview(
  snapshot: LocalVaultSnapshot,
  intent: LocalMovePreviewIntent,
  currentUserId = "local-user",
): LocalMovePreview {
  const blockers = new Set<string>();
  const warnings = new Set<string>();
  const unavailable = () => new Error("Move preview unavailable");
  if (
    !currentUserId ||
    !intent.source_space_id ||
    !intent.root.id ||
    !["document", "folder"].includes(intent.root.kind)
  )
    throw unavailable();
  const source = snapshot.spaces.find((item) => item.id === intent.source_space_id);
  const destination = snapshot.spaces.find((item) => item.id === intent.destination.space_id);
  if (!source || !destination) throw unavailable();
  const docs = new Map(snapshot.documents.map((item) => [item.id, item]));
  const folders = new Map(snapshot.folders.map((item) => [item.id, item]));
  const drafts = snapshot.drafts ?? [];
  const localOwner = (owner: string | undefined) =>
    !owner || owner === currentUserId || (currentUserId === "local-user" && owner === "Local User");
  const available = (doc: VaultDocument | undefined): doc is VaultDocument =>
    !!doc &&
    !doc.archived_at &&
    !doc.deleted_at &&
    !!(
      doc.published_at ||
      drafts.some(
        (draft) =>
          draft.document_id === doc.id &&
          draft.space_id === doc.space_id &&
          draft.owner_id === currentUserId,
      ) ||
      (!drafts.some((draft) => draft.document_id === doc.id) && localOwner(doc.created_by))
    );
  if (intent.root.kind === "document") {
    const root = docs.get(intent.root.id);
    if (!available(root) || root.space_id !== source.id) throw unavailable();
  } else if (folders.get(intent.root.id)?.space_id !== source.id) throw unavailable();
  for (const records of [
    snapshot.documents,
    snapshot.folders,
    drafts,
    snapshot.property_schemas ?? [],
    snapshot.property_values ?? [],
    snapshot.tags ?? [],
    snapshot.workflow_templates ?? [],
    snapshot.document_workflows ?? [],
    snapshot.document_workflow_events ?? [],
    snapshot.assets,
  ]) {
    if (new Set(records.map((item) => item.id)).size !== records.length)
      blockers.add("duplicate_record_identity");
  }
  const folderIndex = buildFolderIndex(snapshot.folders);
  const folderBudget = createFolderTraversalBudget();
  const folderIds = new Set<string>();
  if (intent.root.kind === "folder") {
    try {
      for (const row of getFolderSubtree(folderIndex, source.id, intent.root.id, folderBudget)) folderIds.add(row.id);
    } catch (error) {
      if (error instanceof Error && error.message.includes("10000")) throw error;
      blockers.add(error instanceof Error && error.message.includes("cycle") ? "cyclic_hierarchy" : "invalid_hierarchy_edge");
      // Preserve the blocked preview's closure without repairing or silently truncating source.
      const pending = [intent.root.id];
      while (pending.length) {
        const id = pending.pop()!;
        if (folderIds.has(id)) continue;
        folderBudget.visit(id);
        folderIds.add(id);
        for (const child of folderIndex.children.get(id) ?? []) {
          if (child.space_id !== source.id) { blockers.add("cross_space_hierarchy_edge"); continue; }
          if (!folderIds.has(child.id)) { folderBudget.visit(child.id); pending.push(child.id); }
        }
      }
    }
  }
  const pageIndex = buildPageIndex(snapshot.documents), pageBudget = createPageTraversalBudget();
  const documentIds = new Set<string>();
  const pendingPages: string[] = intent.root.kind === "document" ? [intent.root.id] : [];
  if (intent.root.kind === "folder") for (const page of snapshot.documents) if (page.folder_id && folderIds.has(page.folder_id)) pendingPages.push(page.id);
  for (const id of pendingPages) pageBudget.visit(id);
  while (pendingPages.length) {
    const id = pendingPages.pop()!;
    if (documentIds.has(id)) continue;
    documentIds.add(id);
    for (const child of pageIndex.children.get(id) ?? []) {
      if (child.space_id !== source.id) { blockers.add("cross_space_hierarchy_edge"); continue; }
      if (!documentIds.has(child.id)) { pageBudget.visit(child.id); pendingPages.push(child.id); }
    }
  }
  const movedDocuments = snapshot.documents.filter((item) => documentIds.has(item.id));
  if (movedDocuments.some((item) => !available(item)))
    blockers.add("participating_page_unavailable");
  if (movedDocuments.some((item) => item.locked)) blockers.add("locked_items");
  const visibleDocs = movedDocuments.filter(available);
  const visibleIds = new Set(visibleDocs.map((item) => item.id));
  const movedDrafts = drafts.filter(
    (item) =>
      visibleIds.has(item.document_id) &&
      item.space_id === source.id &&
      item.owner_id === currentUserId,
  );
  if (
    drafts.some(
      (item) =>
        documentIds.has(item.document_id) &&
        (item.owner_id !== currentUserId || item.space_id !== source.id),
    )
  )
    blockers.add("participating_draft_unavailable");
  const externalHierarchy: LocalMovePreview["inventory"]["external_hierarchy"] = [];
  const inspectHierarchy = (
    kind: "folder" | "document",
    items: readonly { id: string; space_id: string; parent_id?: string }[],
    ids: Set<string>,
  ) => {
    for (const item of items) {
      if (ids.has(item.id)) {
        if (
          item.parent_id &&
          !ids.has(item.parent_id) &&
          (kind === "folder" || visibleIds.has(item.id))
        )
          externalHierarchy.push({
            kind,
            id: item.id,
            boundary: "parent_outside_closure",
          });
      } else if (item.parent_id && ids.has(item.parent_id)) {
        blockers.add("cross_space_hierarchy_edge");
        if (kind === "folder" || available(docs.get(item.id)))
          externalHierarchy.push({
            kind,
            id: item.id,
            boundary: "child_outside_closure",
          });
      }
    }
  };
  inspectHierarchy("folder", snapshot.folders, folderIds);
  inspectHierarchy("document", snapshot.documents, documentIds);
  if (
    intent.root.kind === "folder" &&
    movedDocuments.some((item) => item.folder_id && !folderIds.has(item.folder_id))
  )
    blockers.add("page_descendant_outside_folder_closure");
  for (const doc of movedDocuments)
    if (doc.folder_id && folders.get(doc.folder_id)?.space_id !== doc.space_id)
      blockers.add("invalid_document_folder");
  for (const doc of visibleDocs) {
    const parent = doc.parent_id ? docs.get(doc.parent_id) : undefined;
    if (parent && parent.folder_id !== doc.folder_id)
      blockers.add("invalid_document_parent_folder");
  }
  if (source.id !== destination.id && externalHierarchy.length)
    blockers.add("external_hierarchy_requires_resolution");
  if (
    movedDrafts.some(
      (draft) =>
        (draft.previous_folder_id && !folderIds.has(draft.previous_folder_id)) ||
        (draft.previous_parent_id && !documentIds.has(draft.previous_parent_id)),
    )
  )
    warnings.add("draft_previous_placement_requires_review");

  const sourceAncestorFolders = new Map<string, LocalVaultSnapshot["folders"][number]>();
  const sourceAncestorDocuments = new Map<string, VaultDocument>();
  const collectAncestors = (kind: "folder" | "document", initial: string | undefined) => {
    // A cycle already makes this immutable preview uncommittable. Rewalking its
    // participants cannot add authority or useful ancestry evidence.
    if (kind === "document" && blockers.has("cyclic_hierarchy")) return;
    if (kind === "folder") {
      if (initial && !sourceAncestorFolders.has(initial)) {
        try {
          for (const folder of getFolderAncestors(folderIndex, source.id, initial, folderBudget)) sourceAncestorFolders.set(folder.id, folder);
        } catch (error) {
          if (error instanceof Error && error.message.includes("10000")) throw error;
          blockers.add(error instanceof Error && error.message.includes("cycle") ? "cyclic_hierarchy" : "invalid_hierarchy_edge");
        }
      }
      return;
    }
    if (initial && !sourceAncestorDocuments.has(initial)) {
      try {
        const trail: VaultDocument[] = [], seen = new Set<string>();
        let id: string | undefined = initial;
        const folder = docs.get(initial)?.folder_id ?? null;
        while (id && !sourceAncestorDocuments.has(id)) {
          if (seen.has(id)) throw new Error("Page hierarchy contains a cycle");
          seen.add(id); pageBudget.visit(id);
          const page: VaultDocument | undefined = docs.get(id);
          if (!page || page.space_id !== source.id || (page.folder_id ?? null) !== folder) throw new Error("Page hierarchy unavailable");
          if (!available(page)) { blockers.add("hierarchy_endpoint_unavailable"); break; }
          trail.push(page); id = page.parent_id;
        }
        for (const page of trail) sourceAncestorDocuments.set(page.id, page);
      } catch (error) {
        if (error instanceof Error && error.message.includes("10000")) throw error;
        blockers.add(error instanceof Error && error.message.includes("cycle") ? "cyclic_hierarchy" : "invalid_hierarchy_edge");
      }
    }
  };
  for (const doc of visibleDocs) {
    collectAncestors("folder", doc.folder_id);
    collectAncestors("document", doc.parent_id);
  }
  for (const id of folderIds) collectAncestors("folder", folders.get(id)?.parent_id);
  const destinationFolders = new Map<string, LocalVaultSnapshot["folders"][number]>();
  const destinationDocuments = new Map<string, VaultDocument>();
  const inspectPlacement = (kind: "folder" | "document", id: string | null) => {
    if (id === null) return;
    if (kind === "folder") {
      try {
        for (const folder of getFolderAncestors(folderIndex, destination.id, id, folderBudget)) {
          if (folderIds.has(folder.id)) blockers.add("destination_inside_closure");
          destinationFolders.set(folder.id, folder);
        }
      } catch (error) {
        if (error instanceof Error && error.message.includes("10000")) throw error;
        blockers.add("destination_placement_unavailable");
      }
      return;
    }
    const seen = new Set<string>();
    let next: string | undefined = id;
    while (next) {
      if (seen.has(next)) {
        blockers.add("cyclic_destination_hierarchy");
        return;
      }
      seen.add(next);
      pageBudget.visit(next);
      const item = docs.get(next);
      if (!item || item.space_id !== destination.id || !available(item)) {
        blockers.add("destination_placement_unavailable");
        return;
      }
      if (item.locked) blockers.add("destination_locked");
      if (documentIds.has(next)) blockers.add("destination_inside_closure");
      destinationDocuments.set(next, item);
      if (item.folder_id !== (intent.destination.folder_id ?? undefined))
        blockers.add("destination_parent_folder_mismatch");
      next = item.parent_id;
    }
  };
  if (intent.root.kind === "folder" && intent.destination.folder_id !== null)
    blockers.add("folder_move_destination_folder_unsupported");
  inspectPlacement("folder", intent.destination.folder_id);
  inspectPlacement(intent.root.kind, intent.destination.parent_id);

  const schemas = snapshot.property_schemas ?? [];
  const values = (snapshot.property_values ?? []).filter((item) =>
    visibleIds.has(item.document_id),
  );
  const sourceSchemaIds = new Set(values.map((item) => item.property_id));
  const sourceSchemas = schemas.filter((item) => sourceSchemaIds.has(item.id));
  if (
    sourceSchemas.length !== sourceSchemaIds.size ||
    sourceSchemas.some((item) => item.space_id !== source.id)
  )
    blockers.add("invalid_property_binding");
  for (const value of values) {
    const schema = schemas.find((item) => item.id === value.property_id);
    if (
      schema &&
      errorMessage(() => {
        validatePropertySchema(schema);
        validatePropertyValue(schema, value.value);
      }).length
    )
      blockers.add("invalid_property_value");
  }
  for (const value of values) {
    const schema = schemas.find((item) => item.id === value.property_id);
    if (schema?.property_type === "user" && value.value && value.value !== currentUserId)
      blockers.add("property_identity_requires_resolution");
    if (schema?.property_type === "relation" && value.value) {
      const page = docs.get(value.value);
      if (!available(page) || page.space_id !== source.id)
        blockers.add("property_identity_requires_resolution");
    }
  }
  const externalRelations: LocalMovePreview["inventory"]["external_relations"] = [];
  const relationTargets: VaultDocument[] = [];
  for (const value of snapshot.property_values ?? []) {
    if (
      schemas.find((schema) => schema.id === value.property_id)?.property_type !== "relation" ||
      !value.value
    )
      continue;
    const outgoing = documentIds.has(value.document_id) && !documentIds.has(value.value);
    const incoming = !documentIds.has(value.document_id) && documentIds.has(value.value);
    if (!incoming && !outgoing) continue;
    const owner = docs.get(value.document_id);
    const target = docs.get(value.value);
    if (!available(owner) || !available(target)) {
      blockers.add("relation_endpoint_unavailable");
      continue;
    }
    externalRelations.push({
      direction: incoming ? "incoming" : "outgoing",
      value,
    });
    relationTargets.push(owner, target);
    if (source.id !== destination.id) blockers.add("external_relations_require_resolution");
  }
  const destinationSchemas = schemas.filter((item) => item.space_id === destination.id);
  const properties = sourceSchemas.flatMap((schema) =>
    destinationSchemas.map((target): LocalMoveMappingCandidate => {
      const reasons =
        schema.property_type === target.property_type ? [] : ["property_type_mismatch"];
      reasons.push(
        ...errorMessage(() => validatePropertySchema(schema)),
        ...errorMessage(() => validatePropertySchema(target)),
      );
      for (const value of values.filter((item) => item.property_id === schema.id)) {
        reasons.push(...errorMessage(() => validatePropertyValue(target, value.value)));
        if (value.value && target.property_type === "user" && value.value !== currentUserId)
          reasons.push("user_identity_unavailable");
        if (value.value && target.property_type === "relation") {
          const page = docs.get(value.value);
          if (
            !available(page) ||
            (!documentIds.has(value.value) && page.space_id !== destination.id)
          )
            reasons.push("relation_target_unavailable_in_destination");
        }
      }
      return {
        source_id: schema.id,
        destination_id: target.id,
        compatible: reasons.length === 0,
        reasons: sorted([...new Set(reasons)]),
      };
    }),
  );
  const tagIds = new Set(
    (snapshot.doc_tags ?? [])
      .filter((item) => visibleIds.has(item.document_id))
      .map((item) => item.tag_id),
  );
  const tags = (snapshot.tags ?? []).filter((item) => tagIds.has(item.id));
  if (tags.length !== tagIds.size || tags.some((item) => item.space_id !== source.id))
    blockers.add("invalid_tag_binding");
  const workflows = (snapshot.document_workflows ?? []).filter((item) =>
    visibleIds.has(item.document_id),
  );
  const workflowIds = new Set(workflows.map((item) => item.id));
  const associatedEvents = (snapshot.document_workflow_events ?? []).filter(
    (item) => visibleIds.has(item.document_id) || workflowIds.has(item.document_workflow_id),
  );
  if (
    associatedEvents.some(
      (item) => !visibleIds.has(item.document_id) || !workflowIds.has(item.document_workflow_id),
    )
  )
    blockers.add("invalid_workflow_history_binding");
  const events = associatedEvents.filter(
    (item) => visibleIds.has(item.document_id) && workflowIds.has(item.document_workflow_id),
  );
  const workflowTemplateIds = new Set(workflows.map((item) => item.workflow_template_id));
  const sourceTemplates = (snapshot.workflow_templates ?? []).filter((item) =>
    workflowTemplateIds.has(item.id),
  );
  if (
    sourceTemplates.length !== workflowTemplateIds.size ||
    sourceTemplates.some((item) => item.space_id !== source.id)
  )
    blockers.add("invalid_workflow_binding");
  const destinationTemplates = (snapshot.workflow_templates ?? []).filter(
    (item) => item.space_id === destination.id,
  );
  const templateReasons = (template: VaultWorkflowTemplate): string[] => {
    const reasons: string[] = [];
    const ids = new Set(template.stages.map((stage) => stage.id));
    if (
      ids.size !== template.stages.length ||
      template.stages.length < 2 ||
      template.stages.length > 12 ||
      template.stages.some((stage) => !stage.id || !stage.name.trim())
    )
      reasons.push("invalid_workflow_stages");
    for (const approver of template.approvers) {
      if (approver.approver_type !== "user" || approver.approver_id !== currentUserId)
        reasons.push("approver_identity_unavailable");
      if (approver.stage_id && !ids.has(approver.stage_id))
        reasons.push("approver_stage_unavailable");
    }
    return reasons;
  };
  for (const template of sourceTemplates) {
    const ids = new Set(template.stages.map((stage) => stage.id));
    if (
      templateReasons(template).length ||
      workflows.some(
        (item) => item.workflow_template_id === template.id && !ids.has(item.current_stage_id),
      ) ||
      events.some(
        (event) =>
          workflows.find((item) => item.id === event.document_workflow_id)?.workflow_template_id ===
            template.id &&
          (!ids.has(event.to_stage_id) || (event.from_stage_id && !ids.has(event.from_stage_id))),
      )
    )
      blockers.add("workflow_metadata_requires_resolution");
  }
  const workflowCandidates = sourceTemplates.flatMap((template) =>
    destinationTemplates.map((target): LocalMoveMappingCandidate => {
      const reasons = [...templateReasons(template), ...templateReasons(target)];
      if (canonical(sorted(template.stages)) !== canonical(sorted(target.stages)))
        reasons.push("workflow_stage_identity_mismatch");
      const approvers = (item: VaultWorkflowTemplate) =>
        sorted(
          item.approvers.map(({ approver_type, approver_id, stage_id }) => ({
            approver_type,
            approver_id,
            stage_id,
          })),
        );
      if (canonical(approvers(template)) !== canonical(approvers(target)))
        reasons.push("workflow_approver_identity_mismatch");
      const ids = new Set(target.stages.map((stage) => stage.id));
      if (
        workflows.some(
          (item) => item.workflow_template_id === template.id && !ids.has(item.current_stage_id),
        ) ||
        events.some(
          (event) =>
            workflows.find((item) => item.id === event.document_workflow_id)
              ?.workflow_template_id === template.id &&
            (!ids.has(event.to_stage_id) || (event.from_stage_id && !ids.has(event.from_stage_id))),
        )
      )
        reasons.push("workflow_history_stage_unavailable");
      return {
        source_id: template.id,
        destination_id: target.id,
        compatible: reasons.length === 0,
        reasons: sorted([...new Set(reasons)]),
      };
    }),
  );
  const bodies = [
    ...visibleDocs.map((item) => item.content),
    ...movedDrafts.map((item) => item.content),
  ];
  const references = (asset: LocalVaultSnapshot["assets"][number], body: string) =>
    body.includes(`starlane://asset/${asset.id}`) || (!!asset.url && body.includes(asset.url));
  const assets = snapshot.assets.filter(
    (item) =>
      (item.document_id && visibleIds.has(item.document_id)) ||
      bodies.some((body) => references(item, body)) ||
      visibleDocs.some((doc) => doc.cover_image === item.url),
  );
  const assetReferences: LocalMovePreview["inventory"]["asset_references"] = [];
  for (const asset of assets) {
    for (const doc of snapshot.documents)
      if (references(asset, doc.content) || doc.cover_image === asset.url) {
        if (available(doc)) assetReferences.push({ asset_id: asset.id, document_id: doc.id });
        else blockers.add("asset_reference_unavailable");
      }
    for (const draft of drafts)
      if (references(asset, draft.content) || draft.cover_image === asset.url) {
        if (draft.owner_id === currentUserId && available(docs.get(draft.document_id)))
          assetReferences.push({
            asset_id: asset.id,
            document_id: draft.document_id,
            draft_id: draft.id,
          });
        else blockers.add("asset_reference_unavailable");
      }
  }
  for (const asset of assets) {
    if (!asset.space_id || (asset.space_id !== source.id && asset.space_id !== destination.id))
      warnings.add("asset_ownership_requires_review");
    if (asset.vault_path) warnings.add("asset_path_retained_file_relocation_requires_journal");
    if (
      snapshot.documents.some(
        (doc) =>
          !documentIds.has(doc.id) &&
          (references(asset, doc.content) || doc.cover_image === asset.url),
      ) ||
      drafts.some(
        (draft) => !documentIds.has(draft.document_id) && references(asset, draft.content),
      )
    )
      warnings.add("shared_asset_reference_retained");
  }
  if (bodies.some((body) => body.includes(`/spaces/${source.id}/`)))
    warnings.add("source_space_qualified_content_links_retained");
  const affectedIds = new Set([
    ...documentIds,
    ...folderIds,
    ...sourceSchemaIds,
    ...tagIds,
    ...workflowIds,
    ...workflowTemplateIds,
    ...assets.map((item) => item.id),
    ...movedDrafts.map((item) => item.id),
  ]);
  const payloadTouches = (value: unknown): boolean =>
    typeof value === "string"
      ? affectedIds.has(value)
      : Array.isArray(value)
        ? value.some(payloadTouches)
        : !!value &&
          typeof value === "object" &&
          Object.entries(value).some(
            ([key, item]) =>
              (/(^id$|_id$|_ids$)/.test(key) || (item !== null && typeof item === "object")) &&
              payloadTouches(item),
          );
  const queue = (snapshot.sync_state?.queued_operations ?? []).filter(
    (item) =>
      affectedIds.has(item.entity_id) ||
      item.entity_id.split(":").some((id) => affectedIds.has(id)) ||
      (item.entity_type === "space" && [source.id, destination.id].includes(item.entity_id)) ||
      payloadTouches(item.payload),
  );
  if (queue.length) blockers.add("pending_affected_writes_require_resolution");
  const queuedPrivate = (value: unknown): boolean => {
    if (typeof value === "string")
      return (
        (docs.has(value) && !available(docs.get(value))) ||
        drafts.some((item) => item.id === value && item.owner_id !== currentUserId)
      );
    if (Array.isArray(value)) return value.some(queuedPrivate);
    return !!value && typeof value === "object" && Object.values(value).some(queuedPrivate);
  };
  if (queue.some((item) => queuedPrivate(item.entity_id) || queuedPrivate(item.payload)))
    blockers.add("queued_participation_unavailable");
  const inventory: Omit<
    LocalMovePreview["inventory"],
    | "identifiers"
    | "destination_identifier_rows"
    | "maintenance"
    | "maintenance_catalogs"
    | "destination_maintenance_targets"
  > = {
    folders: sorted(snapshot.folders.filter((item) => folderIds.has(item.id))),
    documents: sorted(visibleDocs),
    drafts: sorted(movedDrafts),
    property_schemas: sorted(sourceSchemas),
    property_values: sorted(values),
    tags: sorted(tags),
    doc_tags: sorted((snapshot.doc_tags ?? []).filter((item) => visibleIds.has(item.document_id))),
    workflow_templates: templates(sourceTemplates),
    document_workflows: sorted(workflows),
    document_workflow_events: sorted(events),
    assets: sorted(assets),
    source_ancestors: {
      folders: sorted([...sourceAncestorFolders.values()]),
      documents: sorted([...sourceAncestorDocuments.values()]),
    },
    asset_references: sorted(assetReferences),
    search_index: sorted(
      (snapshot.search_index ?? []).filter((item) => visibleIds.has(item.document_id)),
    ),
    relation_schemas: sorted(
      schemas.filter((item) =>
        externalRelations.some((relation) => relation.value.property_id === item.id),
      ),
    ),
    external_relations: sorted(externalRelations),
    external_hierarchy: sorted(externalHierarchy),
    queued_operations: sorted(
      queue.map(({ id, entity_type, action, queued_at }) => ({
        id,
        entity_type,
        action,
        queued_at,
      })),
    ),
    destination: {
      space: destination,
      folders: sorted([...destinationFolders.values()]),
      documents: sorted([...destinationDocuments.values()]),
      property_schemas: sorted(destinationSchemas),
      tags: sorted((snapshot.tags ?? []).filter((item) => item.space_id === destination.id)),
      workflow_templates: templates(destinationTemplates),
    },
  };
  const destinationIdentifierRows = sorted(
    snapshot.documents
      .filter((doc) => doc.space_id === destination.id && !visibleIds.has(doc.id))
      .map((doc) => ({
        id: doc.id,
        human_identifier: normalizeHumanIdentifier(doc.human_identifier ?? null),
      })),
  );
  const identifiers = sorted(
    inventory.documents.map((doc) => ({
      document_id: doc.id,
      human_identifier: normalizeHumanIdentifier(doc.human_identifier ?? null),
      resolution_required:
        !!humanIdentifierKey(doc.human_identifier ?? null) &&
        (destinationIdentifierRows.some(
          (row) =>
            humanIdentifierKey(row.human_identifier) ===
            humanIdentifierKey(doc.human_identifier ?? null),
        ) ||
          inventory.documents.some(
            (other) =>
              other.id !== doc.id &&
              humanIdentifierKey(other.human_identifier ?? null) ===
                humanIdentifierKey(doc.human_identifier ?? null),
          )),
    })),
  );
  const maintenanceCatalogs = normalizeDocumentMaintenanceCatalogs(
    (snapshot as LocalVaultSnapshot & { maintenance_target_catalogs?: unknown })
      .maintenance_target_catalogs,
  );
  const destinationCatalogs = maintenanceCatalogs.filter(
    (catalog) => catalog.space_id === destination.id && catalog.account_id === currentUserId,
  );
  const maintenanceTargets =
    destinationCatalogs.length === 1 ? destinationCatalogs[0].targets : null;
  const maintenance = sorted(
    inventory.documents.map((doc) => {
      const pair = normalizeDocumentMaintenanceFields(
        doc as VaultDocument & { maintenance_user_id?: unknown; maintenance_group_id?: unknown },
      );
      const user = (pair.maintenance_user_id ?? null) as string | null;
      const group = (pair.maintenance_group_id ?? null) as string | null;
      return {
        document_id: doc.id,
        maintenance_user_id: user,
        maintenance_group_id: group,
        resolution_required:
          (!!user && !maintenanceTargets?.users.some((target) => target.id === user)) ||
          (!!group &&
            !maintenanceTargets?.groups.some(
              (target) =>
                target.id === group &&
                (target.managed_space_id === null || target.managed_space_id === destination.id),
            )),
      };
    }),
  );
  const identifierInventory = {
    ...inventory,
    identifiers,
    destination_identifier_rows: destinationIdentifierRows,
    maintenance,
    maintenance_catalogs: maintenanceCatalogs,
    destination_maintenance_targets: maintenanceTargets,
  };
  const candidates = {
    properties: sorted(properties),
    workflows: sorted(workflowCandidates),
  };
  const signature = canonical({
    intent,
    current_user_id: currentUserId,
    source,
    inventory: identifierInventory,
    mapping_candidates: candidates,
    queued_operations_internal: sorted(
      queue.map((item) =>
        [...blockers].some((code) => code.endsWith("_unavailable"))
          ? {
              id: item.id,
              entity_type: item.entity_type,
              action: item.action,
              queued_at: item.queued_at,
            }
          : item,
      ),
    ),
    relation_endpoints: sorted([
      ...new Map(relationTargets.map((item) => [item.id, item])).values(),
    ]),
    blockers: sorted([...blockers]),
    warnings: sorted([...warnings]),
  });
  return detached({
    intent,
    current_user_id: currentUserId,
    unavailable_participation: [
      "participating_page_unavailable",
      "participating_draft_unavailable",
      "relation_endpoint_unavailable",
      "hierarchy_endpoint_unavailable",
      "asset_reference_unavailable",
      "queued_participation_unavailable",
    ].some((code) => blockers.has(code)),
    internal_inventory_signature: signature,
    inventory: identifierInventory,
    mapping_candidates: candidates,
    blockers: sorted([...blockers]),
    warnings: sorted([...warnings]),
    commit_available: false as const,
  });
}

/** Pure choices only: a caller must build a fresh preview from its atomic latest snapshot.
 * This does not enable local relocation or turn an old signature into authority. */
export function resolveLocalMoveIdentifiers(
  preview: LocalMovePreview,
  decisions: { document_id: string; choice: ServerMoveIdentifierChoice }[] = [],
): { document_id: string; human_identifier: string | null }[] {
  if (preview.unavailable_participation) throw new Error("Move participation unavailable");
  const choices = new Map<string, ServerMoveIdentifierChoice>();
  for (const decision of decisions) {
    if (
      choices.has(decision.document_id) ||
      !preview.inventory.identifiers.some((p) => p.document_id === decision.document_id)
    )
      throw new Error("Invalid move identifier decisions");
    choices.set(decision.document_id, decision.choice);
  }
  const keys = new Set(
    preview.inventory.destination_identifier_rows
      .map((row) => humanIdentifierKey(row.human_identifier))
      .filter((key): key is string => key !== null),
  );
  return preview.inventory.identifiers.map((page) => {
    const choice = choices.get(page.document_id);
    if (page.resolution_required && (!choice || choice.kind === "preserve"))
      throw new Error("Resolve conflicting move identifiers explicitly");
    const value =
      choice?.kind === "clear"
        ? null
        : normalizeHumanIdentifier(
            choice?.kind === "rename" ? choice.value : page.human_identifier,
          );
    if (choice?.kind === "rename" && !value)
      throw new Error("Use the explicit clear identifier choice");
    const key = humanIdentifierKey(value);
    if (key && keys.has(key)) throw new Error("Move identifier conflicts in destination");
    if (key) keys.add(key);
    return { document_id: page.document_id, human_identifier: value };
  });
}

/** Data-only decisions. Recompute the full preview synchronously under canonical CAS before any writer consumes them. */
export function resolveLocalMoveMaintenance(
  preview: LocalMovePreview,
  decisions: { document_id: string; choice: ServerMoveMaintenanceChoice }[] = [],
): {
  document_id: string;
  maintenance_user_id: string | null;
  maintenance_group_id: string | null;
}[] {
  if (preview.unavailable_participation) throw new Error("Move participation is unavailable");
  const selected = new Map<string, ServerMoveMaintenanceChoice>();
  for (const decision of decisions) {
    if (
      selected.has(decision.document_id) ||
      !preview.inventory.maintenance.some((p) => p.document_id === decision.document_id)
    )
      throw new Error("Invalid move maintenance decisions");
    selected.set(decision.document_id, decision.choice);
  }
  const targets = preview.inventory.destination_maintenance_targets;
  return preview.inventory.maintenance.map((page) => {
    const choice = selected.get(page.document_id);
    if (!choice || choice.kind === "preserve") {
      if (page.resolution_required)
        throw new Error("Map or clear unresolved maintenance assignment");
      return {
        document_id: page.document_id,
        maintenance_user_id: page.maintenance_user_id,
        maintenance_group_id: page.maintenance_group_id,
      };
    }
    const source = preview.inventory.documents.find((p) => p.id === page.document_id)!;
    if (source.locked || source.archived_at || source.deleted_at)
      throw new Error("Maintenance assignment cannot be edited");
    if (choice.kind === "user" && !targets?.users.some((u) => u.id === choice.user_id))
      throw new Error("Select a current catalog person");
    if (
      choice.kind === "group" &&
      !targets?.groups.some(
        (g) =>
          g.id === choice.group_id &&
          (g.managed_space_id === null ||
            g.managed_space_id === preview.intent.destination.space_id),
      )
    )
      throw new Error("Select a current catalog team");
    return {
      document_id: page.document_id,
      maintenance_user_id: choice.kind === "user" ? choice.user_id : null,
      maintenance_group_id: choice.kind === "group" ? choice.group_id : null,
    };
  });
}
