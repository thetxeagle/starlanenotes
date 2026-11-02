/** Server-owned reviewed relocation wire. Local inventories are never server authority. */
export type MoveMetadataFamily = "tag" | "property" | "workflow";
export interface ServerMoveIntent {
  root: { kind: "document" | "folder"; id: string };
  destination: {
    space_id: string;
    folder_id: string | null;
    parent_id: string | null;
  };
}
export interface MoveChoiceAssignment {
  id: string;
  document_id: string;
  value: string | null;
}
export interface MoveDefinitionReview {
  kind: MoveMetadataFamily;
  source_definition: Record<string, unknown>;
  assignment_count: number;
  assignments: MoveChoiceAssignment[];
  create_allowed: boolean;
  resolution_candidates: {
    id: string;
    name: string;
    compatibility: "requires_explicit_relation_targets";
    choice_details: Record<string, unknown>;
  }[];
  candidates: {
    id: string;
    name: string;
    compatibility: string;
    choice_details: Record<string, unknown>;
  }[];
  decision_required: boolean;
  relation_scope_requires_resolution: boolean;
}
export interface MoveRelationReview {
  assignment_id: string;
  property_id: string;
  from_document_id: string;
  to_document_id: string;
  crosses_subtree: boolean;
}
export interface ServerMoveViewerGrantReview {
  grant_id: string;
  document_id: string;
  revision: string;
  role: "viewer";
  scope: "page";
  recipient: { available: boolean; display_name: string | null };
  /** Point-in-time server hints; never authority to commit. */
  preserve_allowed: boolean;
  remove_allowed: boolean;
}
export type ServerMoveViewerGrantChoice = { kind: "preserve" } | { kind: "remove"; confirm: true };
export interface ServerMoveViewerGrantDecision {
  grant_id: string;
  revision: string;
  choice: ServerMoveViewerGrantChoice;
}
export interface ServerMoveInventory {
  viewer_grants?: ServerMoveViewerGrantReview[];
  pages?: {
    id: string;
    title: string;
    space_id: string;
    folder_id: string | null;
    parent_id: string | null;
    published: boolean;
    public_link_enabled: boolean;
    human_identifier?: string | null;
    identifier_resolution_required?: boolean;
    identifier_edit_allowed?: boolean;
    maintenance_user_id?: string | null;
    maintenance_group_id?: string | null;
    maintenance_resolution_required?: boolean;
    maintenance_edit_allowed?: boolean;
  }[];
  maintenance_targets?: {
    users: { id: string; name: string; active: true }[];
    groups: { id: string; name: string; managed_space_id: string | null }[];
    groups_available: boolean;
  };
  folders?: Record<string, unknown>[];
  metadata?: MoveDefinitionReview[];
  relations?: MoveRelationReview[];
  attachments?: number;
  drawings?: number;
  history_versions?: number;
  workflow_history_events?: number;
  private_drafts_preserved?: boolean;
  relation_targets?: { id: string; title: string }[];
  relation_targets_complete?: boolean;
  relation_targets_unavailable_reason?: string | null;
}
export interface ServerMovePreview {
  preview_id: string;
  expires_at: string;
  fingerprint: string;
  stale: boolean;
  inventory: ServerMoveInventory;
  access_consequences: {
    space_audience_gained: number;
    space_audience_lost: number;
    public_links: string;
    private_drafts: string;
    permission_model: string;
  };
  blockers: string[];
  commit_available: boolean;
  commit_unavailable_reason: string;
}
export type ServerMetadataChoice =
  | {
      kind: "map";
      destination_id: string;
      values: Record<string, string>;
      stages: Record<string, string>;
    }
  | {
      kind: "create";
      definition: Record<string, unknown>;
      values: Record<string, string>;
      stages: Record<string, string>;
    }
  | { kind: "preserve" }
  | { kind: "remove"; confirm: true };
export type ServerIncidentChoice =
  | { kind: "map"; target_id: string }
  | { kind: "preserve" }
  | { kind: "remove"; confirm: true };
export type ServerMoveIdentifierChoice =
  | { kind: "preserve" }
  | { kind: "rename"; value: string }
  | { kind: "clear" };
export type ServerMoveMaintenanceChoice =
  | { kind: "preserve" }
  | { kind: "clear" }
  | { kind: "user"; user_id: string }
  | { kind: "group"; group_id: string };
export interface ServerMoveCommit {
  viewer_grant_decisions?: ServerMoveViewerGrantDecision[];
  maintenance_decisions?: {
    document_id: string;
    choice: ServerMoveMaintenanceChoice;
  }[];
  identifier_decisions?: {
    document_id: string;
    choice: ServerMoveIdentifierChoice;
  }[];
  fingerprint: string;
  decisions: {
    family: MoveMetadataFamily;
    source_id: string;
    choice: ServerMetadataChoice;
  }[];
  incident_relations: { assignment_id: string; choice: ServerIncidentChoice }[];
  public_links: "preserve" | "disable";
  confirm_audience_change: boolean;
  confirm_draft_space_change: boolean;
}
export interface ServerMoveCommitResult {
  operation_id: string;
  committed: true;
  historical_receipt?: boolean;
  [key: string]: unknown;
}

/** Inactive evidence wire shape. Fresh POST transport is available; discoverable UI remains pending. */
export interface ServerMovePreservedEvidence {
  operation_id: string;
  space_id: string;
  document_id: string;
  kind: "immutable_metadata_evidence";
  editable: false;
  restoration_available: false;
  entries: {
    family: "tag" | "property" | "workflow" | "incident_relation" | "identifier" | "maintenance" | "page_viewer_grant";
    source_id: string;
    disposition: "map" | "create" | "preserve" | "remove" | "rename" | "clear";
    original: unknown;
    expected_active: unknown;
  }[];
}

/** Fresh page-bound evidence discovery; no total or restoration capability. */
export interface ServerMovePreservedIndex {
  schema_version: 1;
  space_id: string;
  document_id: string;
  checked_at: string;
  operations: { operation_id: string; created_at: string; kind: "immutable_metadata_evidence" }[];
  next_cursor: string | null;
}
