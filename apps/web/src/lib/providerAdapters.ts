import { requireNoPendingGoogleReviewedMoves } from "./localVault";
import { parseDocumentMaintenanceIntent, normalizeDocumentMaintenanceFields } from "@/lib/documentMaintenance";
import { parseDocumentIdentifierIntent, normalizeHumanIdentifier } from "@/lib/humanIdentifier";
import type { SyncProviderKind } from "@/lib/storageProviders";
import {
  createConnectedProviderSession,
  createDisconnectedProviderSession,
  createProviderNotConnectedError,
  readProviderDefaultSyncValidationEvidence,
  readProviderRemoteSyncEnablement,
  readProviderSession,
  summarizeProviderSession,
  type ProviderDefaultSyncValidationEvidenceRecord,
  type ProviderRemoteSyncEnablementRecord,
  type ProviderSessionSummary,
  type ProviderSessionRecord,
  writeProviderRemoteSyncEnablement,
  writeProviderSession,
} from "@/lib/providerSession";

const PROVIDER_REMOTE_IMPORT_MAX_DOCUMENT_CONTENT_BYTES = 5 * 1024 * 1024;

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

export interface ProviderApiResponse {
  ok: boolean;
  status: number;
  statusText?: string;
  json?: () => Promise<unknown>;
  text?: () => Promise<string>;
}

export interface ProviderApiTransport {
  fetch: (input: string, init: RequestInit) => Promise<ProviderApiResponse>;
}

export interface ProviderTransportOptions {
  nativeTransport?: TauriCoreModule;
  apiTransport?: ProviderApiTransport;
  accessToken?: string;
  session?: ProviderSessionRecord;
  providerRootId?: string;
  providerSyncPayload?: unknown;
}

export type ProviderCapability =
  | "connect"
  | "disconnect"
  | "pull"
  | "push"
  | "conflict-resolution";

export interface ProviderSyncResult {
  synced_at: string;
  pulled: number;
  pushed: number;
  conflicts: number;
  remote_mutation_envelope?: ProviderRemoteMutationEnvelope;
  remote_mutation_attempt?: ProviderRemoteMutationAttempt;
}

export interface ProviderRemoteContract {
  status: "not_applicable" | "contract_only" | "enabled";
  root_strategy: string;
  auth_model: string;
  required_scopes: string[];
  write_model: string;
  conflict_model: string;
}

export type ProviderRemoteMutationStatus = "not_applicable" | "dry_run_only" | "ready" | "blocked";

export interface ProviderRemoteMutationPlan {
  provider: SyncProviderKind;
  direction: "pull" | "push";
  status: ProviderRemoteMutationStatus;
  remote_mutation_enabled: boolean;
  dry_run: boolean;
  root_strategy: string;
  write_model: string;
  conflict_model: string;
  required_scopes: string[];
  blockers: string[];
}

export interface ProviderRemoteEnablementPlan {
  provider: SyncProviderKind;
  status: "not_applicable" | "ready_to_enable" | "blocked";
  transport_kind: "none" | "provider-api" | "provider-file";
  enablement_boundary: "none" | "explicit_manual_provider_session";
  default_sync_enabled: false;
  required_scopes: string[];
  blockers: string[];
  steps: string[];
}

export interface ProviderDefaultSyncPolicy {
  provider: SyncProviderKind;
  status: "not_applicable" | "manual_only" | "eligible_after_explicit_enablement";
  default_sync_enabled: false;
  automatic_enablement_allowed: false;
  policy_boundary: "none" | "explicit_manual_provider_session";
  readiness_gates: Array<{
    gate: string;
    passed: boolean;
    detail: string;
  }>;
  blockers: string[];
  required_user_actions: string[];
  validation_evidence: {
    production_oauth_verified: boolean;
    device_background_wake_verified: boolean;
    production_oauth_verified_at?: string;
    device_background_wake_verified_at?: string;
    notes: string[];
  };
}

export interface ProviderRemoteMutationEnvelope {
  schema_version: 1;
  provider: SyncProviderKind;
  direction: "pull" | "push";
  mutation_id: string;
  mutation_kind: "provider-sync-pull" | "provider-sync-push";
  payload_format: "starlane-provider-sync-manifest+json";
  target_root_strategy: string;
  write_model: string;
  conflict_model: string;
  required_scopes: string[];
  remote_mutation_enabled: boolean;
  dry_run: boolean;
  blockers: string[];
  created_at: string;
}

export interface ProviderRemoteSyncManifest {
  schema_version: 1;
  manifest_kind: "starlane-provider-sync-manifest";
  envelope: ProviderRemoteMutationEnvelope;
  replay_manifest?: unknown;
  import_ready: boolean;
  import_blockers: string[];
  created_at: string;
}

export interface ProviderRemoteImportPreflight {
  schema_version: 1;
  provider: SyncProviderKind;
  status: "ready_to_review" | "blocked";
  import_ready: boolean;
  replay_manifest_present: boolean;
  operation_count: number;
  ready_operation_count?: number;
  missing_payload_count?: number;
  blockers: string[];
}

export interface ProviderRemoteImportDryRunOperation {
  operation_id: string;
  entity_type: string;
  entity_id: string;
  action: string;
  status: "would_apply" | "conflict" | "failed" | "unsupported";
  message: string;
  payload?: unknown;
}

export interface ProviderRemoteImportDryRun {
  schema_version: 1;
  provider: SyncProviderKind;
  status: "blocked";
  import_ready: false;
  operation_count: number;
  would_apply_count: number;
  conflict_count: number;
  failed_count: number;
  unsupported_count: number;
  blockers: string[];
  operations: ProviderRemoteImportDryRunOperation[];
}

export interface ProviderRemoteImportLocalQueuedOperationIdentity {
  id: string;
  entity_type: string;
  entity_id: string;
  action: string;
  payload?: unknown;
}

export interface ProviderRemoteImportDryRunOptions {
  /**
   * Whole-space deletion is intentionally excluded from ordinary provider pulls.
   * A recovery UI must opt in only after a destructive confirmation.
   */
  allowDestructiveSpaceRecovery?: boolean;
}

export interface ProviderRemoteImportApplyPlanOperation {
  operation_id: string;
  entity_type: string;
  entity_id: string;
  action: string;
  source_status: ProviderRemoteImportDryRunOperation["status"];
  apply_status: "selected" | "skipped_conflict" | "skipped_failed" | "skipped_unsupported";
  message: string;
  payload?: unknown;
}

export interface ProviderRemoteImportApplyPlan {
  schema_version: 1;
  provider: SyncProviderKind;
  status: "blocked";
  apply_ready: false;
  selected_operation_count: number;
  skipped_operation_count: number;
  blockers: string[];
  operations: ProviderRemoteImportApplyPlanOperation[];
}

export interface ProviderRemoteImportApplyExecutionOperationResult {
  operation_id: string;
  entity_type: string;
  entity_id: string;
  action: string;
  status: "applied" | "skipped" | "failed" | "blocked";
  message: string;
}

export interface ProviderRemoteImportApplyExecutionResult {
  schema_version: 1;
  provider: SyncProviderKind;
  status: "blocked" | "completed";
  applied: boolean;
  attempted_operation_count: number;
  applied_operation_count: number;
  skipped_operation_count: number;
  failed_operation_count: number;
  blocked_operation_count: number;
  blockers: string[];
  executed_at: string;
  results: ProviderRemoteImportApplyExecutionOperationResult[];
}

export interface ProviderRemoteImportMergeExecutor {
  applyOperation(
    operation: ProviderRemoteImportApplyPlanOperation,
  ): Promise<
    Omit<
      ProviderRemoteImportApplyExecutionOperationResult,
      "operation_id" | "entity_type" | "entity_id" | "action"
    >
  >;
}

export interface ProviderRemoteImportApplyExecutionOptions {
  allowLocalMutation?: boolean;
  mergeExecutor?: ProviderRemoteImportMergeExecutor;
  executedAt?: string;
}

export type ProviderRemoteWriteRequestKind =
  | "none"
  | "google-drive-file-download"
  | "google-drive-file-upsert"
  | "icloud-drive-file-upsert";

export interface ProviderRemoteWriteRequestSpec {
  schema_version: 1;
  provider: SyncProviderKind;
  direction: "pull" | "push";
  mutation_id: string;
  request_kind: ProviderRemoteWriteRequestKind;
  target_root_strategy: string;
  payload_format: ProviderRemoteMutationEnvelope["payload_format"];
  method?: "GET" | "POST" | "PATCH" | "PUT";
  endpoint?: string;
  native_command?: string;
  remote_path?: string;
  required_scopes: string[];
  idempotency_key: string;
}

export type ProviderRemoteMutationAttemptStatus = "dry_run" | "blocked" | "applied";

export interface ProviderRemotePullDiscovery {
  schema_version: 1;
  provider: "google-drive";
  status: "found" | "not_found" | "invalid_response";
  remote_path: string;
  import_ready: boolean;
  blockers: string[];
  file?: {
    id: string;
    name: string;
    modified_time?: string;
    size?: string;
    web_view_link?: string;
  };
}

export interface ProviderRemoteMutationAttempt {
  schema_version: 1;
  provider: SyncProviderKind;
  direction: "pull" | "push";
  mutation_id: string;
  status: ProviderRemoteMutationAttemptStatus;
  transport_kind: "contract-only" | "provider-api" | "provider-file";
  applied: boolean;
  blockers: string[];
  attempted_at: string;
  request_spec?: ProviderRemoteWriteRequestSpec;
  receipt?: Record<string, unknown>;
  pull_discovery?: ProviderRemotePullDiscovery;
  remote_manifest?: ProviderRemoteMutationEnvelope;
  provider_sync_manifest?: ProviderRemoteSyncManifest;
  import_preflight?: ProviderRemoteImportPreflight;
  import_dry_run?: ProviderRemoteImportDryRun;
  import_apply_plan?: ProviderRemoteImportApplyPlan;
}

export interface ProviderStatus {
  connected: boolean;
  last_synced_at?: string;
  session?: ProviderSessionSummary;
  remote_contract: ProviderRemoteContract;
  remote_sync_enablement: ProviderRemoteSyncEnablementRecord;
  remote_enablement: ProviderRemoteEnablementPlan;
  default_sync_policy: ProviderDefaultSyncPolicy;
  remote_mutation_plans: ProviderRemoteMutationPlan[];
}

export interface ProviderAdapter {
  kind: SyncProviderKind;
  label: string;
  capabilities: ProviderCapability[];
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  enableRemoteSync(): Promise<void>;
  disableRemoteSync(reason?: string): Promise<void>;
  getStatus(): Promise<ProviderStatus>;
  pull(transportOptions?: ProviderTransportOptions): Promise<ProviderSyncResult>;
  push(transportOptions?: ProviderTransportOptions): Promise<ProviderSyncResult>;
}

export function buildProviderRemoteMutationPlan(
  provider: SyncProviderKind,
  label: string,
  direction: "pull" | "push",
  contract: ProviderRemoteContract,
  session?: ProviderSessionSummary,
): ProviderRemoteMutationPlan {
  if (contract.status === "not_applicable") {
    return {
      provider,
      direction,
      status: "not_applicable",
      remote_mutation_enabled: false,
      dry_run: false,
      root_strategy: contract.root_strategy,
      write_model: contract.write_model,
      conflict_model: contract.conflict_model,
      required_scopes: contract.required_scopes,
      blockers: [],
    };
  }

  if (contract.status !== "enabled") {
    const simulated = session?.sync_mode === "simulated";
    return {
      provider,
      direction,
      status: simulated ? "dry_run_only" : "blocked",
      remote_mutation_enabled: false,
      dry_run: true,
      root_strategy: contract.root_strategy,
      write_model: contract.write_model,
      conflict_model: contract.conflict_model,
      required_scopes: contract.required_scopes,
      blockers: simulated
        ? []
        : [`${label} remote ${direction} mutation is contract-only and not enabled yet`],
    };
  }

  const blockers = session?.connected ? [] : [`${label} is not connected`];
  return {
    provider,
    direction,
    status: blockers.length === 0 ? "ready" : "blocked",
    remote_mutation_enabled: blockers.length === 0,
    dry_run: false,
    root_strategy: contract.root_strategy,
    write_model: contract.write_model,
    conflict_model: contract.conflict_model,
    required_scopes: contract.required_scopes,
    blockers,
  };
}

export function buildProviderRemoteEnablementPlan(
  provider: SyncProviderKind,
  label: string,
  contract: ProviderRemoteContract,
  session?: ProviderSessionSummary,
  now = new Date(),
): ProviderRemoteEnablementPlan {
  if (contract.status === "not_applicable") {
    return {
      provider,
      status: "not_applicable",
      transport_kind: "none",
      enablement_boundary: "none",
      default_sync_enabled: false,
      required_scopes: [],
      blockers: [],
      steps: [],
    };
  }

  const transportKind = provider === "icloud-drive" ? "provider-file" : "provider-api";
  const blockers: string[] = [];
  if (!session?.connected) {
    blockers.push(`${label} must be connected before remote provider sync can be enabled`);
  }
  if (session?.sync_mode !== "manual") {
    blockers.push(`${label} requires a real manual provider session before remote sync enablement`);
  }

  const grantedScopes = new Set(session?.scopes ?? []);
  const missingScopes = contract.required_scopes.filter((scope) => !grantedScopes.has(scope));
  if (missingScopes.length > 0) {
    blockers.push(`${label} remote sync missing scopes: ${missingScopes.join(", ")}`);
  }

  const expiresAt = Date.parse(session?.expires_at ?? "");
  if (session?.expires_at && (!Number.isFinite(expiresAt) || expiresAt <= now.getTime())) {
    blockers.push(`${label} remote sync requires an unexpired provider session`);
  }
  if (provider === "google-drive" && session?.refresh_token_ready !== true) {
    blockers.push("Google Drive remote sync requires a refresh token");
  }
  if (provider === "google-drive" && !session?.provider_root_id) {
    blockers.push("Google Drive remote sync requires a selected Drive root folder");
  }

  return {
    provider,
    status: blockers.length === 0 ? "ready_to_enable" : "blocked",
    transport_kind: transportKind,
    enablement_boundary: "explicit_manual_provider_session",
    default_sync_enabled: false,
    required_scopes: contract.required_scopes,
    blockers,
    steps: [
      "Confirm the manual provider session is connected and scoped for remote sync.",
      "Enable provider remote mutation only through an explicit user/admin action.",
      "Route provider writes through the existing local-first replay/provider transport boundary.",
      "Keep default provider sync disabled until the explicit enablement action is recorded.",
    ],
  };
}

export function buildProviderDefaultSyncPolicy(
  provider: SyncProviderKind,
  label: string,
  enablementPlan: ProviderRemoteEnablementPlan,
  enablement: ProviderRemoteSyncEnablementRecord,
  validationEvidence?: ProviderDefaultSyncValidationEvidenceRecord,
): ProviderDefaultSyncPolicy {
  const evidence = validationEvidence ?? {
    provider,
    production_oauth_verified: false,
    device_background_wake_verified: false,
    notes: [],
  };
  if (provider === "none" || enablementPlan.status === "not_applicable") {
    return {
      provider,
      status: "not_applicable",
      default_sync_enabled: false,
      automatic_enablement_allowed: false,
      policy_boundary: "none",
      readiness_gates: [],
      blockers: [],
      required_user_actions: [],
      validation_evidence: {
        production_oauth_verified: false,
        device_background_wake_verified: false,
        notes: [],
      },
    };
  }

  const blockers = [...enablementPlan.blockers];
  const manualSessionReady = enablementPlan.status === "ready_to_enable";
  if (!enablement.enabled) {
    blockers.push(`${label} remote sync must be explicitly enabled before default sync can be considered`);
  }
  if (!evidence.production_oauth_verified) {
    blockers.push(`${label} default sync requires production OAuth validation`);
  }
  if (!evidence.device_background_wake_verified) {
    blockers.push(`${label} default sync requires device-executed background wake validation`);
  }
  const validationNotes = [
    ...evidence.notes,
    evidence.production_oauth_verified
      ? null
      : `${label} production OAuth completion must be verified outside fixtures before default sync can be enabled.`,
    evidence.device_background_wake_verified
      ? null
      : `${label} OS-delivered native background wake execution must be recorded on device before default sync can be enabled.`,
  ].filter((note): note is string => Boolean(note));
  const readinessGates = [
    {
      gate: "manual_provider_session",
      passed: manualSessionReady,
      detail: manualSessionReady
        ? `${label} has a connected manual session with required scopes/root metadata`
        : `${label} manual session is not ready for provider transport`,
    },
    {
      gate: "explicit_remote_enablement",
      passed: enablement.enabled,
      detail: enablement.enabled
        ? `${label} remote sync was explicitly enabled`
        : `${label} remote sync has not been explicitly enabled`,
    },
    {
      gate: "background_transport_wake",
      passed: enablement.enabled,
      detail: enablement.enabled
        ? `${label} can be attempted by the Tauri local background/lifecycle scheduler`
        : `${label} background provider attempts wait for explicit remote enablement`,
    },
    {
      gate: "production_oauth_validation",
      passed: evidence.production_oauth_verified,
      detail: evidence.production_oauth_verified
        ? `${label} production OAuth validation was recorded`
        : "No production OAuth validation has been recorded",
    },
    {
      gate: "device_executed_wake_validation",
      passed: evidence.device_background_wake_verified,
      detail: evidence.device_background_wake_verified
        ? `${label} OS-delivered device background wake validation was recorded`
        : "No OS-delivered device background wake validation has been recorded",
    },
  ];

  return {
    provider,
    status:
      manualSessionReady && enablement.enabled
        ? "eligible_after_explicit_enablement"
        : "manual_only",
    default_sync_enabled: false,
    automatic_enablement_allowed: false,
    policy_boundary: "explicit_manual_provider_session",
    readiness_gates: readinessGates,
    blockers,
    required_user_actions: [
      "Complete a real manual provider session with the required scopes and root metadata.",
      "Resolve queued replay blockers and pending conflicts before remote provider sync.",
      "Use the explicit remote sync enablement control before background provider pushes run.",
      "Record OS-delivered native background wake validation before allowing default provider sync.",
    ],
    validation_evidence: {
      production_oauth_verified: evidence.production_oauth_verified,
      device_background_wake_verified: evidence.device_background_wake_verified,
      production_oauth_verified_at: evidence.production_oauth_verified_at,
      device_background_wake_verified_at: evidence.device_background_wake_verified_at,
      notes: validationNotes,
    },
  };
}

export function buildProviderRemoteMutationEnvelope(
  plan: ProviderRemoteMutationPlan,
  createdAt = new Date().toISOString(),
  mutationId = `${plan.provider}-${plan.direction}-${createdAt}`,
): ProviderRemoteMutationEnvelope {
  return {
    schema_version: 1,
    provider: plan.provider,
    direction: plan.direction,
    mutation_id: mutationId,
    mutation_kind: plan.direction === "pull" ? "provider-sync-pull" : "provider-sync-push",
    payload_format: "starlane-provider-sync-manifest+json",
    target_root_strategy: plan.root_strategy,
    write_model: plan.write_model,
    conflict_model: plan.conflict_model,
    required_scopes: plan.required_scopes,
    remote_mutation_enabled: plan.remote_mutation_enabled,
    dry_run: plan.dry_run,
    blockers: plan.blockers,
    created_at: createdAt,
  };
}

export function buildProviderRemoteWriteRequestSpec(
  envelope: ProviderRemoteMutationEnvelope,
): ProviderRemoteWriteRequestSpec {
  const base = {
    schema_version: 1 as const,
    provider: envelope.provider,
    direction: envelope.direction,
    mutation_id: envelope.mutation_id,
    target_root_strategy: envelope.target_root_strategy,
    payload_format: envelope.payload_format,
    required_scopes: envelope.required_scopes,
    idempotency_key: envelope.mutation_id,
  };

  if (envelope.provider === "google-drive") {
    return {
      ...base,
      request_kind:
        envelope.direction === "pull"
          ? "google-drive-file-download"
          : "google-drive-file-upsert",
      method: envelope.direction === "pull" ? "GET" : "POST",
      endpoint:
        envelope.direction === "pull"
          ? "https://www.googleapis.com/drive/v3/files"
          : "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart",
      remote_path: "Starlane Notes/provider-sync-manifest.json",
    };
  }

  if (envelope.provider === "icloud-drive") {
    return {
      ...base,
      request_kind: "icloud-drive-file-upsert",
      native_command: "write_icloud_provider_sync_manifest",
      remote_path: "Starlane Notes/provider-sync-manifest.json",
    };
  }

  return {
    ...base,
    request_kind: "none",
  };
}

export function executeProviderRemoteMutationEnvelope(
  envelope: ProviderRemoteMutationEnvelope,
  transportKind: ProviderRemoteMutationAttempt["transport_kind"],
  attemptedAt = new Date().toISOString(),
): ProviderRemoteMutationAttempt {
  const requestSpec = buildProviderRemoteWriteRequestSpec(envelope);
  if (envelope.blockers.length > 0) {
    return {
      schema_version: 1,
      provider: envelope.provider,
      direction: envelope.direction,
      mutation_id: envelope.mutation_id,
      status: "blocked",
      transport_kind: transportKind,
      applied: false,
      blockers: envelope.blockers,
      attempted_at: attemptedAt,
      request_spec: requestSpec,
    };
  }

  if (envelope.dry_run || !envelope.remote_mutation_enabled) {
    return {
      schema_version: 1,
      provider: envelope.provider,
      direction: envelope.direction,
      mutation_id: envelope.mutation_id,
      status: "dry_run",
      transport_kind: transportKind,
      applied: false,
      blockers: [],
      attempted_at: attemptedAt,
      request_spec: requestSpec,
    };
  }

  return {
    schema_version: 1,
    provider: envelope.provider,
    direction: envelope.direction,
    mutation_id: envelope.mutation_id,
    status: "blocked",
    transport_kind: transportKind,
    applied: false,
    blockers: [`${envelope.provider} remote mutation transport is not configured yet`],
    attempted_at: attemptedAt,
    request_spec: requestSpec,
  };
}

function canUseTauriProviderTransport(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function invokeTauriProviderTransport<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriCoreModule })
    .__TAURI_INTERNALS__;
  if (internals?.invoke) return internals.invoke<T>(cmd, args);

  const coreModulePath = "@tauri-apps/api/core";
  const coreMod = (await import(/* @vite-ignore */ coreModulePath)) as unknown as TauriCoreModule;
  return coreMod.invoke<T>(cmd, args);
}

function normalizeProviderTransportOptions(
  options?: TauriCoreModule | ProviderTransportOptions,
): ProviderTransportOptions {
  if (!options) return {};
  if ("invoke" in options) return { nativeTransport: options };
  return options;
}

export function buildProviderRemoteSyncManifest(
  envelope: ProviderRemoteMutationEnvelope,
  replayManifest?: unknown,
): ProviderRemoteSyncManifest {
  return {
    schema_version: 1,
    manifest_kind: "starlane-provider-sync-manifest",
    envelope,
    replay_manifest: replayManifest,
    import_ready: true,
    import_blockers: [],
    created_at: new Date().toISOString(),
  };
}

function googleDriveMultipartBody(
  envelope: ProviderRemoteMutationEnvelope,
  requestSpec: ProviderRemoteWriteRequestSpec,
  providerRootId?: string,
  providerSyncPayload?: unknown,
): { contentType: string; body: string } {
  const boundary = `starlane-provider-sync-${envelope.mutation_id.replace(/[^a-z0-9_-]/gi, "-")}`;
  const metadata = {
    name: "provider-sync-manifest.json",
    mimeType: "application/json",
    ...(providerRootId ? { parents: [providerRootId] } : {}),
    appProperties: {
      starlaneMutationId: envelope.mutation_id,
      starlaneDirection: envelope.direction,
      starlanePayloadFormat: envelope.payload_format,
      starlaneRemotePath: requestSpec.remote_path ?? "",
    },
  };
  const manifest = JSON.stringify(
    buildProviderRemoteSyncManifest(envelope, providerSyncPayload),
    null,
    2,
  );
  const body = [
    `--${boundary}`,
    "Content-Type: application/json; charset=UTF-8",
    "",
    JSON.stringify(metadata),
    `--${boundary}`,
    "Content-Type: application/json; charset=UTF-8",
    "",
    manifest,
    `--${boundary}--`,
    "",
  ].join("\r\n");

  return {
    contentType: `multipart/related; boundary=${boundary}`,
    body,
  };
}

function googleDrivePullEndpoint(
  requestSpec: ProviderRemoteWriteRequestSpec,
  providerRootId?: string,
): string {
  const endpoint = requestSpec.endpoint ?? "https://www.googleapis.com/drive/v3/files";
  const url = new URL(endpoint);
  const query = [
    "name = 'provider-sync-manifest.json'",
    "trashed = false",
    providerRootId ? `'${providerRootId.replace(/'/g, "\\'")}' in parents` : null,
  ]
    .filter(Boolean)
    .join(" and ");
  url.searchParams.set("q", query);
  url.searchParams.set("spaces", "drive");
  url.searchParams.set(
    "fields",
    "files(id,name,mimeType,modifiedTime,size,appProperties,webViewLink)",
  );
  url.searchParams.set("pageSize", "1");
  return url.toString();
}

function googleDriveManifestContentEndpoint(fileId: string): string {
  return `https://www.googleapis.com/drive/v3/files/${encodeURIComponent(fileId)}?alt=media`;
}

async function providerApiReceipt(response: ProviderApiResponse): Promise<Record<string, unknown>> {
  if (response.json) {
    try {
      const json = await response.json();
      return typeof json === "object" && json !== null
        ? (json as Record<string, unknown>)
        : { value: json };
    } catch {
      // Fall through to text/status receipt.
    }
  }
  if (response.text) {
    try {
      const text = await response.text();
      return { status: response.status, status_text: response.statusText, body: text };
    } catch {
      // Fall through to status receipt.
    }
  }
  return { status: response.status, status_text: response.statusText };
}

function parseProviderRemoteMutationEnvelope(
  value: Record<string, unknown>,
): ProviderRemoteMutationEnvelope | null {
  const provider = value.provider;
  const direction = value.direction;
  const mutationKind = value.mutation_kind;
  const payloadFormat = value.payload_format;
  const requiredScopes = value.required_scopes;
  const blockers = value.blockers;

  if (value.schema_version !== 1) return null;
  if (provider !== "icloud-drive" && provider !== "google-drive" && provider !== "none") {
    return null;
  }
  if (direction !== "pull" && direction !== "push") return null;
  if (typeof value.mutation_id !== "string") return null;
  if (mutationKind !== "provider-sync-pull" && mutationKind !== "provider-sync-push") {
    return null;
  }
  if (payloadFormat !== "starlane-provider-sync-manifest+json") return null;
  if (typeof value.target_root_strategy !== "string") return null;
  if (typeof value.write_model !== "string") return null;
  if (typeof value.conflict_model !== "string") return null;
  if (!Array.isArray(requiredScopes) || !requiredScopes.every((scope) => typeof scope === "string")) {
    return null;
  }
  if (typeof value.remote_mutation_enabled !== "boolean") return null;
  if (typeof value.dry_run !== "boolean") return null;
  if (!Array.isArray(blockers) || !blockers.every((blocker) => typeof blocker === "string")) {
    return null;
  }
  if (typeof value.created_at !== "string") return null;

  return {
    schema_version: 1,
    provider,
    direction,
    mutation_id: value.mutation_id,
    mutation_kind: mutationKind,
    payload_format: payloadFormat,
    target_root_strategy: value.target_root_strategy,
    write_model: value.write_model,
    conflict_model: value.conflict_model,
    required_scopes: requiredScopes,
    remote_mutation_enabled: value.remote_mutation_enabled,
    dry_run: value.dry_run,
    blockers,
    created_at: value.created_at,
  };
}

function parseProviderRemoteSyncManifest(
  value: Record<string, unknown>,
): ProviderRemoteSyncManifest | null {
  if (value.schema_version !== 1) return null;
  if (value.manifest_kind !== "starlane-provider-sync-manifest") return null;
  if (typeof value.envelope !== "object" || value.envelope === null) return null;
  const envelope = parseProviderRemoteMutationEnvelope(value.envelope as Record<string, unknown>);
  if (!envelope) return null;
  if (typeof value.import_ready !== "boolean") return null;
  const blockers = value.import_blockers;
  if (!Array.isArray(blockers) || !blockers.every((blocker) => typeof blocker === "string")) {
    return null;
  }
  if (typeof value.created_at !== "string") return null;

  return {
    schema_version: 1,
    manifest_kind: "starlane-provider-sync-manifest",
    envelope,
    replay_manifest: value.replay_manifest,
    import_ready: value.import_ready,
    import_blockers: blockers,
    created_at: value.created_at,
  };
}

function countFromRecord(
  value: Record<string, unknown>,
  key: string,
): { value?: number; invalid: boolean } {
  const raw = value[key];
  if (raw === undefined || raw === null) return { invalid: false };
  if (typeof raw !== "number" || !Number.isFinite(raw) || raw < 0 || !Number.isInteger(raw)) {
    return { invalid: true };
  }
  return { value: raw, invalid: false };
}

function isProviderReplayPayloadStatus(value: unknown): value is "available" | "missing" {
  return value === "available" || value === "missing";
}

export function buildProviderRemoteImportPreflight(
  manifest: ProviderRemoteSyncManifest | undefined,
): ProviderRemoteImportPreflight {
  if (!manifest) {
    return {
      schema_version: 1,
      provider: "none",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: false,
      operation_count: 0,
      blockers: ["Provider import preflight requires a typed provider-sync manifest wrapper"],
    };
  }

  const replayManifest =
    typeof manifest.replay_manifest === "object" && manifest.replay_manifest !== null
      ? (manifest.replay_manifest as Record<string, unknown>)
      : null;
  const operationSummary =
    typeof replayManifest?.operation_summary === "object" &&
    replayManifest.operation_summary !== null
      ? (replayManifest.operation_summary as Record<string, unknown>)
      : null;
  const operationSummaryInvalid = replayManifest
    ? replayManifest.operation_summary !== undefined &&
      (typeof replayManifest.operation_summary !== "object" ||
        replayManifest.operation_summary === null ||
        Array.isArray(replayManifest.operation_summary))
    : false;
  const operationCountRecord = replayManifest
    ? countFromRecord(replayManifest, "operation_count")
    : { invalid: false };
  const operationCount = operationCountRecord.value ?? 0;
  const operationsRecordInvalid = Boolean(replayManifest) && !Array.isArray(replayManifest?.operations);
  const operations = Array.isArray(replayManifest?.operations) ? replayManifest.operations : [];
  const readyOperationCountRecord = operationSummary
    ? countFromRecord(operationSummary, "ready_operation_count")
    : { invalid: false };
  const missingPayloadCountRecord = operationSummary
    ? countFromRecord(operationSummary, "missing_payload_count")
    : { invalid: false };
  const readyOperationCount = readyOperationCountRecord.value;
  const missingPayloadCount = missingPayloadCountRecord.value;
  const payloadStatusInvalid =
    !operationsRecordInvalid &&
    operations.some(
      (operation) => {
        if (typeof operation !== "object" || operation === null) return false;
        const operationRecord = operation as Record<string, unknown>;
        const hasRequiredIdentity =
          typeof operationRecord.id === "string" &&
          operationRecord.id.trim() &&
          typeof operationRecord.entity_type === "string" &&
          operationRecord.entity_type.trim() &&
          typeof operationRecord.entity_id === "string" &&
          operationRecord.entity_id.trim() &&
          typeof operationRecord.action === "string" &&
          operationRecord.action.trim();
        return (
          Boolean(hasRequiredIdentity) &&
          !isProviderReplayPayloadStatus(operationRecord.payload_status)
        );
      },
    );
  const operationIdentityPadded =
    !operationsRecordInvalid &&
    operations.some((operation) => {
      if (typeof operation !== "object" || operation === null) return false;
      const operationRecord = operation as Record<string, unknown>;
      return ["id", "entity_type", "entity_id", "action"].some((key) => {
        const value = operationRecord[key];
        return typeof value === "string" && Boolean(value.trim()) && value !== value.trim();
      });
    });
  const operationIds = new Set<string>();
  const duplicateOperationIds =
    !operationsRecordInvalid &&
    operations.some((operation) => {
      if (typeof operation !== "object" || operation === null) return false;
      const operationRecord = operation as Record<string, unknown>;
      const id = operationRecord.id;
      if (typeof id !== "string" || !id.trim() || id !== id.trim()) return false;
      if (operationIds.has(id)) return true;
      operationIds.add(id);
      return false;
    });
  const blockers = [...manifest.import_blockers];
  if (!replayManifest) {
    blockers.unshift("Provider-sync manifest does not include a queued replay manifest payload");
  } else if (operationsRecordInvalid) {
    blockers.push("Provider-sync replay operations list is invalid");
  } else if (operationCountRecord.invalid) {
    blockers.push("Provider-sync replay operation count is invalid");
  } else if (operationCount !== operations.length) {
    blockers.push(
      `Provider-sync replay operation count mismatch: declared ${operationCount}, found ${operations.length}`,
    );
  }
  if (readyOperationCountRecord.invalid) {
    blockers.push("Provider-sync replay ready operation count is invalid");
  }
  if (missingPayloadCountRecord.invalid) {
    blockers.push("Provider-sync replay missing payload count is invalid");
  }
  if (operationSummaryInvalid) {
    blockers.push("Provider-sync replay operation summary is invalid");
  }
  if (payloadStatusInvalid) {
    blockers.push("Provider-sync replay payload status is invalid");
  }
  if (operationIdentityPadded) {
    blockers.push("Provider-sync replay operation identity fields are padded");
  }
  if (duplicateOperationIds) {
    blockers.push("Provider-sync replay operation IDs are duplicated");
  }
  if (
    readyOperationCount !== undefined &&
    missingPayloadCount !== undefined &&
    readyOperationCount + missingPayloadCount !== operationCount
  ) {
    blockers.push(
      `Provider-sync replay summary mismatch: ready ${readyOperationCount} plus missing ${missingPayloadCount} does not equal ${operationCount}`,
    );
  }
  if (readyOperationCount !== undefined) {
    const actualReadyOperationCount = operations.filter(
      (operation) =>
        typeof operation === "object" &&
        operation !== null &&
        (operation as Record<string, unknown>).payload_status === "available",
    ).length;
    if (readyOperationCount !== actualReadyOperationCount) {
      blockers.push(
        `Provider-sync replay ready count mismatch: summary ${readyOperationCount}, found ${actualReadyOperationCount}`,
      );
    }
  }
  if (missingPayloadCount !== undefined) {
    const actualMissingPayloadCount = operations.filter(
      (operation) =>
        typeof operation === "object" &&
        operation !== null &&
        (operation as Record<string, unknown>).payload_status === "missing",
    ).length;
    if (missingPayloadCount !== actualMissingPayloadCount) {
      blockers.push(
        `Provider-sync replay missing payload count mismatch: summary ${missingPayloadCount}, found ${actualMissingPayloadCount}`,
      );
    }
  }

  return {
    schema_version: 1,
    provider: manifest.envelope.provider,
    status: blockers.length === 0 ? "ready_to_review" : "blocked",
    import_ready: blockers.length === 0,
    replay_manifest_present: Boolean(replayManifest),
    operation_count: operationCount,
    ready_operation_count: readyOperationCount,
    missing_payload_count: missingPayloadCount,
    blockers,
  };
}

function replayOperationsFromManifest(manifest: ProviderRemoteSyncManifest): unknown[] {
  const replayManifest =
    typeof manifest.replay_manifest === "object" && manifest.replay_manifest !== null
      ? (manifest.replay_manifest as Record<string, unknown>)
      : null;
  return Array.isArray(replayManifest?.operations) ? replayManifest.operations : [];
}

export function isProviderRemoteImportOperationSupported(
  entityType: string,
  action: string,
): boolean {
  switch (entityType) {
    case "space":
      return action === "create" || action === "update";
    case "document":
      return (
        action === "create" ||
        action === "update" ||
        action === "update_content" ||
        action === "update_identifier" ||
        action === "update_maintenance" ||
        action === "delete"
      );
    case "folder":
    case "tag":
    case "template":
    case "workflow_template":
      return action === "create" || action === "update" || action === "delete";
    case "document_tag":
      return action === "add" || action === "remove";
    case "property_schema":
      return (
        action === "create" ||
        action === "update" ||
        action === "reorder" ||
        action === "delete"
      );
    case "document_property_values":
      return action === "set";
    case "asset":
      return action === "create" || action === "delete";
    case "document_workflow":
      return (
        action === "create" ||
        action === "update" ||
        action === "transition" ||
        action === "delete"
      );
    case "document_workflow_event":
      return (
        action === "enable" ||
        action === "transition" ||
        action === "disable" ||
        action === "delegate" ||
        action === "escalate"
      );
    default:
      return false;
  }
}

const SERVER_ONLY_PROVIDER_REMOTE_IMPORT_ENTITY_TYPES = new Set([
  "admin_group",
  "api_key",
  "audit_log",
  "auth_session",
  "comment",
  "document_reaction",
  "document_version",
  "group",
  "notification",
  "reaction",
  "scim_group",
  "scim_user",
  "space_group",
  "space_member",
  "user",
  "webhook",
  "webhook_delivery",
]);

export function isProviderRemoteImportServerOnlyOperation(
  entityType: string,
  action: string,
): boolean {
  return (
    SERVER_ONLY_PROVIDER_REMOTE_IMPORT_ENTITY_TYPES.has(entityType) ||
    (entityType === "document" && (action === "publish" || action === "unpublish"))
  );
}

export function isProviderRemoteImportRecoveryRequiredOperation(
  entityType: string,
  action: string,
): boolean {
  return entityType === "space" && action === "delete";
}

export function getProviderRemoteImportUnsupportedMessage(
  entityType: string,
  action: string,
): string {
  if (isProviderRemoteImportRecoveryRequiredOperation(entityType, action)) {
    return "Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow";
  }

  if (isProviderRemoteImportServerOnlyOperation(entityType, action)) {
    return `Remote replay operation ${entityType}:${action} is server-only and cannot be imported into a local vault`;
  }

  return `Remote replay operation ${entityType}:${action} is not supported by local provider import`;
}

function providerRemoteImportPayloadIdentityFailure(
  entityType: string,
  action: string,
  entityId: string,
  payload: unknown,
): string | null {
  if (!payload || typeof payload !== "object") return null;
  const payloadRecord = payload as Record<string, unknown>;
  if (entityType === "document_tag") {
    const documentId = payloadRecord.document_id;
    const tagId = payloadRecord.tag_id;
    if (typeof documentId === "string" && typeof tagId === "string") {
      const relationshipId = `${documentId}:${tagId}`;
      if (relationshipId !== entityId) {
        return `Remote replay operation ${entityType}:${action} payload relationship does not match operation entity ID`;
      }
    }
  }
  if (entityType === "document_property_values") {
    const documentId = payloadRecord.document_id;
    if (typeof documentId === "string" && documentId !== entityId) {
      return `Remote replay operation ${entityType}:${action} payload document does not match operation entity ID`;
    }
  }
  if (entityType === "property_schema" && action === "reorder") {
    const spaceId = payloadRecord.space_id;
    if (typeof spaceId === "string" && spaceId !== entityId) {
      return "Remote replay operation property_schema:reorder payload space does not match operation entity ID";
    }
  }
  const payloadId = (payload as { id?: unknown }).id;
  if (payloadId === undefined || payloadId === null) return null;
  if (typeof payloadId !== "string") {
    return `Remote replay operation ${entityType}:${action} payload ID is invalid`;
  }
  if (payloadId === entityId) return null;

  return `Remote replay operation ${entityType}:${action} payload ID does not match operation entity ID`;
}

function providerRemoteImportPayloadActionFailure(
  entityType: string,
  action: string,
  payload: unknown,
): string | null {
  if (entityType !== "document_workflow_event") return null;
  if (!payload || typeof payload !== "object") return null;
  const payloadAction = (payload as { action?: unknown }).action;
  if (typeof payloadAction !== "string" || payloadAction === action) return null;

  return `Remote replay operation ${entityType}:${action} payload action does not match operation action`;
}

function providerRemoteImportPayloadRequiredFailure(
  entityType: string,
  action: string,
  payload: unknown,
): string | null {
  if (Array.isArray(payload)) {
    return `Remote replay operation ${entityType}:${action} payload is not available for local import`;
  }
  if (action === "delete" && payload === undefined) return null;
  if (action === "delete" && payload === null) return null;
  if (payload && typeof payload === "object" && !Array.isArray(payload)) return null;

  return `Remote replay operation ${entityType}:${action} payload is not available for local import`;
}

function providerRemoteImportDeleteContextFailure(
  entityType: string,
  action: string,
  payload: unknown,
): string | null {
  if (action !== "delete" || payload === undefined || payload === null) return null;
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const payloadRecord = payload as Record<string, unknown>;

  if (
    payloadRecord.space_id !== undefined &&
    payloadRecord.space_id !== null &&
    typeof payloadRecord.space_id !== "string"
  ) {
    return `Remote replay operation ${entityType}:delete payload space is invalid`;
  }
  if (typeof payloadRecord.space_id === "string" && !payloadRecord.space_id.trim()) {
    return `Remote replay operation ${entityType}:delete payload space is invalid`;
  }

  const optionalMetadataFailure = providerRemoteImportOptionalStringFailure(payloadRecord, [
    {
      key: "deleted_by",
      label: `Remote replay operation ${entityType}:delete payload deleter is invalid`,
    },
    {
      key: "deleted_at",
      label: `Remote replay operation ${entityType}:delete payload deleted timestamp is invalid`,
    },
    {
      key: "updated_at",
      label: `Remote replay operation ${entityType}:delete payload updated timestamp is invalid`,
    },
  ]);
  if (optionalMetadataFailure) return optionalMetadataFailure;

  if (
    entityType === "asset" &&
    payloadRecord.document_id !== undefined &&
    payloadRecord.document_id !== null &&
    typeof payloadRecord.document_id !== "string"
  ) {
    return "Remote replay operation asset:delete payload document is invalid";
  }
  if (
    entityType === "asset" &&
    typeof payloadRecord.document_id === "string" &&
    !payloadRecord.document_id.trim()
  ) {
    return "Remote replay operation asset:delete payload document is invalid";
  }

  if (
    entityType === "document_workflow" &&
    payloadRecord.document_id !== undefined &&
    payloadRecord.document_id !== null &&
    typeof payloadRecord.document_id !== "string"
  ) {
    return "Remote replay operation document_workflow:delete payload document is invalid";
  }
  if (
    entityType === "document_workflow" &&
    typeof payloadRecord.document_id === "string" &&
    !payloadRecord.document_id.trim()
  ) {
    return "Remote replay operation document_workflow:delete payload document is invalid";
  }

  return null;
}

function providerRemoteImportDataUrlByteSize(url: string): number | null {
  const match = /^data:([^;,]+)?(;base64)?,(.*)$/i.exec(url);
  if (!match) return null;

  const isBase64 = Boolean(match[2]);
  const payload = match[3] ?? "";
  try {
    if (isBase64) {
      const binary = globalThis.atob(payload);
      return binary.length;
    }
    return new TextEncoder().encode(decodeURIComponent(payload)).length;
  } catch {
    return null;
  }
}

function providerRemoteImportTextByteSize(text: string): number {
  return new TextEncoder().encode(text).length;
}

function providerRemoteImportOptionalStringFailure(
  payloadRecord: Record<string, unknown>,
  fields: Array<{ key: string; label: string }>,
): string | null {
  for (const field of fields) {
    const value = payloadRecord[field.key];
    if (value !== undefined && value !== null && typeof value !== "string") {
      return field.label;
    }
  }
  return null;
}

function providerRemoteImportBlankStringFailure(
  payloadRecord: Record<string, unknown>,
  fields: Array<{ key: string; label: string }>,
): string | null {
  for (const field of fields) {
    const value = payloadRecord[field.key];
    if (typeof value === "string" && !value.trim()) {
      return field.label;
    }
  }
  return null;
}

export interface DocumentMetadataIntent {
  version: 1;
  fields: ("title" | "icon" | "cover_image" | "folder_id" | "parent_id")[];
  source: { page_updated_at: string; published_at: string | null };
  draft?: { mode: "own_latest" } | { mode: "exact"; id: string; updated_at: string; base_updated_at: string };
}
export function parseDocumentMetadataIntent(payload: Record<string, unknown>): DocumentMetadataIntent {
  for (const key of ["space_id", "title", "content", "updated_at", "created_at"]) {
    if (key in payload && typeof payload[key] !== "string") throw new Error(`Metadata ${key} must be a string when supplied`);
  }
  for (const key of ["folder_id", "parent_id", "icon", "cover_image"]) {
    if (key in payload && payload[key] !== null && typeof payload[key] !== "string") throw new Error(`Metadata ${key} must be a string or null`);
  }
  const raw = payload.metadata_intent;
  const object = (value: unknown): Record<string, unknown> => {
    if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Invalid document metadata intent");
    return value as Record<string, unknown>;
  };
  const intent = object(raw), source = object(intent.source);
  const exactKeys = (value: Record<string, unknown>, allowed: string[]) => {
    if (Object.keys(value).some((key) => !allowed.includes(key))) throw new Error("Unknown document metadata intent field");
  };
  exactKeys(intent, ["version", "fields", "source", "draft"]);
  exactKeys(source, ["page_updated_at", "published_at"]);
  if (intent.version !== 1 || !Array.isArray(intent.fields) || !intent.fields.length ||
      typeof source.page_updated_at !== "string" || !source.page_updated_at ||
      !(source.published_at === null || typeof source.published_at === "string" && source.published_at.length > 0))
    throw new Error("Invalid document metadata intent version/source/mask");
  const seen = new Set<string>();
  for (const field of intent.fields) {
    if (typeof field !== "string" || !["title", "icon", "cover_image", "folder_id", "parent_id"].includes(field) || seen.has(field) || !Object.prototype.hasOwnProperty.call(payload, field))
      throw new Error("Metadata intent requires unique known fields and explicit values");
    seen.add(field);
    const value = payload[field];
    if (typeof value !== "string" && !(field !== "title" && value === null)) throw new Error("Metadata intended field has an invalid value");
    if (field === "title" && !(value as string).trim()) throw new Error("Metadata title must not be empty");
  }
  if (intent.draft !== undefined) {
    if (![...seen].some((field) => ["title", "icon", "cover_image"].includes(field))) throw new Error("Draft context requires a presentation field");
    const draft = object(intent.draft);
    if (draft.mode === "own_latest") exactKeys(draft, ["mode"]);
    else if (draft.mode === "exact") {
      exactKeys(draft, ["mode", "id", "updated_at", "base_updated_at"]);
      if ([draft.id, draft.updated_at, draft.base_updated_at].some((value) => typeof value !== "string" || !value.trim())) throw new Error("Exact metadata draft source is invalid");
    } else throw new Error("Metadata draft choice is invalid");
  }
  if (typeof payload.space_id !== "string" || !payload.space_id.trim()) throw new Error("Metadata space is required");
  return intent as unknown as DocumentMetadataIntent;
}

/** Absent legacy fields retain defaults; supplied fields use the server replay wire contract. */
export function providerPropertySchemaFieldsFailure(
  payload: Record<string, unknown>,
): "type" | "sort order" | null {
  if (
    Object.prototype.hasOwnProperty.call(payload, "property_type") &&
    (typeof payload.property_type !== "string" || !payload.property_type.trim())
  )
    return "type";
  if (
    Object.prototype.hasOwnProperty.call(payload, "sort_order") &&
    (typeof payload.sort_order !== "number" ||
      !Number.isInteger(payload.sort_order) ||
      payload.sort_order < -2147483648 ||
      payload.sort_order > 2147483647)
  )
    return "sort order";
  return null;
}

function providerRemoteImportPayloadSemanticFailure(
  entityType: string,
  action: string,
  payload: unknown,
): string | null {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const payloadRecord = payload as Record<string, unknown>;

  if (entityType === "document" && action === "update_maintenance") {
    try { parseDocumentMaintenanceIntent(payloadRecord); return null; }
    catch (error) { return error instanceof Error ? error.message : "Invalid maintenance intent"; }
  }
  if (entityType === "document" && (action === "create" || action === "update")) {
    try { normalizeDocumentMaintenanceFields(payloadRecord); }
    catch (error) { return error instanceof Error ? error.message : "Invalid maintenance pair"; }
  }
  if (entityType === "document" && action === "update_identifier") {
    try { parseDocumentIdentifierIntent(payloadRecord); return null; }
    catch (error) { return error instanceof Error ? error.message : "Invalid identifier intent"; }
  }
  if (entityType === "document" && (action === "create" || action === "update") && "human_identifier" in payloadRecord) {
    try { normalizeHumanIdentifier(payloadRecord.human_identifier); }
    catch (error) { return error instanceof Error ? error.message : "Invalid page identifier"; }
  }
  if (entityType === "document" && action === "update" && "metadata_intent" in payloadRecord) {
    try { parseDocumentMetadataIntent(payloadRecord); return null; }
    catch (error) { return error instanceof Error ? error.message : "Invalid metadata intent"; }
  }
  if (entityType === "document" && (action === "create" || action === "update")) {
    if (typeof payloadRecord.space_id !== "string" || typeof payloadRecord.title !== "string") {
      return `Remote replay operation document:${action} payload is missing space or title`;
    }
    const blankContextFailure = providerRemoteImportBlankStringFailure(payloadRecord, [
      { key: "space_id", label: `Remote replay operation document:${action} payload space is invalid` },
    ]);
    if (blankContextFailure) return blankContextFailure;
    if (!payloadRecord.title.trim()) {
      return `Remote replay operation document:${action} payload is missing title`;
    }
    if (
      payloadRecord.folder_id !== undefined &&
      payloadRecord.folder_id !== null &&
      typeof payloadRecord.folder_id !== "string"
    ) {
      return `Remote replay operation document:${action} payload folder is invalid`;
    }
    const optionalMetadataFailure = providerRemoteImportOptionalStringFailure(payloadRecord, [
      { key: "icon", label: `Remote replay operation document:${action} payload icon is invalid` },
      {
        key: "cover_image",
        label: `Remote replay operation document:${action} payload cover image is invalid`,
      },
      {
        key: "created_by",
        label: `Remote replay operation document:${action} payload creator is invalid`,
      },
      {
        key: "last_updated_by",
        label: `Remote replay operation document:${action} payload updater is invalid`,
      },
      {
        key: "created_at",
        label: `Remote replay operation document:${action} payload created timestamp is invalid`,
      },
      {
        key: "updated_at",
        label: `Remote replay operation document:${action} payload updated timestamp is invalid`,
      },
    ]);
    if (optionalMetadataFailure) return optionalMetadataFailure;
    if (
      action === "create" &&
      typeof payloadRecord.content === "string" &&
      providerRemoteImportTextByteSize(payloadRecord.content) >
        PROVIDER_REMOTE_IMPORT_MAX_DOCUMENT_CONTENT_BYTES
    ) {
      return "Remote replay operation document:create payload content exceeds 5 MB limit";
    }
  }

  if (entityType === "document" && action === "update_content") {
    if (typeof payloadRecord.content !== "string") {
      return "Remote replay operation document:update_content payload is missing content";
    }
    if (
      providerRemoteImportTextByteSize(payloadRecord.content) >
      PROVIDER_REMOTE_IMPORT_MAX_DOCUMENT_CONTENT_BYTES
    ) {
      return "Remote replay operation document:update_content payload content exceeds 5 MB limit";
    }
    if (typeof payloadRecord.title === "string" && !payloadRecord.title.trim()) {
      return "Remote replay operation document:update_content payload is missing title";
    }
    if (
      payloadRecord.folder_id !== undefined &&
      payloadRecord.folder_id !== null &&
      typeof payloadRecord.folder_id !== "string"
    ) {
      return "Remote replay operation document:update_content payload folder is invalid";
    }
    if (
      payloadRecord.title !== undefined &&
      payloadRecord.title !== null &&
      typeof payloadRecord.title !== "string"
    ) {
      return "Remote replay operation document:update_content payload title is invalid";
    }
    const optionalMetadataFailure = providerRemoteImportOptionalStringFailure(payloadRecord, [
      {
        key: "icon",
        label: "Remote replay operation document:update_content payload icon is invalid",
      },
      {
        key: "cover_image",
        label: "Remote replay operation document:update_content payload cover image is invalid",
      },
      {
        key: "last_updated_by",
        label: "Remote replay operation document:update_content payload updater is invalid",
      },
      {
        key: "updated_at",
        label: "Remote replay operation document:update_content payload updated timestamp is invalid",
      },
    ]);
    if (optionalMetadataFailure) return optionalMetadataFailure;
  }

  if (entityType === "space" && (action === "create" || action === "update")) {
    if (typeof payloadRecord.name !== "string") {
      return `Remote replay operation space:${action} payload is missing name`;
    }
    if (!payloadRecord.name.trim()) {
      return `Remote replay operation space:${action} payload is missing space name`;
    }
    const optionalMetadataFailure = providerRemoteImportOptionalStringFailure(payloadRecord, [
      { key: "slug", label: `Remote replay operation space:${action} payload slug is invalid` },
      {
        key: "owner_username",
        label: `Remote replay operation space:${action} payload owner username is invalid`,
      },
      {
        key: "owner_display_name",
        label: `Remote replay operation space:${action} payload owner display name is invalid`,
      },
      {
        key: "created_by",
        label: `Remote replay operation space:${action} payload creator is invalid`,
      },
      {
        key: "created_at",
        label: `Remote replay operation space:${action} payload created timestamp is invalid`,
      },
      {
        key: "updated_at",
        label: `Remote replay operation space:${action} payload updated timestamp is invalid`,
      },
    ]);
    if (optionalMetadataFailure) return optionalMetadataFailure;
  }

  if (entityType === "folder" && (action === "create" || action === "update")) {
    if (typeof payloadRecord.space_id !== "string" || typeof payloadRecord.name !== "string") {
      return `Remote replay operation folder:${action} payload is missing space or name`;
    }
    const blankContextFailure = providerRemoteImportBlankStringFailure(payloadRecord, [
      { key: "space_id", label: `Remote replay operation folder:${action} payload space is invalid` },
    ]);
    if (blankContextFailure) return blankContextFailure;
    if (!payloadRecord.name.trim()) {
      return `Remote replay operation folder:${action} payload is missing folder name`;
    }
    if (
      payloadRecord.parent_id !== undefined &&
      payloadRecord.parent_id !== null &&
      typeof payloadRecord.parent_id !== "string"
    ) {
      return `Remote replay operation folder:${action} payload parent is invalid`;
    }
  }

  if (entityType === "tag" && (action === "create" || action === "update")) {
    if (typeof payloadRecord.space_id !== "string" || typeof payloadRecord.name !== "string") {
      return `Remote replay operation tag:${action} payload is missing space or name`;
    }
    const blankContextFailure = providerRemoteImportBlankStringFailure(payloadRecord, [
      { key: "space_id", label: `Remote replay operation tag:${action} payload space is invalid` },
    ]);
    if (blankContextFailure) return blankContextFailure;
    if (!payloadRecord.name.trim()) {
      return `Remote replay operation tag:${action} payload is missing tag name`;
    }
  }

  if (entityType === "template" && (action === "create" || action === "update")) {
    if (
      typeof payloadRecord.space_id !== "string" ||
      typeof payloadRecord.name !== "string" ||
      typeof payloadRecord.content !== "string"
    ) {
      return `Remote replay operation template:${action} payload is missing space, name, or content`;
    }
    const blankContextFailure = providerRemoteImportBlankStringFailure(payloadRecord, [
      { key: "space_id", label: `Remote replay operation template:${action} payload space is invalid` },
    ]);
    if (blankContextFailure) return blankContextFailure;
    if (!payloadRecord.name.trim()) {
      return `Remote replay operation template:${action} payload is missing template name`;
    }
    if (payloadRecord.name.trim().length > 100) {
      return `Remote replay operation template:${action} payload name is too long`;
    }
    if (payloadRecord.description !== undefined && typeof payloadRecord.description !== "string") {
      return `Remote replay operation template:${action} payload description is invalid`;
    }
    if (typeof payloadRecord.description === "string" && payloadRecord.description.length > 1000) {
      return `Remote replay operation template:${action} payload description is too long`;
    }
    const optionalMetadataFailure = providerRemoteImportOptionalStringFailure(payloadRecord, [
      {
        key: "created_by",
        label: `Remote replay operation template:${action} payload creator is invalid`,
      },
      {
        key: "created_at",
        label: `Remote replay operation template:${action} payload created timestamp is invalid`,
      },
      {
        key: "updated_at",
        label: `Remote replay operation template:${action} payload updated timestamp is invalid`,
      },
    ]);
    if (optionalMetadataFailure) return optionalMetadataFailure;
    if (
      payloadRecord.tags !== undefined &&
      payloadRecord.tags !== null &&
      (!Array.isArray(payloadRecord.tags) ||
        !payloadRecord.tags.every((tag) => typeof tag === "string"))
    ) {
      return `Remote replay operation template:${action} payload tags are invalid`;
    }
    if (
      providerRemoteImportTextByteSize(payloadRecord.content) >
      PROVIDER_REMOTE_IMPORT_MAX_DOCUMENT_CONTENT_BYTES
    ) {
      return `Remote replay operation template:${action} payload content exceeds 5 MB limit`;
    }
  }

  if (entityType === "workflow_template" && (action === "create" || action === "update")) {
    if (
      typeof payloadRecord.space_id !== "string" ||
      typeof payloadRecord.name !== "string" ||
      !Array.isArray(payloadRecord.stages)
    ) {
      return `Remote replay operation workflow_template:${action} payload is missing space, name, or stages`;
    }
    const blankContextFailure = providerRemoteImportBlankStringFailure(payloadRecord, [
      {
        key: "space_id",
        label: `Remote replay operation workflow_template:${action} payload space is invalid`,
      },
    ]);
    if (blankContextFailure) return blankContextFailure;
    const name = payloadRecord.name.trim();
    if (!name) {
      return `Remote replay operation workflow_template:${action} payload is missing workflow template name`;
    }
    if (name.length > 100) {
      return `Remote replay operation workflow_template:${action} payload name is too long`;
    }
    if (payloadRecord.description !== undefined && typeof payloadRecord.description !== "string") {
      return `Remote replay operation workflow_template:${action} payload description is invalid`;
    }
    if (typeof payloadRecord.description === "string" && payloadRecord.description.length > 1000) {
      return `Remote replay operation workflow_template:${action} payload description is too long`;
    }
    const optionalMetadataFailure = providerRemoteImportOptionalStringFailure(payloadRecord, [
      {
        key: "created_by",
        label: `Remote replay operation workflow_template:${action} payload creator is invalid`,
      },
      {
        key: "created_at",
        label: `Remote replay operation workflow_template:${action} payload created timestamp is invalid`,
      },
      {
        key: "updated_at",
        label: `Remote replay operation workflow_template:${action} payload updated timestamp is invalid`,
      },
    ]);
    if (optionalMetadataFailure) return optionalMetadataFailure;
    const stages = payloadRecord.stages;
    if (
      !stages.every(
        (stage) =>
          stage &&
          typeof stage === "object" &&
          !Array.isArray(stage) &&
          typeof (stage as Record<string, unknown>).name === "string",
      )
    ) {
      return `Remote replay operation workflow_template:${action} payload includes invalid stages`;
    }
    if (
      !stages.every((stage) => {
        const record = stage as Record<string, unknown>;
        const sortOrder = record.sort_order;
        return (
          (record.id === undefined || record.id === null || typeof record.id === "string") &&
          (sortOrder === undefined ||
            sortOrder === null ||
            (typeof sortOrder === "number" && Number.isFinite(sortOrder)))
        );
      })
    ) {
      return `Remote replay operation workflow_template:${action} payload includes invalid stage metadata`;
    }
    const normalizedStages = stages
      .map((stage, index) => {
        const record = stage as Record<string, unknown>;
        return {
          id: typeof record.id === "string" ? record.id.trim() : `stage-${index}`,
          name: (record.name as string).trim(),
        };
      })
      .filter((stage) => stage.name);
    if (normalizedStages.length < 2) {
      return `Remote replay operation workflow_template:${action} payload requires at least two stages`;
    }
    if (normalizedStages.length > 12) {
      return `Remote replay operation workflow_template:${action} payload includes too many stages`;
    }
    const stageIds = new Set(normalizedStages.map((stage) => stage.id));
    if (
      payloadRecord.approvers !== undefined &&
      payloadRecord.approvers !== null &&
      !Array.isArray(payloadRecord.approvers)
    ) {
      return `Remote replay operation workflow_template:${action} payload includes invalid approvers`;
    }
    const approvers = Array.isArray(payloadRecord.approvers) ? payloadRecord.approvers : [];
    for (const approver of approvers) {
      if (
        !approver ||
        typeof approver !== "object" ||
        Array.isArray(approver) ||
        ((approver as Record<string, unknown>).id !== undefined &&
          (approver as Record<string, unknown>).id !== null &&
          typeof (approver as Record<string, unknown>).id !== "string") ||
        typeof (approver as Record<string, unknown>).approver_type !== "string" ||
        typeof (approver as Record<string, unknown>).approver_id !== "string"
      ) {
        return `Remote replay operation workflow_template:${action} payload includes invalid approvers`;
      }
      const record = approver as Record<string, unknown>;
      const approverId = record.approver_id as string;
      if (!approverId.trim()) {
        return `Remote replay operation workflow_template:${action} payload includes invalid approvers`;
      }
      if (record.stage_id !== undefined && typeof record.stage_id !== "string") {
        return `Remote replay operation workflow_template:${action} payload includes invalid approvers`;
      }
      if (typeof record.stage_id === "string" && !stageIds.has(record.stage_id)) {
        return `Remote replay operation workflow_template:${action} payload approver stage does not exist`;
      }
    }
  }

  if (entityType === "property_schema" && action === "reorder") {
    if (typeof payloadRecord.space_id !== "string" || !Array.isArray(payloadRecord.property_ids)) {
      return "Remote replay operation property_schema:reorder payload is missing space or property order";
    }
    const blankContextFailure = providerRemoteImportBlankStringFailure(payloadRecord, [
      {
        key: "space_id",
        label: "Remote replay operation property_schema:reorder payload space is invalid",
      },
    ]);
    if (blankContextFailure) return blankContextFailure;
    const propertyIds = payloadRecord.property_ids;
    if (!propertyIds.every((id) => typeof id === "string" && Boolean(id.trim()))) {
      return "Remote replay operation property_schema:reorder payload includes invalid property IDs";
    }
    if (new Set(propertyIds).size !== propertyIds.length) {
      return "Remote replay operation property_schema:reorder payload includes duplicate property IDs";
    }
  }

  if (entityType === "property_schema" && (action === "create" || action === "update")) {
    if (typeof payloadRecord.space_id !== "string" || typeof payloadRecord.name !== "string") {
      return `Remote replay operation property_schema:${action} payload is missing space or name`;
    }
    const blankContextFailure = providerRemoteImportBlankStringFailure(payloadRecord, [
      {
        key: "space_id",
        label: `Remote replay operation property_schema:${action} payload space is invalid`,
      },
    ]);
    if (blankContextFailure) return blankContextFailure;
    if (!payloadRecord.name.trim()) {
      return `Remote replay operation property_schema:${action} payload is missing property name`;
    }
    const schemaFieldFailure = providerPropertySchemaFieldsFailure(payloadRecord);
    if (schemaFieldFailure) {
      return `Remote replay operation property_schema:${action} payload ${schemaFieldFailure} is invalid`;
    }
    if (
      payloadRecord.options !== undefined &&
      payloadRecord.options !== null &&
      !(
        (Array.isArray(payloadRecord.options) &&
          payloadRecord.options.every((option) => typeof option === "string")) ||
        (typeof payloadRecord.options === "object" && !Array.isArray(payloadRecord.options))
      )
    ) {
      return `Remote replay operation property_schema:${action} payload options are invalid`;
    }
  }

  if (entityType === "document_property_values" && action === "set") {
    if (
      typeof payloadRecord.document_id !== "string" ||
      typeof payloadRecord.space_id !== "string" ||
      !Array.isArray(payloadRecord.values)
    ) {
      return "Remote replay operation document_property_values:set payload is missing document, space, or values";
    }
    const blankContextFailure = providerRemoteImportBlankStringFailure(payloadRecord, [
      {
        key: "document_id",
        label: "Remote replay operation document_property_values:set payload document is invalid",
      },
      {
        key: "space_id",
        label: "Remote replay operation document_property_values:set payload space is invalid",
      },
    ]);
    if (blankContextFailure) return blankContextFailure;
    const propertyIds = new Set<string>();
    for (const value of payloadRecord.values) {
      if (!value || typeof value !== "object" || Array.isArray(value)) {
        return "Remote replay operation document_property_values:set payload includes an invalid value entry";
      }
      const entry = value as Record<string, unknown>;
      if (
        typeof entry.property_id !== "string" ||
        !entry.property_id.trim() ||
        (entry.value !== null && typeof entry.value !== "string")
      ) {
        return "Remote replay operation document_property_values:set payload includes a value without property or value";
      }
      if (
        (entry.id !== undefined && entry.id !== null && typeof entry.id !== "string") ||
        (entry.created_at !== undefined &&
          entry.created_at !== null &&
          typeof entry.created_at !== "string") ||
        (entry.updated_at !== undefined &&
          entry.updated_at !== null &&
          typeof entry.updated_at !== "string")
      ) {
        return "Remote replay operation document_property_values:set payload includes invalid value metadata";
      }
      if (propertyIds.has(entry.property_id)) {
        return "Remote replay operation document_property_values:set payload includes duplicate property values";
      }
      propertyIds.add(entry.property_id);
    }
  }

  if (entityType === "asset" && action === "create") {
    if (
      typeof payloadRecord.space_id !== "string" ||
      typeof payloadRecord.name !== "string" ||
      typeof payloadRecord.size !== "number"
    ) {
      return "Remote replay operation asset:create payload is missing space, name, or size";
    }
    const blankContextFailure = providerRemoteImportBlankStringFailure(payloadRecord, [
      { key: "space_id", label: "Remote replay operation asset:create payload space is invalid" },
    ]);
    if (blankContextFailure) return blankContextFailure;
    if (!payloadRecord.name.trim()) {
      return "Remote replay operation asset:create payload is missing asset name";
    }
    if (
      !Number.isFinite(payloadRecord.size) ||
      payloadRecord.size < 0 ||
      !Number.isInteger(payloadRecord.size)
    ) {
      return "Remote replay operation asset:create payload size is invalid";
    }
    if (
      payloadRecord.document_id !== undefined &&
      payloadRecord.document_id !== null &&
      typeof payloadRecord.document_id !== "string"
    ) {
      return "Remote replay operation asset:create payload document is invalid";
    }
    if (
      payloadRecord.mime_type !== undefined &&
      payloadRecord.mime_type !== null &&
      typeof payloadRecord.mime_type !== "string"
    ) {
      return "Remote replay operation asset:create payload MIME type is invalid";
    }
    if (
      payloadRecord.created_at !== undefined &&
      payloadRecord.created_at !== null &&
      typeof payloadRecord.created_at !== "string"
    ) {
      return "Remote replay operation asset:create payload created timestamp is invalid";
    }
    const url =
      typeof payloadRecord.data_url === "string"
        ? payloadRecord.data_url
        : typeof payloadRecord.url === "string"
          ? payloadRecord.url
          : "";
    if (!url.startsWith("data:")) {
      return "Remote replay operation asset:create payload requires embedded data URL bytes";
    }
    const byteSize = providerRemoteImportDataUrlByteSize(url);
    if (byteSize === null) {
      return "Remote replay operation asset:create payload includes invalid data URL bytes";
    }
    if (byteSize !== payloadRecord.size) {
      return "Remote replay operation asset:create payload byte size does not match declared size";
    }
  }

  if (
    entityType === "document_workflow" &&
    (action === "create" || action === "update" || action === "transition")
  ) {
    if (
      typeof payloadRecord.id !== "string" ||
      typeof payloadRecord.document_id !== "string" ||
      typeof payloadRecord.workflow_template_id !== "string" ||
      typeof payloadRecord.current_stage_id !== "string" ||
      typeof payloadRecord.status !== "string"
    ) {
      return `Remote replay operation document_workflow:${action} payload is missing workflow state`;
    }
    if (
      !payloadRecord.document_id.trim() ||
      !payloadRecord.workflow_template_id.trim() ||
      !payloadRecord.current_stage_id.trim()
    ) {
      return `Remote replay operation document_workflow:${action} payload includes empty workflow state fields`;
    }
    if (
      payloadRecord.status !== "active" &&
      payloadRecord.status !== "completed" &&
      payloadRecord.status !== "cancelled"
    ) {
      return `Remote replay operation document_workflow:${action} payload has invalid status`;
    }
    if (
      payloadRecord.space_id !== undefined &&
      payloadRecord.space_id !== null &&
      typeof payloadRecord.space_id !== "string"
    ) {
      return `Remote replay operation document_workflow:${action} payload space is invalid`;
    }
    if (typeof payloadRecord.space_id === "string" && !payloadRecord.space_id.trim()) {
      return `Remote replay operation document_workflow:${action} payload space is invalid`;
    }
    const optionalMetadataFailure = providerRemoteImportOptionalStringFailure(payloadRecord, [
      {
        key: "created_by",
        label: `Remote replay operation document_workflow:${action} payload creator is invalid`,
      },
      {
        key: "created_at",
        label: `Remote replay operation document_workflow:${action} payload created timestamp is invalid`,
      },
      {
        key: "updated_at",
        label: `Remote replay operation document_workflow:${action} payload updated timestamp is invalid`,
      },
    ]);
    if (optionalMetadataFailure) return optionalMetadataFailure;
  }

  if (entityType === "document_workflow_event") {
    if (
      typeof payloadRecord.id !== "string" ||
      typeof payloadRecord.document_workflow_id !== "string" ||
      typeof payloadRecord.document_id !== "string" ||
      typeof payloadRecord.to_stage_id !== "string" ||
      typeof payloadRecord.action !== "string" ||
      typeof payloadRecord.created_at !== "string"
    ) {
      return `Remote replay operation document_workflow_event:${action} payload is missing event state`;
    }
    if (
      !payloadRecord.document_workflow_id.trim() ||
      !payloadRecord.document_id.trim() ||
      !payloadRecord.to_stage_id.trim() ||
      !payloadRecord.created_at.trim()
    ) {
      return `Remote replay operation document_workflow_event:${action} payload includes empty event state fields`;
    }
    if (
      payloadRecord.space_id !== undefined &&
      payloadRecord.space_id !== null &&
      typeof payloadRecord.space_id !== "string"
    ) {
      return `Remote replay operation document_workflow_event:${action} payload space is invalid`;
    }
    if (typeof payloadRecord.space_id === "string" && !payloadRecord.space_id.trim()) {
      return `Remote replay operation document_workflow_event:${action} payload space is invalid`;
    }
    if (
      payloadRecord.from_stage_id !== undefined &&
      payloadRecord.from_stage_id !== null &&
      typeof payloadRecord.from_stage_id !== "string"
    ) {
      return `Remote replay operation document_workflow_event:${action} payload source stage is invalid`;
    }
    if (typeof payloadRecord.from_stage_id === "string" && !payloadRecord.from_stage_id.trim()) {
      return `Remote replay operation document_workflow_event:${action} payload source stage is invalid`;
    }
    if (
      payloadRecord.note !== undefined &&
      payloadRecord.note !== null &&
      typeof payloadRecord.note !== "string"
    ) {
      return `Remote replay operation document_workflow_event:${action} payload note is invalid`;
    }
    if (
      payloadRecord.created_by !== undefined &&
      payloadRecord.created_by !== null &&
      typeof payloadRecord.created_by !== "string"
    ) {
      return `Remote replay operation document_workflow_event:${action} payload creator is invalid`;
    }
  }

  return null;
}

function providerRemoteImportDocumentConflictKeys(
  entityType: string,
  entityId: string,
  payload?: unknown,
): string[] {
  const keys = new Set<string>();
  const payloadRecord =
    payload && typeof payload === "object" && !Array.isArray(payload)
      ? (payload as Record<string, unknown>)
      : {};
  const documentId =
    typeof payloadRecord.document_id === "string" && payloadRecord.document_id.trim()
      ? payloadRecord.document_id.trim()
      : undefined;

  if (entityType === "document") {
    keys.add(`document:${entityId}`);
  }
  if (entityType === "document_property_values") {
    keys.add(`document:${documentId ?? entityId}`);
  }
  if (entityType === "document_tag") {
    const relationshipDocumentId = entityId.split(":")[0];
    if (documentId) keys.add(`document:${documentId}`);
    if (relationshipDocumentId) keys.add(`document:${relationshipDocumentId}`);
  }
  if (entityType === "document_workflow" && documentId) {
    keys.add(`document:${documentId}`);
  }
  if (entityType === "document_workflow_event" && documentId) {
    keys.add(`document:${documentId}`);
  }
  if (entityType === "asset" && documentId) {
    keys.add(`document:${documentId}`);
  }

  return Array.from(keys);
}

function providerRemoteImportRelatedEntityConflictKeys(
  entityType: string,
  payload?: unknown,
): string[] {
  const keys = new Set<string>();
  const payloadRecord =
    payload && typeof payload === "object" && !Array.isArray(payload)
      ? (payload as Record<string, unknown>)
      : {};

  if (entityType === "document_tag") {
    const tagId = payloadRecord.tag_id;
    if (typeof tagId === "string" && tagId.trim()) {
      keys.add(`tag:${tagId.trim()}`);
    }
  }

  if (entityType === "document") {
    const folderId = payloadRecord.folder_id;
    if (typeof folderId === "string" && folderId.trim()) {
      keys.add(`folder:${folderId.trim()}`);
    }
  }

  if (entityType === "document_property_values") {
    const values = payloadRecord.values;
    if (Array.isArray(values)) {
      for (const value of values) {
        if (!value || typeof value !== "object" || Array.isArray(value)) continue;
        const propertyId = (value as Record<string, unknown>).property_id;
        if (typeof propertyId === "string" && propertyId.trim()) {
          keys.add(`property_schema:${propertyId.trim()}`);
        }
      }
    }
  }

  if (entityType === "document_workflow") {
    const templateId = payloadRecord.workflow_template_id;
    if (typeof templateId === "string" && templateId.trim()) {
      keys.add(`workflow_template:${templateId.trim()}`);
    }
  }

  if (entityType === "document_workflow_event") {
    const workflowId = payloadRecord.document_workflow_id;
    if (typeof workflowId === "string" && workflowId.trim()) {
      keys.add(`document_workflow:${workflowId.trim()}`);
    }
  }

  return Array.from(keys);
}

/** Reject the whole unsupported dedicated move before ordinary per-item import. */
export function assertNoReviewedMoveProviderManifest(manifest: ProviderRemoteSyncManifest | undefined): void {
  const rows = manifest ? replayOperationsFromManifest(manifest) : [];
  for (const raw of rows) {
    if (!raw || typeof raw !== "object") continue;
    const row = raw as Record<string, unknown>;
    requireNoPendingGoogleReviewedMoves([{ entity_type: typeof row.entity_type === "string" ? row.entity_type : "", action: typeof row.action === "string" ? row.action : "", payload: row.payload }]);
  }
}

export function buildProviderRemoteImportDryRun(
  manifest: ProviderRemoteSyncManifest | undefined,
  localQueuedOperationIds: string[] = [],
  localQueuedOperations: ProviderRemoteImportLocalQueuedOperationIdentity[] = [],
  options: ProviderRemoteImportDryRunOptions = {},
): ProviderRemoteImportDryRun {
  assertNoReviewedMoveProviderManifest(manifest);
  requireNoPendingGoogleReviewedMoves(localQueuedOperations);
  const preflight = buildProviderRemoteImportPreflight(manifest);
  if (!manifest) {
    return {
      schema_version: 1,
      provider: preflight.provider,
      status: "blocked",
      import_ready: false,
      operation_count: 0,
      would_apply_count: 0,
      conflict_count: 0,
      failed_count: 0,
      unsupported_count: 0,
      blockers: preflight.blockers,
      operations: [],
    };
  }

  const localIds = new Set(localQueuedOperationIds);
  const localEntityKeys = new Set(
    localQueuedOperations.map(
      (operation) => `${operation.entity_type}:${operation.entity_id}`,
    ),
  );
  const localDocumentConflictKeys = new Set(
    localQueuedOperations.flatMap((operation) =>
      providerRemoteImportDocumentConflictKeys(
        operation.entity_type,
        operation.entity_id,
        operation.payload,
      ),
    ),
  );
  const remoteOperationIds = new Set<string>();
  const operations = replayOperationsFromManifest(manifest).map((operation, index) => {
    if (typeof operation !== "object" || operation === null) {
      return {
        operation_id: `invalid-${index + 1}`,
        entity_type: "unknown",
        entity_id: "unknown",
        action: "unknown",
        status: "unsupported",
        message: "Remote replay operation is not an object",
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    const candidate = operation as Record<string, unknown>;
    const operationId =
      typeof candidate.id === "string" && candidate.id.trim()
        ? candidate.id
        : `invalid-${index + 1}`;
    const entityType =
      typeof candidate.entity_type === "string" && candidate.entity_type.trim()
        ? candidate.entity_type
        : "unknown";
    const entityId =
      typeof candidate.entity_id === "string" && candidate.entity_id.trim()
        ? candidate.entity_id
        : "unknown";
    const action =
      typeof candidate.action === "string" && candidate.action.trim()
        ? candidate.action
        : "unknown";
    const payloadStatus = candidate.payload_status;
    const payload = candidate.payload;

    if (
      typeof candidate.id !== "string" ||
      !candidate.id.trim() ||
      typeof candidate.entity_type !== "string" ||
      !candidate.entity_type.trim() ||
      typeof candidate.entity_id !== "string" ||
      !candidate.entity_id.trim() ||
      typeof candidate.action !== "string" ||
      !candidate.action.trim()
    ) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "unsupported",
        message: "Remote replay operation is missing required identity fields",
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    if (
      candidate.id !== candidate.id.trim() ||
      candidate.entity_type !== candidate.entity_type.trim() ||
      candidate.entity_id !== candidate.entity_id.trim() ||
      candidate.action !== candidate.action.trim()
    ) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: "Remote replay operation identity fields must not include leading or trailing whitespace",
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    if (preflight.blockers.length > 0) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: `Provider import preflight is blocked: ${preflight.blockers.join("; ")}`,
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    if (
      typeof payloadStatus !== "string" ||
      payloadStatus !== payloadStatus.trim() ||
      !isProviderReplayPayloadStatus(payloadStatus)
    ) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: "Remote replay operation payload status is invalid",
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    if (remoteOperationIds.has(operationId)) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: "Remote replay operation ID is duplicated in the provider manifest",
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }
    remoteOperationIds.add(operationId);

    if (localIds.has(operationId)) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "conflict",
        message: "A local queued operation with the same ID already exists",
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    const destructiveSpaceRecovery =
      options.allowDestructiveSpaceRecovery && entityType === "space" && action === "delete";
    if (!destructiveSpaceRecovery && !isProviderRemoteImportOperationSupported(entityType, action)) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "unsupported",
        message: getProviderRemoteImportUnsupportedMessage(entityType, action),
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    if (localEntityKeys.has(`${entityType}:${entityId}`)) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "conflict",
        message: "A local queued operation targets the same entity",
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    if (
      providerRemoteImportRelatedEntityConflictKeys(entityType, payload).some((key) =>
        localEntityKeys.has(key),
      )
    ) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "conflict",
        message: "A local queued operation targets a related entity",
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    if (
      providerRemoteImportDocumentConflictKeys(entityType, entityId, payload).some((key) =>
        localDocumentConflictKeys.has(key),
      )
    ) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "conflict",
        message: "A local queued operation targets the same document",
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    if (payloadStatus !== "available") {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: "Remote replay operation payload is not available",
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    const payloadRequiredFailure = providerRemoteImportPayloadRequiredFailure(
      entityType,
      action,
      payload,
    );
    if (payloadRequiredFailure) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: payloadRequiredFailure,
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    const deleteContextFailure = providerRemoteImportDeleteContextFailure(
      entityType,
      action,
      payload,
    );
    if (deleteContextFailure) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: deleteContextFailure,
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    const payloadIdentityFailure = providerRemoteImportPayloadIdentityFailure(
      entityType,
      action,
      entityId,
      payload,
    );
    if (payloadIdentityFailure) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: payloadIdentityFailure,
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    const payloadSemanticFailure = providerRemoteImportPayloadSemanticFailure(
      entityType,
      action,
      payload,
    );
    if (payloadSemanticFailure) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: payloadSemanticFailure,
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    const payloadActionFailure = providerRemoteImportPayloadActionFailure(
      entityType,
      action,
      payload,
    );
    if (payloadActionFailure) {
      return {
        operation_id: operationId,
        entity_type: entityType,
        entity_id: entityId,
        action,
        status: "failed",
        message: payloadActionFailure,
        payload,
      } satisfies ProviderRemoteImportDryRunOperation;
    }

    return {
      operation_id: operationId,
      entity_type: entityType,
      entity_id: entityId,
      action,
      status: "would_apply",
      message: "Remote replay operation is eligible for a future explicit import apply step",
      payload,
    } satisfies ProviderRemoteImportDryRunOperation;
  });

  const wouldApplyCount = operations.filter((operation) => operation.status === "would_apply").length;
  const conflictCount = operations.filter((operation) => operation.status === "conflict").length;
  const failedCount = operations.filter((operation) => operation.status === "failed").length;
  const unsupportedCount = operations.filter((operation) => operation.status === "unsupported").length;

  return {
    schema_version: 1,
    provider: manifest.envelope.provider,
    status: "blocked",
    import_ready: false,
    operation_count: operations.length,
    would_apply_count: wouldApplyCount,
    conflict_count: conflictCount,
    failed_count: failedCount,
    unsupported_count: unsupportedCount,
    blockers: [
      ...preflight.blockers,
      "Remote provider replay import dry-run does not mutate the local vault",
    ],
    operations,
  };
}

export function buildProviderRemoteImportApplyPlan(
  dryRun: ProviderRemoteImportDryRun,
): ProviderRemoteImportApplyPlan {
  const operations = dryRun.operations.map((operation) => {
    if (operation.status === "would_apply") {
      return {
        operation_id: operation.operation_id,
        entity_type: operation.entity_type,
        entity_id: operation.entity_id,
        action: operation.action,
        source_status: operation.status,
        apply_status: "selected",
        message: "Remote replay operation is selected for a future explicit local import apply step",
        payload: operation.payload,
      } satisfies ProviderRemoteImportApplyPlanOperation;
    }

    const applyStatus =
      operation.status === "conflict"
        ? "skipped_conflict"
        : operation.status === "failed"
          ? "skipped_failed"
          : "skipped_unsupported";
    return {
      operation_id: operation.operation_id,
      entity_type: operation.entity_type,
      entity_id: operation.entity_id,
      action: operation.action,
      source_status: operation.status,
      apply_status: applyStatus,
      message: operation.message,
      payload: operation.payload,
    } satisfies ProviderRemoteImportApplyPlanOperation;
  });
  const selectedOperationCount = operations.filter(
    (operation) => operation.apply_status === "selected",
  ).length;

  return {
    schema_version: 1,
    provider: dryRun.provider,
    status: "blocked",
    apply_ready: false,
    selected_operation_count: selectedOperationCount,
    skipped_operation_count: operations.length - selectedOperationCount,
    blockers: [
      ...dryRun.blockers,
      "Remote provider replay import apply requires an explicit local merge executor",
    ],
    operations,
  };
}

export async function executeProviderRemoteImportApplyPlan(
  plan: ProviderRemoteImportApplyPlan,
  options: ProviderRemoteImportApplyExecutionOptions = {},
): Promise<ProviderRemoteImportApplyExecutionResult> {
  requireNoPendingGoogleReviewedMoves(plan.operations);
  const executedAt = options.executedAt ?? new Date().toISOString();
  const selectedOperations = plan.operations.filter(
    (operation) => operation.apply_status === "selected",
  );
  const skippedResults = plan.operations
    .filter((operation) => operation.apply_status !== "selected")
    .map((operation) => {
      const status =
        selectedOperations.length > 0
          ? "skipped"
          : operation.apply_status === "skipped_failed"
            ? "failed"
            : "blocked";
      return {
        operation_id: operation.operation_id,
        entity_type: operation.entity_type,
        entity_id: operation.entity_id,
        action: operation.action,
        status,
        message: operation.message,
      } satisfies ProviderRemoteImportApplyExecutionOperationResult;
    });

  if (!options.allowLocalMutation || !options.mergeExecutor) {
    const disabledResults = [
      ...selectedOperations.map((operation) => ({
        operation_id: operation.operation_id,
        entity_type: operation.entity_type,
        entity_id: operation.entity_id,
        action: operation.action,
        status: "blocked",
        message: "Remote replay operation was not applied because local mutation is disabled",
      }) satisfies ProviderRemoteImportApplyExecutionOperationResult),
      ...skippedResults,
    ];
    return {
      schema_version: 1,
      provider: plan.provider,
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: disabledResults.filter((result) => result.status === "skipped")
        .length,
      failed_operation_count: disabledResults.filter((result) => result.status === "failed")
        .length,
      blocked_operation_count: disabledResults.filter((result) => result.status === "blocked")
        .length,
      blockers: [
        ...plan.blockers,
        "Remote provider replay import apply execution requires explicit local mutation permission",
      ],
      executed_at: executedAt,
      results: disabledResults,
    };
  }

  const appliedResults: ProviderRemoteImportApplyExecutionOperationResult[] = [];
  let previousSelectedOperationBlocked = false;
  let attemptedSelectedOperationCount = 0;
  for (const operation of selectedOperations) {
    if (previousSelectedOperationBlocked) {
      appliedResults.push({
        operation_id: operation.operation_id,
        entity_type: operation.entity_type,
        entity_id: operation.entity_id,
        action: operation.action,
        status: "blocked",
        message:
          "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
      });
      continue;
    }

    try {
      attemptedSelectedOperationCount += 1;
      const result = await options.mergeExecutor?.applyOperation(operation);
      const operationResult = {
        operation_id: operation.operation_id,
        entity_type: operation.entity_type,
        entity_id: operation.entity_id,
        action: operation.action,
        status: result?.status ?? "failed",
        message: result?.message ?? "Remote replay operation did not return an apply result",
      } satisfies ProviderRemoteImportApplyExecutionOperationResult;
      appliedResults.push(operationResult);
      previousSelectedOperationBlocked =
        operationResult.status === "failed" || operationResult.status === "blocked";
    } catch (error) {
      appliedResults.push({
        operation_id: operation.operation_id,
        entity_type: operation.entity_type,
        entity_id: operation.entity_id,
        action: operation.action,
        status: "failed",
        message:
          error instanceof Error
            ? error.message
            : "Remote replay operation failed during local import apply",
      });
      previousSelectedOperationBlocked = true;
    }
  }
  const results = [...appliedResults, ...skippedResults];
  const failedOperationCount = results.filter((result) => result.status === "failed").length;
  const blockedOperationCount = results.filter((result) => result.status === "blocked").length;
  const unsuccessfulOperationCount = failedOperationCount + blockedOperationCount;

  return {
    schema_version: 1,
    provider: plan.provider,
    status: unsuccessfulOperationCount > 0 ? "blocked" : "completed",
    applied: unsuccessfulOperationCount === 0,
    attempted_operation_count: attemptedSelectedOperationCount,
    applied_operation_count: results.filter((result) => result.status === "applied").length,
    skipped_operation_count: results.filter((result) => result.status === "skipped").length,
    failed_operation_count: failedOperationCount,
    blocked_operation_count: blockedOperationCount,
    blockers:
      unsuccessfulOperationCount > 0
        ? ["One or more remote replay operations failed or remained blocked"]
        : [],
    executed_at: executedAt,
    results,
  };
}

function summarizeGoogleDrivePullDiscovery(
  receipt: Record<string, unknown>,
  requestSpec: ProviderRemoteWriteRequestSpec,
): ProviderRemotePullDiscovery {
  const files = Array.isArray(receipt.files) ? receipt.files : null;
  if (!files) {
    return {
      schema_version: 1,
      provider: "google-drive",
      status: "invalid_response",
      remote_path: requestSpec.remote_path ?? "Starlane Notes/provider-sync-manifest.json",
      import_ready: false,
      blockers: ["Google Drive provider pull discovery did not return a files array"],
    };
  }

  const file = files.find((candidate) => {
    if (typeof candidate !== "object" || candidate === null) return false;
    const value = candidate as Record<string, unknown>;
    return typeof value.id === "string" && typeof value.name === "string";
  }) as Record<string, unknown> | undefined;

  if (!file) {
    return {
      schema_version: 1,
      provider: "google-drive",
      status: "not_found",
      remote_path: requestSpec.remote_path ?? "Starlane Notes/provider-sync-manifest.json",
      import_ready: false,
      blockers: ["Google Drive provider-sync manifest was not found in the selected root"],
    };
  }

  return {
    schema_version: 1,
    provider: "google-drive",
    status: "found",
    remote_path: requestSpec.remote_path ?? "Starlane Notes/provider-sync-manifest.json",
    import_ready: true,
    blockers: [],
    file: {
      id: String(file.id),
      name: String(file.name),
      modified_time: typeof file.modifiedTime === "string" ? file.modifiedTime : undefined,
      size: typeof file.size === "string" ? file.size : undefined,
      web_view_link: typeof file.webViewLink === "string" ? file.webViewLink : undefined,
    },
  };
}

function validateGoogleDriveProviderSession(
  session: ProviderSessionRecord | undefined,
  requestSpec: ProviderRemoteWriteRequestSpec,
  now = new Date(),
): string[] {
  if (!session) return [];
  const blockers: string[] = [];

  if (!session.connected) {
    blockers.push("Google Drive provider API transport requires a connected provider session");
  }
  if (session.sync_mode !== "manual") {
    blockers.push("Google Drive provider API transport requires a real provider auth session");
  }
  if (!session.auth) {
    blockers.push("Google Drive provider API transport requires provider auth tokens");
    return blockers;
  }

  const grantedScopes = new Set(session.auth.scopes);
  const missingScopes = requestSpec.required_scopes.filter(
    (scope) => !grantedScopes.has(scope),
  );
  if (missingScopes.length > 0) {
    blockers.push(
      `Google Drive provider API transport missing scopes: ${missingScopes.join(", ")}`,
    );
  }

  const expiresAt = Date.parse(session.auth.expires_at);
  if (!Number.isFinite(expiresAt) || expiresAt <= now.getTime()) {
    blockers.push("Google Drive provider API transport requires an unexpired access token");
  }

  return blockers;
}

export async function executeProviderRemoteMutationEnvelopeWithTransport(
  envelope: ProviderRemoteMutationEnvelope,
  attemptedAt = new Date().toISOString(),
  transportOptions?: TauriCoreModule | ProviderTransportOptions,
): Promise<ProviderRemoteMutationAttempt> {
  const {
    nativeTransport,
    apiTransport,
    accessToken,
    session,
    providerRootId,
    providerSyncPayload,
  } = normalizeProviderTransportOptions(transportOptions);
  assertNoReviewedMoveProviderManifest(buildProviderRemoteSyncManifest(envelope, providerSyncPayload));
  const requestSpec = buildProviderRemoteWriteRequestSpec(envelope);
  if (envelope.blockers.length > 0 || envelope.dry_run || !envelope.remote_mutation_enabled) {
    return executeProviderRemoteMutationEnvelope(
      envelope,
      requestSpec.request_kind === "icloud-drive-file-upsert" ? "provider-file" : "provider-api",
      attemptedAt,
    );
  }

  if (requestSpec.request_kind === "icloud-drive-file-upsert" && requestSpec.native_command) {
    if (!nativeTransport && !canUseTauriProviderTransport()) {
      return {
        schema_version: 1,
        provider: envelope.provider,
        direction: envelope.direction,
        mutation_id: envelope.mutation_id,
        status: "blocked",
        transport_kind: "provider-file",
        applied: false,
        blockers: ["iCloud provider transport requires the Tauri native command runtime"],
        attempted_at: attemptedAt,
        request_spec: requestSpec,
      };
    }

    try {
      const receipt = nativeTransport
        ? await nativeTransport.invoke<Record<string, unknown>>(requestSpec.native_command, {
            mutationId: envelope.mutation_id,
            manifest: envelope,
          })
        : await invokeTauriProviderTransport<Record<string, unknown>>(requestSpec.native_command, {
            mutationId: envelope.mutation_id,
            manifest: envelope,
          });
      return {
        schema_version: 1,
        provider: envelope.provider,
        direction: envelope.direction,
        mutation_id: envelope.mutation_id,
        status: "applied",
        transport_kind: "provider-file",
        applied: true,
        blockers: [],
        attempted_at: attemptedAt,
        request_spec: requestSpec,
        receipt,
      };
    } catch (error) {
      return {
        schema_version: 1,
        provider: envelope.provider,
        direction: envelope.direction,
        mutation_id: envelope.mutation_id,
        status: "blocked",
        transport_kind: "provider-file",
        applied: false,
        blockers: [
          error instanceof Error ? error.message : "iCloud provider transport failed",
        ],
        attempted_at: attemptedAt,
        request_spec: requestSpec,
      };
    }
  }

  if (
    (requestSpec.request_kind === "google-drive-file-upsert" ||
      requestSpec.request_kind === "google-drive-file-download") &&
    requestSpec.endpoint
  ) {
    if (!apiTransport) {
      return {
        schema_version: 1,
        provider: envelope.provider,
        direction: envelope.direction,
        mutation_id: envelope.mutation_id,
        status: "blocked",
        transport_kind: "provider-api",
        applied: false,
        blockers: ["Google Drive provider API transport is not configured"],
        attempted_at: attemptedAt,
        request_spec: requestSpec,
      };
    }

    const sessionBlockers = validateGoogleDriveProviderSession(session, requestSpec);
    if (sessionBlockers.length > 0 && !accessToken) {
      return {
        schema_version: 1,
        provider: envelope.provider,
        direction: envelope.direction,
        mutation_id: envelope.mutation_id,
        status: "blocked",
        transport_kind: "provider-api",
        applied: false,
        blockers: sessionBlockers,
        attempted_at: attemptedAt,
        request_spec: requestSpec,
      };
    }

    const bearerToken = accessToken ?? session?.auth?.access_token;
    if (!bearerToken) {
      return {
        schema_version: 1,
        provider: envelope.provider,
        direction: envelope.direction,
        mutation_id: envelope.mutation_id,
        status: "blocked",
        transport_kind: "provider-api",
        applied: false,
        blockers: ["Google Drive provider API transport requires an access token"],
        attempted_at: attemptedAt,
        request_spec: requestSpec,
      };
    }

    const effectiveProviderRootId = providerRootId ?? session?.auth?.provider_root_id;
    try {
      const providerEndpoint = requestSpec.endpoint;
      const response =
        requestSpec.request_kind === "google-drive-file-download"
          ? await apiTransport.fetch(googleDrivePullEndpoint(requestSpec, effectiveProviderRootId), {
              method: "GET",
              headers: {
                Authorization: `Bearer ${bearerToken}`,
                "X-Starlane-Idempotency-Key": requestSpec.idempotency_key,
              },
            })
          : await (async () => {
              const multipart = googleDriveMultipartBody(
                envelope,
                requestSpec,
                effectiveProviderRootId,
                providerSyncPayload,
              );
              return apiTransport.fetch(providerEndpoint, {
                method: requestSpec.method ?? "POST",
                headers: {
                  Authorization: `Bearer ${bearerToken}`,
                  "Content-Type": multipart.contentType,
                  "X-Starlane-Idempotency-Key": requestSpec.idempotency_key,
                },
                body: multipart.body,
              });
            })();
      const receipt = await providerApiReceipt(response);
      if (!response.ok) {
        return {
          schema_version: 1,
          provider: envelope.provider,
          direction: envelope.direction,
          mutation_id: envelope.mutation_id,
          status: "blocked",
          transport_kind: "provider-api",
          applied: false,
          blockers: [
            `Google Drive provider API transport failed with ${response.status}${
              response.statusText ? ` ${response.statusText}` : ""
            }`,
          ],
          attempted_at: attemptedAt,
          request_spec: requestSpec,
          receipt,
        };
      }

      const pullDiscovery =
        requestSpec.request_kind === "google-drive-file-download"
          ? summarizeGoogleDrivePullDiscovery(receipt, requestSpec)
          : undefined;
      let remoteManifest: ProviderRemoteMutationEnvelope | undefined;
      let providerSyncManifest: ProviderRemoteSyncManifest | undefined;
      let importPreflight: ProviderRemoteImportPreflight | undefined;
      let importDryRun: ProviderRemoteImportDryRun | undefined;
      let importApplyPlan: ProviderRemoteImportApplyPlan | undefined;
      let successReceipt: Record<string, unknown> = {
        ...receipt,
        endpoint: requestSpec.endpoint,
        remote_path: requestSpec.remote_path,
      };

      if (pullDiscovery?.status === "found" && pullDiscovery.file?.id) {
        const manifestEndpoint = googleDriveManifestContentEndpoint(pullDiscovery.file.id);
        const manifestResponse = await apiTransport.fetch(manifestEndpoint, {
          method: "GET",
          headers: {
            Authorization: `Bearer ${bearerToken}`,
            "X-Starlane-Idempotency-Key": requestSpec.idempotency_key,
          },
        });
        const manifestReceipt = await providerApiReceipt(manifestResponse);
        if (!manifestResponse.ok) {
          return {
            schema_version: 1,
            provider: envelope.provider,
            direction: envelope.direction,
            mutation_id: envelope.mutation_id,
            status: "blocked",
            transport_kind: "provider-api",
            applied: false,
            blockers: [
              `Google Drive provider manifest download failed with ${manifestResponse.status}${
                manifestResponse.statusText ? ` ${manifestResponse.statusText}` : ""
              }`,
            ],
            attempted_at: attemptedAt,
            request_spec: requestSpec,
            receipt: {
              discovery: receipt,
              manifest: manifestReceipt,
              manifest_endpoint: manifestEndpoint,
            },
            pull_discovery: pullDiscovery,
          };
        }

        providerSyncManifest = parseProviderRemoteSyncManifest(manifestReceipt) ?? undefined;
        remoteManifest =
          providerSyncManifest?.envelope ??
          parseProviderRemoteMutationEnvelope(manifestReceipt) ??
          undefined;
        if (!remoteManifest) {
          return {
            schema_version: 1,
            provider: envelope.provider,
            direction: envelope.direction,
            mutation_id: envelope.mutation_id,
            status: "blocked",
            transport_kind: "provider-api",
            applied: false,
            blockers: [
              "Google Drive provider manifest content is not a valid Starlane provider sync manifest",
            ],
            attempted_at: attemptedAt,
            request_spec: requestSpec,
            receipt: {
              discovery: receipt,
              manifest: manifestReceipt,
              manifest_endpoint: manifestEndpoint,
            },
            pull_discovery: pullDiscovery,
          };
        }

        successReceipt = {
          ...successReceipt,
          manifest_endpoint: manifestEndpoint,
        };
        importPreflight = buildProviderRemoteImportPreflight(providerSyncManifest);
        importDryRun = buildProviderRemoteImportDryRun(providerSyncManifest);
        importApplyPlan = buildProviderRemoteImportApplyPlan(importDryRun);
      }

      return {
        schema_version: 1,
        provider: envelope.provider,
        direction: envelope.direction,
        mutation_id: envelope.mutation_id,
        status: "applied",
        transport_kind: "provider-api",
        applied: true,
        blockers: [],
        attempted_at: attemptedAt,
        request_spec: requestSpec,
        receipt: successReceipt,
        pull_discovery: pullDiscovery,
        remote_manifest: remoteManifest,
        provider_sync_manifest: providerSyncManifest,
        import_preflight: importPreflight,
        import_dry_run: importDryRun,
        import_apply_plan: importApplyPlan,
      };
    } catch (error) {
      return {
        schema_version: 1,
        provider: envelope.provider,
        direction: envelope.direction,
        mutation_id: envelope.mutation_id,
        status: "blocked",
        transport_kind: "provider-api",
        applied: false,
        blockers: [
          error instanceof Error ? error.message : "Google Drive provider API transport failed",
        ],
        attempted_at: attemptedAt,
        request_spec: requestSpec,
      };
    }
  }

  return executeProviderRemoteMutationEnvelope(
    envelope,
    requestSpec.request_kind === "google-drive-file-upsert" ? "provider-api" : "contract-only",
    attemptedAt,
  );
}

class MockCloudProviderAdapter implements ProviderAdapter {
  kind: SyncProviderKind;
  label: string;
  capabilities: ProviderCapability[] = [
    "connect",
    "disconnect",
    "pull",
    "push",
    "conflict-resolution",
  ];

  constructor(kind: SyncProviderKind, label: string) {
    this.kind = kind;
    this.label = label;
  }

  private remoteContract(remoteSyncEnabled = false): ProviderRemoteContract {
    if (this.kind === "icloud-drive") {
      return {
        status: remoteSyncEnabled ? "enabled" : "contract_only",
        root_strategy: "native-ubiquity-documents-container",
        auth_model: "apple-account-via-native-container",
        required_scopes: ["icloud.documents", "icloud.files", "offline_access"],
        write_model: "local-sqlite-first-then-provider-file-push",
        conflict_model: "provider-version-plus-local-replay-queue-validation",
      };
    }

    return {
      status: remoteSyncEnabled ? "enabled" : "contract_only",
      root_strategy: "drive-appdata-or-user-selected-folder",
      auth_model: "oauth2-offline-access",
      required_scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
      write_model: "local-sqlite-first-then-provider-api-push",
      conflict_model: "provider-revision-plus-local-replay-queue-validation",
    };
  }

  async connect(): Promise<void> {
    writeProviderSession(this.kind, createConnectedProviderSession(this.kind));
  }

  async disconnect(): Promise<void> {
    const session = readProviderSession(this.kind);
    writeProviderSession(this.kind, {
      ...createDisconnectedProviderSession(this.kind),
      auth: session.auth,
      last_synced_at: session.last_synced_at,
      last_operation: "disconnect",
    });
    await this.disableRemoteSync("provider disconnected");
  }

  async enableRemoteSync(): Promise<void> {
    const session = readProviderSession(this.kind);
    const sessionSummary = summarizeProviderSession(session);
    const remoteContract = this.remoteContract(false);
    const plan = buildProviderRemoteEnablementPlan(
      this.kind,
      this.label,
      remoteContract,
      sessionSummary,
    );
    if (plan.status !== "ready_to_enable") {
      throw new Error(`Provider remote sync is not ready to enable: ${plan.blockers.join("; ")}`);
    }
    writeProviderRemoteSyncEnablement(this.kind, {
      provider: this.kind,
      enabled: true,
      enablement_boundary: "explicit_manual_provider_session",
      enabled_at: new Date().toISOString(),
    });
  }

  async disableRemoteSync(reason = "disabled explicitly"): Promise<void> {
    writeProviderRemoteSyncEnablement(this.kind, {
      provider: this.kind,
      enabled: false,
      enablement_boundary: "explicit_manual_provider_session",
      disabled_at: new Date().toISOString(),
      reason,
    });
  }

  async getStatus(): Promise<ProviderStatus> {
    const session = readProviderSession(this.kind);
    const sessionSummary = summarizeProviderSession(session);
    const storedEnablement = readProviderRemoteSyncEnablement(this.kind);
    const validationEvidence = readProviderDefaultSyncValidationEvidence(this.kind);
    const baseContract = this.remoteContract(false);
    const remoteEnablement = buildProviderRemoteEnablementPlan(
      this.kind,
      this.label,
      baseContract,
      sessionSummary,
    );
    const effectiveEnablement: ProviderRemoteSyncEnablementRecord =
      storedEnablement.enabled && remoteEnablement.status === "ready_to_enable"
        ? storedEnablement
        : {
            ...storedEnablement,
            enabled: false,
            reason: storedEnablement.enabled
              ? "stored enablement is blocked by current provider readiness"
              : storedEnablement.reason,
          };
    const remoteContract = this.remoteContract(effectiveEnablement.enabled);
    const defaultSyncPolicy = buildProviderDefaultSyncPolicy(
      this.kind,
      this.label,
      remoteEnablement,
      effectiveEnablement,
      validationEvidence,
    );
    return {
      connected: session.connected,
      last_synced_at: session.last_synced_at,
      session: sessionSummary,
      remote_contract: remoteContract,
      remote_sync_enablement: effectiveEnablement,
      remote_enablement: remoteEnablement,
      default_sync_policy: defaultSyncPolicy,
      remote_mutation_plans: [
        buildProviderRemoteMutationPlan(this.kind, this.label, "pull", remoteContract, sessionSummary),
        buildProviderRemoteMutationPlan(this.kind, this.label, "push", remoteContract, sessionSummary),
      ],
    };
  }

  private requireConnected(operation: "pull" | "push"): ProviderSessionRecord {
    const session = readProviderSession(this.kind);
    if (!session.connected) {
      throw createProviderNotConnectedError(this.label, operation);
    }
    return session;
  }

  private completeSync(
    operation: "pull" | "push",
    result: Omit<ProviderSyncResult, "synced_at">,
  ): ProviderSyncResult {
    const syncedAt = new Date().toISOString();
    const session = readProviderSession(this.kind);
    writeProviderSession(this.kind, {
      ...session,
      connected: true,
      last_synced_at: syncedAt,
      last_operation: operation,
    });
    return {
      synced_at: syncedAt,
      ...result,
    };
  }

  private providerTransportKind(): ProviderRemoteMutationAttempt["transport_kind"] {
    return this.kind === "icloud-drive" ? "provider-file" : "provider-api";
  }

  async pull(transportOptions?: ProviderTransportOptions): Promise<ProviderSyncResult> {
    const session = this.requireConnected("pull");
    const status = await this.getStatus();
    const plan = buildProviderRemoteMutationPlan(
      this.kind,
      this.label,
      "pull",
      status.remote_contract,
      summarizeProviderSession(session),
    );
    const envelope = buildProviderRemoteMutationEnvelope(plan);
    const attempt =
      session.sync_mode === "simulated"
        ? executeProviderRemoteMutationEnvelope(envelope, "contract-only")
        : transportOptions
          ? await executeProviderRemoteMutationEnvelopeWithTransport(envelope, undefined, {
              ...transportOptions,
              session: transportOptions.session ?? session,
            })
        : executeProviderRemoteMutationEnvelope(envelope, this.providerTransportKind());
    return this.completeSync("pull", {
      pulled: attempt.applied ? 1 : attempt.status === "dry_run" ? 3 : 0,
      pushed: 0,
      conflicts: 0,
      remote_mutation_envelope: envelope,
      remote_mutation_attempt: attempt,
    });
  }

  async push(transportOptions?: ProviderTransportOptions): Promise<ProviderSyncResult> {
    const session = this.requireConnected("push");
    const status = await this.getStatus();
    const plan = buildProviderRemoteMutationPlan(
      this.kind,
      this.label,
      "push",
      status.remote_contract,
      summarizeProviderSession(session),
    );
    const envelope = buildProviderRemoteMutationEnvelope(plan);
    const attempt =
      session.sync_mode === "simulated"
        ? executeProviderRemoteMutationEnvelope(envelope, "contract-only")
        : transportOptions
          ? await executeProviderRemoteMutationEnvelopeWithTransport(envelope, undefined, {
              ...transportOptions,
              session: transportOptions.session ?? session,
            })
          : executeProviderRemoteMutationEnvelope(envelope, this.providerTransportKind());
    return this.completeSync("push", {
      pulled: 0,
      pushed: attempt.applied ? 1 : attempt.status === "dry_run" ? 2 : 0,
      conflicts: 0,
      remote_mutation_envelope: envelope,
      remote_mutation_attempt: attempt,
    });
  }
}

class LocalOnlyAdapter implements ProviderAdapter {
  kind: SyncProviderKind = "none";
  label = "Local only";
  capabilities: ProviderCapability[] = [];

  async connect(): Promise<void> {
    // No-op for local-only mode.
  }

  async disconnect(): Promise<void> {
    // No-op for local-only mode.
  }

  async enableRemoteSync(): Promise<void> {
    // No-op for local-only mode.
  }

  async disableRemoteSync(): Promise<void> {
    // No-op for local-only mode.
  }

  async getStatus(): Promise<ProviderStatus> {
    const remoteContract: ProviderRemoteContract = {
      status: "not_applicable",
      root_strategy: "local-vault",
      auth_model: "none",
      required_scopes: [],
      write_model: "local-only",
      conflict_model: "none",
    };
    const remoteEnablement = buildProviderRemoteEnablementPlan("none", this.label, remoteContract);
    const remoteSyncEnablement: ProviderRemoteSyncEnablementRecord = {
      provider: "none",
      enabled: false,
      enablement_boundary: "explicit_manual_provider_session",
    };
    return {
      connected: true,
      last_synced_at: undefined,
      session: {
        provider: "none",
        connected: true,
        account_label: "Local vault",
        scopes: ["local.vault"],
        sync_mode: "simulated",
      },
      remote_contract: remoteContract,
      remote_sync_enablement: remoteSyncEnablement,
      remote_enablement: remoteEnablement,
      default_sync_policy: buildProviderDefaultSyncPolicy(
        "none",
        this.label,
        remoteEnablement,
        remoteSyncEnablement,
      ),
      remote_mutation_plans: [
        buildProviderRemoteMutationPlan("none", this.label, "pull", remoteContract),
        buildProviderRemoteMutationPlan("none", this.label, "push", remoteContract),
      ],
    };
  }

  async pull(): Promise<ProviderSyncResult> {
    return {
      synced_at: new Date().toISOString(),
      pulled: 0,
      pushed: 0,
      conflicts: 0,
    };
  }

  async push(): Promise<ProviderSyncResult> {
    return {
      synced_at: new Date().toISOString(),
      pulled: 0,
      pushed: 0,
      conflicts: 0,
    };
  }
}

export function createProviderAdapter(kind: SyncProviderKind): ProviderAdapter {
  if (kind === "none") return new LocalOnlyAdapter();
  if (kind === "icloud-drive") {
    return new MockCloudProviderAdapter("icloud-drive", "iCloud Drive");
  }
  return new MockCloudProviderAdapter("google-drive", "Google Drive");
}
