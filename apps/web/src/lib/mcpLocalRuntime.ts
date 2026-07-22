import {
  dataClient,
  type DocumentRecord,
  type FolderRecord,
  type GraphRecord,
  type PropertySchemaRecord,
  type SearchResultRecord,
  type SpaceRecord,
  type TagRecord,
  type TemplateRecord,
} from "@/lib/dataClient";

export type LocalMcpRuntimeMode = "local_vault";

export type LocalMcpTransportKind = "stdio" | "sse";

export type LocalMcpCapabilityTokenStatus = "disabled";

export type LocalMcpCapabilityTokenLifecycleStatus = "planned";

export type LocalMcpCapabilityTokenValidationReason =
  | "capability_token_not_enabled"
  | "capability_token_missing";

export type LocalMcpCapabilityTokenRequestScope = "notes:read";

export type LocalMcpCapabilityTokenRequestValidationReason =
  | "request_id_required"
  | "created_at_required"
  | "created_at_invalid"
  | "runtime_mode_mismatch"
  | "transport_unsupported"
  | "scope_unsupported"
  | "vault_fingerprint_required"
  | "confirmation_mismatch"
  | "capability_token_not_enabled";

export type LocalMcpCapabilityTokenRevocationValidationReason =
  | "token_id_required"
  | "requested_at_required"
  | "requested_at_invalid"
  | "token_fingerprint_required"
  | "confirmation_mismatch"
  | "capability_token_not_enabled";

export type LocalMcpCapabilityTokenExpiryValidationReason =
  | "token_id_required"
  | "issued_at_required"
  | "issued_at_invalid"
  | "expires_at_required"
  | "expires_at_invalid"
  | "checked_at_required"
  | "checked_at_invalid"
  | "expiry_before_issue"
  | "capability_token_expired"
  | "capability_token_not_enabled";

export type LocalMcpOperationJournalStatus = "planned";

const LOCAL_MCP_CAPABILITY_TOKEN_STORAGE_KEY = "starlane.localMcp.capabilityTokens.v1";
const LOCAL_MCP_OPERATION_JOURNAL_STORAGE_KEY = "starlane.localMcp.operationJournal.v1";

export interface LocalMcpToolExecutionRequest {
  input?: unknown;
  confirmation_text?: string;
  capability_token?: string;
}

export interface LocalMcpTransportRequestEnvelope {
  id: string;
  transport: LocalMcpTransportKind;
  tool: string;
  request?: LocalMcpToolExecutionRequest;
}

export type LocalMcpTransportEnvelopeValidationReason =
  | "request_id_required"
  | "transport_unsupported"
  | "tool_required";

export interface LocalMcpTransportEnvelopeValidation {
  valid: boolean;
  reason?: LocalMcpTransportEnvelopeValidationReason;
  error_code?: LocalMcpErrorCode;
}

export type LocalMcpErrorCode =
  | "invalid_request"
  | "transport_disabled"
  | "capability_token_required"
  | "capability_token_not_enabled"
  | "write_execution_disabled"
  | "tool_not_implemented"
  | "tool_unknown";

export type LocalMcpTransportResponseEnvelope =
  | LocalMcpTransportPreflightResponseEnvelope
  | LocalMcpTransportRejectedResponseEnvelope;

export type LocalMcpTransportAuthPreviewResponseEnvelope =
  | LocalMcpTransportAuthPreviewExecutedResponseEnvelope
  | LocalMcpTransportAuthPreviewPreflightResponseEnvelope
  | LocalMcpTransportRejectedResponseEnvelope;

export interface LocalMcpTransportPreflightResponseEnvelope {
  id: string;
  transport: LocalMcpTransportKind;
  runtime_mode: LocalMcpRuntimeMode;
  executed: false;
  reason: "transport_not_enabled" | "capability_token_not_enabled";
  error_code: LocalMcpErrorCode;
  preflight: LocalMcpTransportPreflight;
  request_trace: LocalMcpTransportRequestTrace;
  result?: never;
}

export interface LocalMcpTransportAuthPreviewPreflightResponseEnvelope
  extends LocalMcpTransportPreflightResponseEnvelope {
  capability_token_validation_preview: LocalMcpCapabilityTokenValidationPreview;
  capability_token_auth_gate_preview: LocalMcpCapabilityTokenAuthGatePreview;
  capability_token_acceptance_gate_preview: LocalMcpCapabilityTokenAcceptanceGatePreview;
  transport_dispatch_gate_preview: LocalMcpTransportDispatchGatePreview;
  journaled_preflight?: LocalMcpTransportJournaledPreflight;
}

export interface LocalMcpTransportAuthPreviewExecutedResponseEnvelope {
  id: string;
  transport: LocalMcpTransportKind;
  runtime_mode: LocalMcpRuntimeMode;
  executed: true;
  reason: "dispatch_allowed";
  preflight: LocalMcpTransportPreflight;
  request_trace: LocalMcpTransportRequestTrace;
  capability_token_validation_preview: LocalMcpCapabilityTokenValidationPreview;
  capability_token_auth_gate_preview: LocalMcpCapabilityTokenAuthGatePreview;
  capability_token_acceptance_gate_preview: LocalMcpCapabilityTokenAcceptanceGatePreview;
  transport_dispatch_gate_preview: LocalMcpTransportDispatchGatePreview;
  result: LocalMcpToolExecutionResponse;
  error_code?: never;
  journaled_preflight?: never;
}

export interface LocalMcpTransportRejectedResponseEnvelope {
  id: string;
  transport?: LocalMcpTransportKind;
  runtime_mode: LocalMcpRuntimeMode;
  executed: false;
  reason: "request_invalid";
  error_code: LocalMcpErrorCode;
  validation: LocalMcpTransportEnvelopeValidation;
  request_trace: LocalMcpTransportRejectedRequestTrace;
  result?: never;
}

export interface LocalMcpToolExecutionResponse {
  tool: LocalMcpReadOnlyToolName;
  read_only: true;
  runtime_mode: LocalMcpRuntimeMode;
  result: unknown;
}

export interface LocalMcpTransportRequestTrace {
  schema_version: 1;
  id: string;
  transport: LocalMcpTransportKind;
  tool: string;
  runtime_mode: LocalMcpRuntimeMode;
  stage: "transport_preflight" | "transport_dispatch";
  status: "preflight_only" | "dispatch_executed";
  blocking_stage: LocalMcpTransportRequestBlockingStage | null;
  blocking_priority: "primary" | null;
  secondary_blocking_stage?: "tool_guard";
  executed: boolean;
  persisted: false;
  journaled: false;
  storage: "response_only";
  retention: "none";
  raw_values_stored: false;
  value_policy: "presence_only";
  input_present: boolean;
  confirmation_text_present: boolean;
  capability_token_present: boolean;
  redacted_fields: LocalMcpTransportRequestTraceRedactedField[];
  reason: "transport_not_enabled" | "capability_token_not_enabled" | "dispatch_allowed";
  error_code?: LocalMcpErrorCode;
  tool_error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportRejectedRequestTrace {
  schema_version: 1;
  id: string;
  transport?: LocalMcpTransportKind;
  tool: string;
  runtime_mode: LocalMcpRuntimeMode;
  stage: "envelope_validation";
  status: "request_invalid";
  blocking_stage: "envelope_validation";
  blocking_priority: "primary";
  executed: false;
  persisted: false;
  journaled: false;
  storage: "response_only";
  retention: "none";
  raw_values_stored: false;
  value_policy: "presence_only";
  input_present: boolean;
  confirmation_text_present: boolean;
  capability_token_present: boolean;
  redacted_fields: LocalMcpTransportRequestTraceRedactedField[];
  reason: "request_invalid";
  validation_reason?: LocalMcpTransportEnvelopeValidationReason;
  error_code: LocalMcpErrorCode;
}

export type LocalMcpTransportRequestTraceRedactedField =
  | "input"
  | "confirmation_text"
  | "capability_token";

export type LocalMcpTransportRequestTraceStage =
  | "envelope_validation"
  | "transport_preflight";

export type LocalMcpTransportRequestBlockingStage =
  | "envelope_validation"
  | "capability_token"
  | "transport"
  | "tool_guard";

export interface LocalMcpTransportRequestTracePolicy {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  status: "preview";
  persisted: false;
  journaled: false;
  storage: "response_only";
  retention: "none";
  raw_values_stored: false;
  value_policy: "presence_only";
  stages: LocalMcpTransportRequestTraceStage[];
  blocking_stages: LocalMcpTransportRequestBlockingStage[];
  primary_blocking_stages: LocalMcpTransportRequestBlockingStage[];
  secondary_blocking_stages: LocalMcpTransportRequestBlockingStage[];
  attaches_to: LocalMcpTransportRequestTraceAttachment[];
  redacted_fields: LocalMcpTransportRequestTraceRedactedField[];
  reason: "operation_journal_not_enabled";
}

export interface LocalMcpTransportRequestTraceContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  status: "preview";
  storage: "response_only";
  retention: "none";
  persisted: false;
  journaled: false;
  raw_values_stored: false;
  value_policy: "presence_only";
  stages: LocalMcpTransportRequestTraceStage[];
  primary_blocking_stages: LocalMcpTransportRequestBlockingStage[];
  secondary_blocking_stages: LocalMcpTransportRequestBlockingStage[];
  attaches_to: LocalMcpTransportRequestTraceAttachment[];
}

export type LocalMcpTransportEnvelopeRequestField =
  | "id"
  | "transport"
  | "tool"
  | "request.input"
  | "request.confirmation_text"
  | "request.capability_token";

export type LocalMcpTransportEnvelopeResponseField =
  | "id"
  | "transport"
  | "runtime_mode"
  | "executed"
  | "reason"
  | "error_code"
  | "preflight"
  | "validation"
  | "request_trace";

export interface LocalMcpTransportEnvelopeContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  transports: LocalMcpTransportKind[];
  transport_available: false;
  execution_enabled: false;
  request_fields: LocalMcpTransportEnvelopeRequestField[];
  response_fields: LocalMcpTransportEnvelopeResponseField[];
  validation_reasons: LocalMcpTransportEnvelopeValidationReason[];
  response_reasons: Array<
    "request_invalid" | "transport_not_enabled" | "capability_token_not_enabled"
  >;
  stable_error_codes: LocalMcpErrorCode[];
  trace_attached: true;
  result_returned: false;
}

export type LocalMcpTransportErrorReason =
  | LocalMcpTransportEnvelopeValidationReason
  | "transport_not_enabled"
  | "capability_token_missing"
  | "capability_token_not_enabled"
  | "write_execution_disabled"
  | "tool_not_implemented"
  | "tool_unknown";

export interface LocalMcpTransportErrorContractEntry {
  error_code: LocalMcpErrorCode;
  stage: LocalMcpTransportRequestTraceStage;
  blocking_stage: LocalMcpTransportRequestBlockingStage;
  blocking_priority: "primary" | "secondary";
  reasons: LocalMcpTransportErrorReason[];
}

export interface LocalMcpTransportErrorContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  stable_errors: LocalMcpTransportErrorContractEntry[];
  default_invalid_request_error: "invalid_request";
  default_preflight_error: "transport_disabled";
  tool_error_field: "tool_error_code";
}

export interface LocalMcpTransportResultContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  helper_read_results_enabled: true;
  transport_results_enabled: false;
  transport_result_returned: false;
  invalid_envelope_result_returned: false;
  preflight_result_returned: false;
  result_field: "result";
  blocked_result_sources: LocalMcpTransportRequestTraceStage[];
  successful_helper_tools: number;
  blocked_write_tools: number;
  not_implemented_tools: number;
}

export type LocalMcpTransportRetryCondition =
  | "fix_invalid_envelope"
  | "enable_transport"
  | "issue_capability_token"
  | "enable_write_execution"
  | "implement_tool";

export interface LocalMcpTransportRetryContractEntry {
  error_code: LocalMcpErrorCode;
  retryable: boolean;
  automatic_retry: false;
  condition: LocalMcpTransportRetryCondition;
}

export interface LocalMcpTransportRetryContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  automatic_retries_enabled: false;
  retry_after_ms: null;
  retry_entries: LocalMcpTransportRetryContractEntry[];
}

export type LocalMcpTransportIdempotencyScope = "request_envelope" | "write_execution";

export interface LocalMcpTransportIdempotencyContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  request_id_required: true;
  replay_cache_enabled: false;
  write_idempotency_enabled: false;
  operation_journal_required: true;
  idempotency_key_field: "id";
  protected_scopes: LocalMcpTransportIdempotencyScope[];
  duplicate_policy: "not_enforced_until_transport_enabled";
  reason: "operation_journal_not_enabled";
}

export type LocalMcpTransportSideEffectBoundary =
  | "envelope_validation"
  | "transport_preflight"
  | "helper_read_dispatch"
  | "write_execution"
  | "operation_journal"
  | "replay_cache";

export interface LocalMcpTransportSideEffectContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  transport_side_effects_enabled: false;
  helper_read_side_effects_enabled: false;
  write_side_effects_enabled: false;
  persistence_enabled: false;
  journal_writes_enabled: false;
  replay_cache_writes_enabled: false;
  boundaries: LocalMcpTransportSideEffectBoundary[];
  non_mutating_boundaries: LocalMcpTransportSideEffectBoundary[];
  mutating_boundaries: [];
}

export interface LocalMcpTransportAdapterContractEntry {
  transport: LocalMcpTransportKind;
  runtime_mode: LocalMcpRuntimeMode;
  adapter_available: false;
  bindable: false;
  accepts_requests: false;
  emits_responses: false;
  requires_capability_token: true;
  requires_operation_journal: true;
  reason: "transport_not_enabled";
}

export interface LocalMcpTransportAdapterContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  adapters_enabled: false;
  adapters: LocalMcpTransportAdapterContractEntry[];
}

export type LocalMcpTransportListenerConfigField =
  | "transport"
  | "stdio.stdin"
  | "stdio.stdout"
  | "stdio.stderr"
  | "sse.endpoint"
  | "sse.method"
  | "sse.headers"
  | "capability_token";

export interface LocalMcpTransportListenerConfigContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  listener_configuration_enabled: false;
  transports: LocalMcpTransportKind[];
  config_fields: LocalMcpTransportListenerConfigField[];
  requires_capability_token: true;
  requires_operation_journal: true;
  stdio_stream_binding_enabled: false;
  sse_endpoint_registration_enabled: false;
  reason: "transport_not_enabled";
}

export interface LocalMcpTransportAdapterRequestPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  adapter_available: false;
  bindable: false;
  request_accepted: false;
  response_emitted: false;
  executed: false;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export interface LocalMcpTransportAdapterBindPreflight {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  adapter_available: false;
  bindable: false;
  bind_attempted: false;
  bound: false;
  listener_started: false;
  request_loop_started: false;
  accepts_requests: false;
  emits_responses: false;
  requires_capability_token: true;
  requires_operation_journal: true;
  transport_readiness: LocalMcpTransportReadiness;
  capability_token_auth_readiness: LocalMcpCapabilityTokenAuthReadiness;
  operation_journal_readiness: LocalMcpOperationJournalReadiness;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export interface LocalMcpTransportAdapterBindPreflightWithAcceptance
  extends LocalMcpTransportAdapterBindPreflight {
  listener_binding_allowed: false;
  capability_token_acceptance_gate_preview: LocalMcpCapabilityTokenAcceptanceGatePreview;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  bind_blocking_gate: LocalMcpTransportDispatchGateBlockingGate;
}

export interface LocalMcpTransportListenerConfigPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  listener_configuration_enabled: false;
  listener_configured: false;
  stdio_stream_binding_enabled: false;
  stdio_stdin_required: boolean;
  stdio_stdout_required: boolean;
  stdio_stderr_required: boolean;
  stdio_streams_attached: false;
  sse_endpoint_registration_enabled: false;
  sse_endpoint_required: boolean;
  sse_method: "GET" | null;
  sse_headers_required: boolean;
  sse_endpoint_registered: false;
  endpoint: null;
  bind_preflight: LocalMcpTransportAdapterBindPreflight;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export type LocalMcpTransportListenerConfigValidationReason =
  | "transport_unsupported"
  | "stdio_streams_required"
  | "sse_endpoint_required"
  | "sse_method_invalid"
  | "listener_configuration_not_enabled";

export interface LocalMcpTransportListenerConfigRequest {
  transport: string;
  tool?: string;
  stdio?: {
    stdin?: unknown;
    stdout?: unknown;
    stderr?: unknown;
  };
  sse?: {
    endpoint?: string;
    method?: string;
    headers?: Record<string, string>;
  };
  capability_token?: string;
}

export interface LocalMcpTransportListenerConfigValidation {
  valid: false;
  transport?: LocalMcpTransportKind;
  reason: LocalMcpTransportListenerConfigValidationReason;
  error_code: LocalMcpErrorCode;
  listener_configuration_enabled: false;
}

export interface LocalMcpTransportListenerConfigValidationPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport?: LocalMcpTransportKind;
  listener_configuration_enabled: false;
  listener_configured: false;
  validation: LocalMcpTransportListenerConfigValidation;
  config_preview?: LocalMcpTransportListenerConfigPreview;
  bind_preflight?: LocalMcpTransportAdapterBindPreflightWithAcceptance;
  listener_bind_descriptor?: LocalMcpTransportListenerBindDescriptor;
  startup_preview?: LocalMcpTransportAdapterStartupWithAcceptancePreview;
  request_loop_registration?: LocalMcpTransportRequestLoopRegistrationPreview;
  reason: LocalMcpTransportListenerConfigValidationReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpTransportListenerBindDescriptor {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  listener_bind_descriptor_available: true;
  listener_binding_allowed: false;
  bind_attempted: false;
  listener_started: false;
  request_loop_started: false;
  stdio_streams_attached: false;
  sse_endpoint_registered: false;
  endpoint: string | null;
  stdio_streams_present: boolean;
  sse_endpoint_present: boolean;
  bind_preflight: LocalMcpTransportAdapterBindPreflightWithAcceptance;
  bind_blocking_gate: LocalMcpTransportDispatchGateBlockingGate;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpTransportAdapterStartupPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  startup_attempted: false;
  adapter_available: false;
  listener_configured: false;
  listener_started: false;
  request_loop_started: false;
  stdio_streams_attached: false;
  sse_endpoint_registered: false;
  endpoint: null;
  bind_preflight: LocalMcpTransportAdapterBindPreflight;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export interface LocalMcpTransportAdapterStartupWithAcceptancePreview
  extends Omit<LocalMcpTransportAdapterStartupPreview, "bind_preflight"> {
  bind_preflight: LocalMcpTransportAdapterBindPreflightWithAcceptance;
  listener_binding_allowed: false;
  listener_bind_descriptor?: LocalMcpTransportListenerBindDescriptor;
  capability_token_acceptance_gate_preview: LocalMcpCapabilityTokenAcceptanceGatePreview;
}

export interface LocalMcpTransportRequestLoopRegistrationPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  request_loop_registration_enabled: false;
  request_loop_registered: false;
  request_loop_started: false;
  accepts_requests: false;
  emits_responses: false;
  handler_available: false;
  listener_binding_allowed: false;
  startup_preview: LocalMcpTransportAdapterStartupWithAcceptancePreview;
  bind_preflight: LocalMcpTransportAdapterBindPreflightWithAcceptance;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"];
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpTransportRequestLoopDispatchPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  request_loop_dispatch_enabled: boolean;
  request_loop_registered: false;
  request_loop_started: false;
  dispatch_attempted: boolean;
  request_accepted: boolean;
  handler_invoked: boolean;
  response_emitted: false;
  executed: boolean;
  journaled: boolean;
  registration_preview: LocalMcpTransportRequestLoopRegistrationPreview;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  handler_response?: LocalMcpTransportAuthPreviewResponseEnvelope;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportHandlerInvocationPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  handler_invocation_enabled: boolean;
  handler_available: boolean;
  request_loop_started: false;
  request_accepted: boolean;
  handler_invoked: boolean;
  response_emitted: false;
  executed: boolean;
  journaled: boolean;
  request_loop_dispatch: LocalMcpTransportRequestLoopDispatchPreview;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportHandlerResponseFramePreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  response_frame_enabled: boolean;
  handler_available: boolean;
  handler_invoked: boolean;
  handler_result_available: boolean;
  response_envelope_created: boolean;
  response_emitted: false;
  executed: boolean;
  journaled: boolean;
  response_frame: LocalMcpTransportAuthPreviewResponseEnvelope | null;
  response_envelope: LocalMcpTransportAuthPreviewResponseEnvelope | null;
  handler_invocation: LocalMcpTransportHandlerInvocationPreview;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportResponseEmissionPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  response_emission_enabled: boolean;
  transport_output_available: boolean;
  response_frame_available: boolean;
  response_envelope_created: boolean;
  response_emitted: boolean;
  stdio_stdout_written: boolean;
  sse_event_sent: boolean;
  executed: boolean;
  journaled: boolean;
  response_frame: LocalMcpTransportAuthPreviewResponseEnvelope | null;
  response_envelope: LocalMcpTransportAuthPreviewResponseEnvelope | null;
  handler_response_frame: LocalMcpTransportHandlerResponseFramePreview;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportResponseCompletionPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  response_completion_enabled: boolean;
  response_emitted: boolean;
  response_completed: boolean;
  request_completed: boolean;
  lifecycle_completed: false;
  cleanup_scheduled: false;
  executed: boolean;
  journaled: boolean;
  response_emission: LocalMcpTransportResponseEmissionPreview;
  response_envelope: LocalMcpTransportAuthPreviewResponseEnvelope | null;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportErrorEnvelopeFramePreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  error_envelope_framing_enabled: false;
  error_detected: true;
  error_envelope_created: false;
  error_envelope_emitted: false;
  response_completed: false;
  request_completed: false;
  lifecycle_completed: false;
  executed: false;
  journaled: false;
  error_envelope: null;
  response_completion: LocalMcpTransportResponseCompletionPreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"];
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpTransportErrorEmissionPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  error_emission_enabled: false;
  error_envelope_available: false;
  error_envelope_emitted: false;
  stdio_stderr_written: false;
  sse_error_event_sent: false;
  response_completed: false;
  request_completed: false;
  lifecycle_completed: false;
  executed: false;
  journaled: false;
  error_envelope: null;
  error_envelope_frame: LocalMcpTransportErrorEnvelopeFramePreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"];
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpTransportLifecycleAuditPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  lifecycle_audit_enabled: boolean;
  cleanup_scheduling_enabled: false;
  response_path_completed: boolean;
  error_path_completed: false;
  response_completed: boolean;
  request_completed: boolean;
  lifecycle_completed: false;
  cleanup_scheduled: false;
  lifecycle_audit_recorded: false;
  executed: boolean;
  journaled: boolean;
  response_completion: LocalMcpTransportResponseCompletionPreview;
  error_emission: LocalMcpTransportErrorEmissionPreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportRequestCloseoutPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  request_closeout_enabled: boolean;
  cleanup_queue_handoff_enabled: false;
  closeout_attempted: boolean;
  cleanup_queue_handoff_attempted: false;
  cleanup_queue_enqueued: false;
  response_completed: boolean;
  request_completed: boolean;
  lifecycle_completed: false;
  cleanup_scheduled: false;
  lifecycle_audit_recorded: false;
  executed: boolean;
  journaled: boolean;
  lifecycle_audit: LocalMcpTransportLifecycleAuditPreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportTerminalCleanupPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  terminal_cleanup_enabled: boolean;
  adapter_closeout_handoff_enabled: false;
  terminal_cleanup_attempted: boolean;
  adapter_closeout_handoff_attempted: false;
  adapter_closeout_handed_off: false;
  cleanup_queue_enqueued: false;
  request_completed: boolean;
  lifecycle_completed: false;
  cleanup_scheduled: false;
  lifecycle_audit_recorded: false;
  executed: boolean;
  journaled: boolean;
  request_closeout: LocalMcpTransportRequestCloseoutPreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportHandlerTerminalStatePreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  handler_terminal_state_enabled: boolean;
  terminal_state_recorded: false;
  handler_available: boolean;
  handled: boolean;
  request_accepted: boolean;
  response_emitted: boolean;
  request_completed: boolean;
  lifecycle_completed: false;
  cleanup_scheduled: false;
  cleanup_queue_enqueued: false;
  executed: boolean;
  journaled: boolean;
  terminal_cleanup: LocalMcpTransportTerminalCleanupPreview;
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportEnablementReadinessPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  transport_enablement_readiness_enabled: boolean;
  transport_enablement_allowed: boolean;
  stdio_enablement_allowed: boolean;
  sse_enablement_allowed: boolean;
  handler_terminal_state_ready: boolean;
  capability_token_acceptance_ready: boolean;
  operation_journal_ready: boolean;
  confirmed_write_ready: false;
  listener_binding_ready: boolean;
  request_loop_ready: boolean;
  response_pipeline_ready: boolean;
  cleanup_pipeline_ready: boolean;
  executed: boolean;
  journaled: boolean;
  transport_readiness: LocalMcpTransportReadiness;
  handler_terminal_state: LocalMcpTransportHandlerTerminalStatePreview;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  blocking_gates: LocalMcpTransportDispatchGateBlockingGate[];
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"] | null;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpCapabilityTokenAcceptanceReadinessPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  acceptance_persistence_enabled: false;
  persistence_allowed: false;
  can_persist_acceptance: boolean;
  token_record_found: boolean;
  token_checks_passed: boolean;
  token_accepted: boolean;
  accepted_token_persisted: boolean;
  accepted_at: string | null;
  raw_token_stored: false;
  requires_existing_record: true;
  requires_token_checks_passed: true;
  requires_unexpired_token: true;
  requires_unrevoked_token: true;
  requires_accepted_false: true;
  requires_raw_token_storage_false: true;
  accepted_at_field_required: true;
  listener_binding_allowed: false;
  transport_dispatch_allowed: false;
  executed: false;
  journaled: false;
  record?: LocalMcpCapabilityTokenRecord;
  acceptance_gate: LocalMcpCapabilityTokenAcceptanceGatePreview;
  blocking_gates: LocalMcpCapabilityTokenAcceptanceGateBlockingGate[];
  first_blocking_stage: LocalMcpTransportDispatchGateBlockingGate["gate"];
  reason: LocalMcpCapabilityTokenAcceptanceGateReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpTransportAdapterShutdownPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  shutdown_attempted: false;
  adapter_available: false;
  listener_was_started: false;
  request_loop_was_started: false;
  listener_closed: false;
  request_loop_stopped: false;
  stdio_streams_detached: false;
  sse_endpoint_unregistered: false;
  pending_requests_drained: false;
  error_state_recorded: false;
  startup_preview: LocalMcpTransportAdapterStartupPreview;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export interface LocalMcpTransportAdapterRuntimeStatePreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  adapter_available: false;
  runtime_active: false;
  bound: false;
  listener_started: false;
  request_loop_started: false;
  request_accepted: false;
  response_emitted: false;
  lifecycle_completed: false;
  executed: false;
  journaled: false;
  bind_preflight: LocalMcpTransportAdapterBindPreflightWithAcceptance;
  startup_preview: LocalMcpTransportAdapterStartupWithAcceptancePreview;
  shutdown_preview: LocalMcpTransportAdapterShutdownPreview;
  request_preview: LocalMcpTransportAdapterRequestPreview;
  response_preview: LocalMcpTransportAdapterResponsePreview;
  lifecycle_preview: LocalMcpTransportAdapterLifecyclePreview;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export interface LocalMcpTransportHandlerSkeletonPreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  handler_available: boolean;
  handler_started: false;
  listener_binding_allowed: false;
  request_loop_started: false;
  request_accepted: boolean;
  response_emitted: boolean;
  executed: boolean;
  journaled: boolean;
  bind_preflight: LocalMcpTransportAdapterBindPreflightWithAcceptance;
  startup_preview: LocalMcpTransportAdapterStartupWithAcceptancePreview;
  request_loop_registration: LocalMcpTransportRequestLoopRegistrationPreview;
  request_loop_dispatch: LocalMcpTransportRequestLoopDispatchPreview;
  handler_invocation: LocalMcpTransportHandlerInvocationPreview;
  handler_response_frame: LocalMcpTransportHandlerResponseFramePreview;
  response_emission: LocalMcpTransportResponseEmissionPreview;
  response_completion: LocalMcpTransportResponseCompletionPreview;
  error_envelope_frame: LocalMcpTransportErrorEnvelopeFramePreview;
  error_emission: LocalMcpTransportErrorEmissionPreview;
  lifecycle_audit: LocalMcpTransportLifecycleAuditPreview;
  request_closeout: LocalMcpTransportRequestCloseoutPreview;
  terminal_cleanup: LocalMcpTransportTerminalCleanupPreview;
  handler_terminal_state: LocalMcpTransportHandlerTerminalStatePreview;
  transport_enablement_readiness: LocalMcpTransportEnablementReadinessPreview;
  capability_token_acceptance_readiness: LocalMcpCapabilityTokenAcceptanceReadinessPreview;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  preflight: LocalMcpTransportPreflight;
  operation_journal_gate_preview: LocalMcpConfirmedWriteJournalGatePreview;
  operation_journal_replay_preview: LocalMcpOperationJournalReplayPreview;
  runtime_state: LocalMcpTransportAdapterRuntimeStatePreview;
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportHandlerSkeletonResponse {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  handler_available: false;
  handled: false;
  response_emitted: false;
  executed: false;
  journaled: false;
  response_envelope: null;
  skeleton: LocalMcpTransportHandlerSkeletonPreview;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export interface LocalMcpTransportAdapterResponsePreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  adapter_available: false;
  request_accepted: false;
  response_emitted: false;
  executed: false;
  response_envelope: null;
  request_preview: LocalMcpTransportAdapterRequestPreview;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export interface LocalMcpTransportAdapterLifecycleStagePreview {
  stage: LocalMcpTransportLifecycleStage;
  enabled: false;
  completed: false;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export interface LocalMcpTransportAdapterLifecyclePreview {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  lifecycle_enabled: false;
  adapter_available: false;
  request_accepted: false;
  response_emitted: false;
  executed: false;
  journaled: false;
  completed: false;
  first_blocking_stage: "configure";
  stages: LocalMcpTransportAdapterLifecycleStagePreview[];
  request_preview: LocalMcpTransportAdapterRequestPreview;
  response_preview: LocalMcpTransportAdapterResponsePreview;
  reason: "transport_not_enabled";
  error_code: "transport_disabled";
}

export type LocalMcpTransportLifecycleStage =
  | "configure"
  | "bind"
  | "authenticate"
  | "preflight"
  | "dispatch"
  | "respond"
  | "journal"
  | "shutdown";

export interface LocalMcpTransportLifecycleContract {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  lifecycle_enabled: false;
  enabled_stages: [];
  disabled_stages: LocalMcpTransportLifecycleStage[];
  first_enabled_stage: null;
  terminal_stage: "shutdown";
  reason: "transport_not_enabled";
}

export type LocalMcpTransportRequestTraceAttachment =
  | "valid_preflight_response"
  | "invalid_envelope_response";

export interface LocalMcpToolManifestEntry {
  name: LocalMcpReadOnlyToolName;
  read_only: true;
  runtime_modes: LocalMcpRuntimeMode[];
}

export type LocalMcpBlockedToolReason = "write_not_enabled" | "not_implemented";

export interface LocalMcpBlockedToolManifestEntry {
  name: string;
  reason: LocalMcpBlockedToolReason;
}

export type LocalMcpToolAvailability =
  | "executable"
  | "confirmation_required"
  | "not_implemented"
  | "unknown";

export interface LocalMcpToolSafetyPlan {
  name: string;
  availability: LocalMcpToolAvailability;
  read_only: boolean;
  runtime_mode: LocalMcpRuntimeMode;
  requires_confirmation: boolean;
  reason?: LocalMcpBlockedToolReason | "unknown_tool";
}

export type LocalMcpToolExecutionGuardRejectionReason =
  | "write_execution_disabled"
  | "not_implemented"
  | "unknown_tool";

export interface LocalMcpToolExecutionGuard {
  tool: string;
  executable: boolean;
  runtime_mode: LocalMcpRuntimeMode;
  plan: LocalMcpToolSafetyPlan;
  confirmation_challenge?: LocalMcpConfirmationChallenge;
  confirmation_validation?: LocalMcpConfirmationValidationResult;
  rejection_reason?: LocalMcpToolExecutionGuardRejectionReason;
}

export interface LocalMcpConfirmationChallenge {
  tool: string;
  runtime_mode: LocalMcpRuntimeMode;
  required: true;
  confirmation_text: string;
  reason: "write_not_enabled";
}

export interface LocalMcpConfirmationValidationResult {
  tool: string;
  valid: boolean;
  runtime_mode: LocalMcpRuntimeMode;
  reason?: "confirmation_not_required" | "confirmation_mismatch";
}

export interface LocalMcpRuntimeManifest {
  schema_version: 1;
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  read_only: true;
  tool_count: number;
  tools: LocalMcpToolManifestEntry[];
  blocked_tools: LocalMcpBlockedToolManifestEntry[];
  planned_transports: LocalMcpTransportStatus[];
  capability_token: LocalMcpCapabilityTokenPolicy;
  capability_token_lifecycle: LocalMcpCapabilityTokenLifecyclePolicy;
  capability_token_auth_readiness: LocalMcpCapabilityTokenAuthReadiness;
  capability_token_auth_contract: LocalMcpCapabilityTokenAuthContract;
  operation_journal: LocalMcpOperationJournalPolicy;
  operation_journal_contract: LocalMcpOperationJournalContract;
  operation_journal_record_contract: LocalMcpOperationJournalRecordContract;
  transport_capabilities: LocalMcpTransportCapabilitySummary;
  transport_execution_contract: LocalMcpTransportExecutionContract;
  transport_envelope_contract: LocalMcpTransportEnvelopeContract;
  transport_dispatch_contract: LocalMcpTransportDispatchContract;
  transport_error_contract: LocalMcpTransportErrorContract;
  transport_result_contract: LocalMcpTransportResultContract;
  transport_retry_contract: LocalMcpTransportRetryContract;
  transport_idempotency_contract: LocalMcpTransportIdempotencyContract;
  transport_side_effect_contract: LocalMcpTransportSideEffectContract;
  transport_adapter_contract: LocalMcpTransportAdapterContract;
  transport_listener_config_contract: LocalMcpTransportListenerConfigContract;
  transport_lifecycle_contract: LocalMcpTransportLifecycleContract;
  write_block_contract: LocalMcpWriteBlockContract;
  trace_contract: LocalMcpTransportRequestTraceContract;
  request_trace_policy: LocalMcpTransportRequestTracePolicy;
}

export interface LocalMcpCapabilityTokenPolicy {
  status: LocalMcpCapabilityTokenStatus;
  required_for_transports: LocalMcpTransportKind[];
  reason: "capability_token_not_enabled";
}

export interface LocalMcpCapabilityTokenLifecyclePolicy {
  status: LocalMcpCapabilityTokenLifecycleStatus;
  token_issued: false;
  storage: "local_vault_metadata";
  token_format: "opaque_bearer";
  fingerprint_algorithm: "sha256";
  token_ttl_seconds: 3600;
  expiry_required: true;
  revocation_supported: false;
  reason: "capability_token_not_enabled";
  planned_fields: LocalMcpCapabilityTokenLifecycleField[];
}

export interface LocalMcpCapabilityTokenAuthReadiness {
  runtime_mode: LocalMcpRuntimeMode;
  status: "disabled";
  transport_ready: false;
  token_accepted: false;
  token_issued: false;
  token_revoked: false;
  required_for_transports: LocalMcpTransportKind[];
  lifecycle: LocalMcpCapabilityTokenLifecyclePolicy;
  request_challenge: LocalMcpCapabilityTokenRequestChallenge;
  preview_capabilities: LocalMcpCapabilityTokenPreviewCapability[];
  blocking_gates: LocalMcpCapabilityTokenAuthBlockingGate[];
}

export interface LocalMcpCapabilityTokenAuthContract {
  runtime_mode: LocalMcpRuntimeMode;
  status: "disabled";
  transport_ready: false;
  token_accepted: false;
  token_issued: false;
  token_revoked: false;
  required_for_transports: LocalMcpTransportKind[];
  lifecycle_status: LocalMcpCapabilityTokenLifecycleStatus;
  token_format: "opaque_bearer";
  fingerprint_algorithm: "sha256";
  token_ttl_seconds: 3600;
  expiry_required: true;
  revocation_supported: false;
  preview_capabilities: LocalMcpCapabilityTokenPreviewCapability[];
  blocking_gates: LocalMcpCapabilityTokenAuthBlockingGate[];
}

export type LocalMcpCapabilityTokenPreviewCapability =
  | "request_validation"
  | "issuance_preview"
  | "expiry_preview"
  | "revocation_preview"
  | "acceptance_gate_preview";

export interface LocalMcpCapabilityTokenAuthBlockingGate {
  gate: "transport" | "token_issuance" | "token_acceptance" | "token_revocation";
  status: "disabled";
  reason: "transport_not_enabled" | "capability_token_not_enabled";
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpCapabilityTokenRequestChallenge {
  runtime_mode: LocalMcpRuntimeMode;
  status: LocalMcpCapabilityTokenStatus;
  token_issued: false;
  required_for_transports: LocalMcpTransportKind[];
  reason: "capability_token_not_enabled";
  confirmation_text: string;
  planned_fields: LocalMcpCapabilityTokenRequestField[];
}

export type LocalMcpCapabilityTokenRequestField =
  | "request_id"
  | "created_at"
  | "runtime_mode"
  | "transport"
  | "scope"
  | "vault_fingerprint"
  | "confirmation_text";

export type LocalMcpCapabilityTokenLifecycleField =
  | "token_id"
  | "issued_at"
  | "expires_at"
  | "runtime_mode"
  | "transport"
  | "scope"
  | "vault_fingerprint"
  | "token_fingerprint"
  | "revoked_at";

export interface LocalMcpCapabilityTokenRequest {
  request_id: string;
  created_at: string;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  scope: LocalMcpCapabilityTokenRequestScope;
  vault_fingerprint: string;
  confirmation_text: string;
}

export interface LocalMcpCapabilityTokenRequestValidation {
  valid: false;
  token_issued: false;
  reason: LocalMcpCapabilityTokenRequestValidationReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpCapabilityTokenIssuancePreview {
  token_issued: false;
  runtime_mode: LocalMcpRuntimeMode;
  reason: LocalMcpCapabilityTokenRequestValidationReason;
  error_code: LocalMcpErrorCode;
  request_validation: LocalMcpCapabilityTokenRequestValidation;
  planned_record?: LocalMcpCapabilityTokenRecordPreview;
}

export interface LocalMcpCapabilityTokenRecordPreview {
  token_id: string;
  issued_at: string;
  expires_at: string;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  scope: LocalMcpCapabilityTokenRequestScope;
  vault_fingerprint: string;
  token_fingerprint: string | null;
  revoked_at: string | null;
}

export type LocalMcpCapabilityTokenRecordReason =
  | "capability_token_not_enabled"
  | "capability_token_issued"
  | "capability_token_accepted"
  | "capability_token_revoked";

export interface LocalMcpCapabilityTokenRecord extends LocalMcpCapabilityTokenRecordPreview {
  token_fingerprint: string | null;
  persisted: true;
  accepted: boolean;
  accepted_at?: string | null;
  raw_token_stored: false;
  reason: LocalMcpCapabilityTokenRecordReason;
}

export interface LocalMcpCapabilityTokenRecordPersistResult {
  record_persisted: boolean;
  reason?: LocalMcpCapabilityTokenRequestValidationReason;
  record?: LocalMcpCapabilityTokenRecord;
  records: LocalMcpCapabilityTokenRecord[];
}

export type LocalMcpCapabilityTokenIssueFailureReason =
  | LocalMcpCapabilityTokenRequestValidationReason
  | "token_generation_unavailable"
  | "token_fingerprint_unavailable";

export interface LocalMcpCapabilityTokenIssueResult {
  token_issued: boolean;
  reason?: LocalMcpCapabilityTokenIssueFailureReason;
  raw_token?: string;
  record?: LocalMcpCapabilityTokenRecord;
  records: LocalMcpCapabilityTokenRecord[];
}

export type LocalMcpCapabilityTokenRevokeFailureReason =
  | LocalMcpCapabilityTokenRevocationValidationReason
  | "token_record_not_found"
  | "token_fingerprint_mismatch";

export interface LocalMcpCapabilityTokenRevokeResult {
  token_revoked: boolean;
  reason?: LocalMcpCapabilityTokenRevokeFailureReason;
  record?: LocalMcpCapabilityTokenRecord;
  records: LocalMcpCapabilityTokenRecord[];
}

export interface LocalMcpCapabilityTokenAcceptOptions {
  acceptance_enabled?: boolean;
  accepted_at?: string;
}

export type LocalMcpCapabilityTokenAcceptFailureReason =
  | LocalMcpCapabilityTokenAcceptanceGateReason
  | "accepted_at_invalid"
  | "accepted_token_already_persisted";

export interface LocalMcpCapabilityTokenAcceptResult {
  token_accepted: boolean;
  acceptance_enabled: boolean;
  reason?: LocalMcpCapabilityTokenAcceptFailureReason;
  record?: LocalMcpCapabilityTokenRecord;
  records: LocalMcpCapabilityTokenRecord[];
  acceptance_readiness: LocalMcpCapabilityTokenAcceptanceReadinessPreview;
}

export type LocalMcpCapabilityTokenRecordLookupReason = "token_record_not_found";

export type LocalMcpCapabilityTokenValidationPreviewReason =
  | "capability_token_missing"
  | "token_fingerprint_unavailable"
  | "token_record_not_found"
  | "token_fingerprint_mismatch"
  | "capability_token_revoked"
  | "capability_token_expired"
  | "capability_token_not_enabled";

export type LocalMcpCapabilityTokenAuthGateFailureReason =
  | "capability_token_missing"
  | "token_fingerprint_unavailable"
  | "token_record_not_found"
  | "token_fingerprint_mismatch"
  | "capability_token_revoked"
  | "capability_token_expired"
  | "capability_token_not_enabled"
  | "transport_not_enabled"
  | "operation_journal_not_enabled";

export type LocalMcpCapabilityTokenAcceptanceGateReason =
  | "capability_token_accepted"
  | "capability_token_acceptance_not_enabled"
  | LocalMcpCapabilityTokenAuthGateFailureReason;

export interface LocalMcpCapabilityTokenValidationPreview {
  token_accepted: boolean;
  token_present: boolean;
  token_record_found: boolean;
  fingerprint_matched: boolean;
  expired: boolean;
  reason: LocalMcpCapabilityTokenValidationPreviewReason;
  token_fingerprint?: string;
  record?: LocalMcpCapabilityTokenRecord;
  expiry_preview?: LocalMcpCapabilityTokenExpiryPreview;
}

export interface LocalMcpCapabilityTokenAuthGatePreview {
  runtime_mode: LocalMcpRuntimeMode;
  token_checks_passed: boolean;
  token_accepted: boolean;
  transport_dispatch_allowed: false;
  token_present: boolean;
  token_record_found: boolean;
  fingerprint_matched: boolean;
  expired: boolean;
  revoked: boolean;
  reason: "capability_token_ready" | LocalMcpCapabilityTokenAuthGateFailureReason;
  blocking_gates: LocalMcpCapabilityTokenAuthGateBlockingGate[];
  validation_preview: LocalMcpCapabilityTokenValidationPreview;
}

export interface LocalMcpCapabilityTokenAuthGateBlockingGate {
  gate: "capability_token" | "transport" | "operation_journal";
  status: "blocked" | "disabled" | "planned";
  reason: LocalMcpCapabilityTokenAuthGateFailureReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpCapabilityTokenAcceptanceGatePreview {
  runtime_mode: LocalMcpRuntimeMode;
  token_acceptance_enabled: boolean;
  token_checks_passed: boolean;
  token_accepted: boolean;
  acceptance_allowed: boolean;
  transport_dispatch_allowed: false;
  listener_binding_allowed: false;
  reason: LocalMcpCapabilityTokenAcceptanceGateReason;
  blocking_gates: LocalMcpCapabilityTokenAcceptanceGateBlockingGate[];
  auth_gate_preview: LocalMcpCapabilityTokenAuthGatePreview;
}

export interface LocalMcpCapabilityTokenAcceptanceGateBlockingGate {
  gate: "capability_token" | "token_acceptance" | "transport" | "operation_journal";
  status: "blocked" | "disabled" | "planned";
  reason: LocalMcpCapabilityTokenAcceptanceGateReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpCapabilityTokenRecordExpiryPreview {
  token_record_found: boolean;
  reason?: LocalMcpCapabilityTokenRecordLookupReason;
  record?: LocalMcpCapabilityTokenRecord;
  preview?: LocalMcpCapabilityTokenExpiryPreview;
}

export interface LocalMcpCapabilityTokenRecordRevocationPreview {
  token_record_found: boolean;
  reason?: LocalMcpCapabilityTokenRecordLookupReason;
  record?: LocalMcpCapabilityTokenRecord;
  preview?: LocalMcpCapabilityTokenRevocationPreview;
}

export interface LocalMcpCapabilityTokenRevocationRequest {
  token_id: string;
  requested_at: string;
  token_fingerprint: string;
  confirmation_text: string;
}

export interface LocalMcpCapabilityTokenRevocationValidation {
  valid: false;
  token_revoked: false;
  reason: LocalMcpCapabilityTokenRevocationValidationReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpCapabilityTokenRevocationPreview {
  token_revoked: false;
  runtime_mode: LocalMcpRuntimeMode;
  reason: LocalMcpCapabilityTokenRevocationValidationReason;
  error_code: LocalMcpErrorCode;
  request_validation: LocalMcpCapabilityTokenRevocationValidation;
  planned_record?: LocalMcpCapabilityTokenRevocationRecordPreview;
}

export interface LocalMcpCapabilityTokenRevocationRecordPreview {
  token_id: string;
  token_fingerprint: string;
  revoked_at: string;
  revocation_supported: false;
}

export interface LocalMcpCapabilityTokenExpiryRequest {
  token_id: string;
  issued_at: string;
  expires_at: string;
  checked_at: string;
}

export interface LocalMcpCapabilityTokenExpiryValidation {
  valid: false;
  token_accepted: false;
  expired: boolean;
  reason: LocalMcpCapabilityTokenExpiryValidationReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpCapabilityTokenExpiryPreview {
  token_accepted: false;
  runtime_mode: LocalMcpRuntimeMode;
  expired: boolean;
  reason: LocalMcpCapabilityTokenExpiryValidationReason;
  error_code: LocalMcpErrorCode;
  request_validation: LocalMcpCapabilityTokenExpiryValidation;
  planned_record?: LocalMcpCapabilityTokenExpiryRecordPreview;
}

export interface LocalMcpCapabilityTokenExpiryRecordPreview {
  token_id: string;
  issued_at: string;
  expires_at: string;
  checked_at: string;
  seconds_until_expiry: number;
  expired: boolean;
}

export interface LocalMcpCapabilityTokenValidation {
  valid: false;
  status: LocalMcpCapabilityTokenStatus;
  reason: LocalMcpCapabilityTokenValidationReason;
}

export interface LocalMcpOperationJournalPolicy {
  status: LocalMcpOperationJournalStatus;
  required_for_writes: true;
  reason: "operation_journal_not_enabled";
  planned_fields: LocalMcpOperationJournalField[];
}

export interface LocalMcpOperationJournalContract {
  runtime_mode: LocalMcpRuntimeMode;
  status: LocalMcpOperationJournalStatus;
  journal_enabled: false;
  write_journal_ready: false;
  required_for_writes: true;
  reason: "operation_journal_not_enabled";
  planned_fields: LocalMcpOperationJournalField[];
  blocking_gate: LocalMcpOperationJournalBlockingGate;
}

export interface LocalMcpOperationJournalRecordContract {
  runtime_mode: LocalMcpRuntimeMode;
  status: LocalMcpOperationJournalStatus;
  journal_enabled: false;
  record_written: false;
  record_storage: "local_vault_metadata";
  required_for_operations: ["write"];
  id_strategy: "local-mcp-journal:<request_id>";
  timestamp_source: "transport_request";
  raw_confirmation_text_stored: false;
  raw_capability_token_stored: false;
  token_fingerprint_algorithm: "sha256";
  planned_fields: LocalMcpOperationJournalField[];
  reason: "operation_journal_not_enabled";
  error_code: "transport_disabled";
}

export type LocalMcpOperationJournalField =
  | "id"
  | "timestamp"
  | "runtime_mode"
  | "transport"
  | "tool"
  | "operation"
  | "target"
  | "confirmation_text"
  | "capability_token_fingerprint";

export interface LocalMcpOperationJournalPreview {
  journal_enabled: false;
  reason: "operation_journal_not_enabled" | "journal_not_required";
  runtime_mode: LocalMcpRuntimeMode;
  tool: string;
  operation: "write" | "read" | "unknown";
  required: boolean;
  planned_fields: LocalMcpOperationJournalField[];
}

export interface LocalMcpOperationJournalReadiness {
  runtime_mode: LocalMcpRuntimeMode;
  status: LocalMcpOperationJournalStatus;
  journal_enabled: false;
  write_journal_ready: false;
  required_for_writes: true;
  policy: LocalMcpOperationJournalPolicy;
  preview: LocalMcpOperationJournalPreview;
  blocking_gates: LocalMcpOperationJournalBlockingGate[];
}

export interface LocalMcpOperationJournalBlockingGate {
  gate: "operation_journal";
  status: "planned";
  reason: "operation_journal_not_enabled";
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpOperationJournalRecordWriteRequest {
  request_id: string;
  timestamp: string;
  transport: LocalMcpTransportKind;
  tool: string;
  target?: string;
  confirmation_text?: string;
  capability_token?: string;
}

export interface LocalMcpOperationJournalRecord {
  id: string;
  timestamp: string;
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  tool: string;
  operation: "write";
  target: string | null;
  confirmation_text_present: boolean;
  capability_token_fingerprint: string | null;
  raw_confirmation_text_stored: false;
  raw_capability_token_stored: false;
  executed: false;
  persisted: true;
  reason: "write_execution_disabled";
}

export interface LocalMcpOperationJournalRecordWriteResult {
  record_written: boolean;
  reason?:
    | "journal_not_required"
    | "request_id_required"
    | "timestamp_required"
    | "journal_replay_detected";
  record?: LocalMcpOperationJournalRecord;
  records: LocalMcpOperationJournalRecord[];
  operation_journal_replay_preview?: LocalMcpOperationJournalReplayPreview;
}

export type LocalMcpOperationJournalReplayReason =
  | "journal_replay_not_found"
  | "journal_replay_detected"
  | "request_id_required";

export interface LocalMcpOperationJournalReplayPreview {
  runtime_mode: LocalMcpRuntimeMode;
  request_id_present: boolean;
  idempotency_key: string | null;
  replay_detected: boolean;
  record_write_allowed: boolean;
  existing_record?: LocalMcpOperationJournalRecord;
  reason: LocalMcpOperationJournalReplayReason;
  error_code: LocalMcpErrorCode;
}

export type LocalMcpConfirmedWriteJournalGateReason =
  | "operation_journal_not_enabled"
  | "journal_not_required"
  | "confirmation_required";

export interface LocalMcpConfirmedWriteJournalGatePreview {
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  tool: string;
  operation: "write" | "read" | "unknown";
  confirmation_valid: boolean;
  journal_required: boolean;
  journal_enabled: false;
  journal_write_allowed: false;
  would_record_journal: boolean;
  record_written: false;
  executed: false;
  reason: LocalMcpConfirmedWriteJournalGateReason;
  error_code: LocalMcpErrorCode;
  operation_journal_readiness: LocalMcpOperationJournalReadiness;
  guard: LocalMcpToolExecutionGuard;
  blocking_gate?: LocalMcpOperationJournalBlockingGate;
}

export interface LocalMcpTransportJournaledPreflight {
  preflight: LocalMcpTransportPreflight;
  dispatch_gate: LocalMcpTransportDispatchGatePreview;
  operation_journal_gate_preview: LocalMcpConfirmedWriteJournalGatePreview;
  operation_journal_record: LocalMcpOperationJournalRecordWriteResult;
  confirmed_write_closeout: LocalMcpConfirmedWriteJournalCloseout;
}

export type LocalMcpConfirmedWriteJournalCloseoutStatus =
  | "journal_recorded"
  | "journal_replay_detected"
  | "journal_skipped"
  | "journal_blocked";

export interface LocalMcpConfirmedWriteJournalCloseout {
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  tool: string;
  journal_required: boolean;
  confirmation_valid: boolean;
  journal_recorded: boolean;
  replay_detected: boolean;
  record_available: boolean;
  execution_allowed: false;
  executed: false;
  closeout_status: LocalMcpConfirmedWriteJournalCloseoutStatus;
  reason:
    | LocalMcpConfirmedWriteJournalGateReason
    | NonNullable<LocalMcpOperationJournalRecordWriteResult["reason"]>
    | "write_execution_disabled";
  error_code: LocalMcpErrorCode;
  dispatch_blocking_gate?: LocalMcpTransportDispatchGateBlockingGate;
  idempotency_key?: string | null;
  record_id?: string;
}

export interface LocalMcpTransportStatus {
  transport: LocalMcpTransportKind;
  runtime_mode: LocalMcpRuntimeMode;
  available: false;
  status: "planned";
  reason: "transport_not_enabled";
  guard_enforced: true;
  executable_tools: LocalMcpReadOnlyToolName[];
}

export interface LocalMcpTransportPreflight {
  transport: LocalMcpTransportKind;
  runtime_mode: LocalMcpRuntimeMode;
  transport_available: false;
  reason: "transport_not_enabled" | "capability_token_not_enabled";
  error_code: LocalMcpErrorCode;
  tool_error_code?: LocalMcpErrorCode;
  capability_token: LocalMcpCapabilityTokenValidation;
  capability_token_auth_readiness: LocalMcpCapabilityTokenAuthReadiness;
  operation_journal_readiness: LocalMcpOperationJournalReadiness;
  transport_readiness: LocalMcpTransportReadiness;
  guard: LocalMcpToolExecutionGuard;
}

export interface LocalMcpTransportAuthPreviewPreflight {
  preflight: LocalMcpTransportPreflight;
  capability_token_validation_preview: LocalMcpCapabilityTokenValidationPreview;
  capability_token_auth_gate_preview: LocalMcpCapabilityTokenAuthGatePreview;
  capability_token_acceptance_gate_preview: LocalMcpCapabilityTokenAcceptanceGatePreview;
}

export type LocalMcpTransportDispatchGateBlockingReason =
  | LocalMcpTransportEnvelopeValidationReason
  | LocalMcpCapabilityTokenAuthGateFailureReason
  | "dispatch_allowed"
  | "capability_token_acceptance_not_enabled"
  | "transport_not_enabled"
  | "operation_journal_not_enabled"
  | "write_execution_disabled"
  | "tool_not_implemented"
  | "tool_unknown";

export interface LocalMcpTransportDispatchGateBlockingGate {
  gate:
    | "envelope"
    | "capability_token"
    | "token_acceptance"
    | "transport"
    | "operation_journal"
    | "tool_guard"
    | "write_execution";
  status: "blocked" | "disabled" | "planned";
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code: LocalMcpErrorCode;
}

export interface LocalMcpTransportDispatchGatePreview {
  runtime_mode: LocalMcpRuntimeMode;
  dispatch_allowed: boolean;
  executed: false;
  transport?: LocalMcpTransportKind;
  tool: string;
  envelope_validation: LocalMcpTransportEnvelopeValidation;
  auth_gate?: LocalMcpCapabilityTokenAuthGatePreview;
  acceptance_gate?: LocalMcpCapabilityTokenAcceptanceGatePreview;
  transport_readiness?: LocalMcpTransportReadiness;
  operation_journal_readiness?: LocalMcpOperationJournalReadiness;
  guard?: LocalMcpToolExecutionGuard;
  blocking_gates: LocalMcpTransportDispatchGateBlockingGate[];
  reason: LocalMcpTransportDispatchGateBlockingReason;
  error_code?: LocalMcpErrorCode;
  transport_gate_enabled?: boolean;
}

export interface LocalMcpTransportDispatchGateOptions {
  transport_enabled?: boolean;
}

export interface LocalMcpTransportReadiness {
  runtime_mode: LocalMcpRuntimeMode;
  transport: LocalMcpTransportKind;
  transport_available: false;
  status: "disabled";
  tool: string;
  tool_availability: LocalMcpToolAvailability;
  read_only: boolean;
  executable: false;
  auth: LocalMcpCapabilityTokenAuthReadiness;
  operation_journal: LocalMcpOperationJournalReadiness;
  write_execution: LocalMcpWriteExecutionReadiness;
  blocking_gates: LocalMcpTransportBlockingGate[];
}

export interface LocalMcpWriteExecutionReadiness {
  runtime_mode: LocalMcpRuntimeMode;
  status: "disabled" | "not_required" | "not_implemented" | "unknown";
  write_execution_enabled: false;
  reason: "write_not_enabled" | "write_not_required" | "tool_not_implemented" | "unknown_tool";
  error_code?: LocalMcpErrorCode;
}

export interface LocalMcpTransportCapabilitySummary {
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  transports: LocalMcpTransportKind[];
  executable_read_tools: number;
  blocked_write_tools: number;
  blocking_gates: LocalMcpTransportBlockingGate[];
}

export interface LocalMcpTransportExecutionContract {
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  transports: LocalMcpTransportKind[];
  transport_available: false;
  guard_enforced: true;
  auth_required: true;
  operation_journal_required_for_writes: true;
  request_trace_available: true;
  executable_read_tools: number;
  blocked_write_tools: number;
  blocking_gates: LocalMcpTransportBlockingGate[];
}

export interface LocalMcpTransportDispatchContract {
  runtime_mode: LocalMcpRuntimeMode;
  transport_status: "helper_only";
  transports: LocalMcpTransportKind[];
  helper_dispatch_enabled: true;
  transport_dispatch_enabled: false;
  preflight_required: true;
  envelope_required: true;
  guard_required: true;
  auth_required_for_transports: true;
  operation_journal_required_for_writes: true;
  read_tools_dispatched: number;
  write_tools_dispatched: 0;
  blocked_write_tools: number;
  not_implemented_tools: number;
  disabled_error_code: "transport_disabled";
  write_error_code: "write_execution_disabled";
}

export interface LocalMcpTransportBlockingGate {
  gate: "transport" | "capability_token" | "operation_journal" | "write_execution";
  status: "disabled" | "planned";
  error_code: LocalMcpErrorCode;
  reason:
    | "transport_not_enabled"
    | "capability_token_not_enabled"
    | "operation_journal_not_enabled"
    | "write_not_enabled";
}

export interface LocalMcpWriteBlockContract {
  runtime_mode: LocalMcpRuntimeMode;
  write_execution_enabled: false;
  confirmation_required: true;
  operation_journal_required: true;
  blocked_write_tools: string[];
  not_implemented_tools: string[];
  reason: "write_not_enabled";
  error_code: "write_execution_disabled";
}

export type LocalMcpReadOnlyToolName =
  | "starlane.spaces.list"
  | "starlane.spaces.get"
  | "starlane.folders.list"
  | "starlane.documents.get"
  | "starlane.search.query"
  | "starlane.graph.get"
  | "starlane.tags.list"
  | "starlane.properties.list"
  | "starlane.templates.list";

const LOCAL_MCP_READ_ONLY_TOOLS = new Set<LocalMcpReadOnlyToolName>([
  "starlane.spaces.list",
  "starlane.spaces.get",
  "starlane.folders.list",
  "starlane.documents.get",
  "starlane.search.query",
  "starlane.graph.get",
  "starlane.tags.list",
  "starlane.properties.list",
  "starlane.templates.list",
]);

const LOCAL_MCP_WRITE_BLOCKED_TOOLS = new Set([
  "starlane.documents.create",
  "starlane.documents.update",
  "starlane.documents.delete",
  "starlane.workflows.record_event",
  "starlane.export.snapshot",
  "starlane.import.execute",
]);

const LOCAL_MCP_NOT_IMPLEMENTED_TOOLS = new Set([
  "starlane.import.preview",
  "starlane.admin.users.list",
  "starlane.admin.audit.export",
]);

const LOCAL_MCP_PLANNED_TRANSPORTS: LocalMcpTransportKind[] = ["stdio", "sse"];

export function listLocalMcpReadOnlyTools(): LocalMcpReadOnlyToolName[] {
  return Array.from(LOCAL_MCP_READ_ONLY_TOOLS);
}

export function listLocalMcpBlockedTools(): LocalMcpBlockedToolManifestEntry[] {
  return [
    ...Array.from(LOCAL_MCP_WRITE_BLOCKED_TOOLS).map((name) => ({
      name,
      reason: "write_not_enabled" as const,
    })),
    ...Array.from(LOCAL_MCP_NOT_IMPLEMENTED_TOOLS).map((name) => ({
      name,
      reason: "not_implemented" as const,
    })),
  ];
}

export function listLocalMcpTransportStatuses(): LocalMcpTransportStatus[] {
  const executableTools = listLocalMcpReadOnlyTools();
  return LOCAL_MCP_PLANNED_TRANSPORTS.map((transport) => ({
    transport,
    runtime_mode: "local_vault",
    available: false,
    status: "planned",
    reason: "transport_not_enabled",
    guard_enforced: true,
    executable_tools: executableTools,
  }));
}

export function localMcpCapabilityTokenPolicy(): LocalMcpCapabilityTokenPolicy {
  return {
    status: "disabled",
    required_for_transports: [...LOCAL_MCP_PLANNED_TRANSPORTS],
    reason: "capability_token_not_enabled",
  };
}

export function buildLocalMcpCapabilityTokenRequestChallenge(): LocalMcpCapabilityTokenRequestChallenge {
  return {
    runtime_mode: "local_vault",
    status: "disabled",
    token_issued: false,
    required_for_transports: [...LOCAL_MCP_PLANNED_TRANSPORTS],
    reason: "capability_token_not_enabled",
    confirmation_text: "confirm local mcp capability token",
    planned_fields: [
      "request_id",
      "created_at",
      "runtime_mode",
      "transport",
      "scope",
      "vault_fingerprint",
      "confirmation_text",
    ],
  };
}

export function localMcpCapabilityTokenLifecyclePolicy(): LocalMcpCapabilityTokenLifecyclePolicy {
  return {
    status: "planned",
    token_issued: false,
    storage: "local_vault_metadata",
    token_format: "opaque_bearer",
    fingerprint_algorithm: "sha256",
    token_ttl_seconds: 3600,
    expiry_required: true,
    revocation_supported: false,
    reason: "capability_token_not_enabled",
    planned_fields: [
      "token_id",
      "issued_at",
      "expires_at",
      "runtime_mode",
      "transport",
      "scope",
      "vault_fingerprint",
      "token_fingerprint",
      "revoked_at",
    ],
  };
}

export function localMcpCapabilityTokenAuthReadiness(): LocalMcpCapabilityTokenAuthReadiness {
  const policy = localMcpCapabilityTokenPolicy();
  return {
    runtime_mode: "local_vault",
    status: "disabled",
    transport_ready: false,
    token_accepted: false,
    token_issued: false,
    token_revoked: false,
    required_for_transports: policy.required_for_transports,
    lifecycle: localMcpCapabilityTokenLifecyclePolicy(),
    request_challenge: buildLocalMcpCapabilityTokenRequestChallenge(),
    preview_capabilities: [
      "request_validation",
      "issuance_preview",
      "expiry_preview",
      "revocation_preview",
      "acceptance_gate_preview",
    ],
    blocking_gates: [
      {
        gate: "transport",
        status: "disabled",
        reason: "transport_not_enabled",
        error_code: "transport_disabled",
      },
      {
        gate: "token_issuance",
        status: "disabled",
        reason: "capability_token_not_enabled",
        error_code: "capability_token_not_enabled",
      },
      {
        gate: "token_acceptance",
        status: "disabled",
        reason: "capability_token_not_enabled",
        error_code: "capability_token_required",
      },
      {
        gate: "token_revocation",
        status: "disabled",
        reason: "capability_token_not_enabled",
        error_code: "capability_token_not_enabled",
      },
    ],
  };
}

export function localMcpCapabilityTokenAuthContract(): LocalMcpCapabilityTokenAuthContract {
  const readiness = localMcpCapabilityTokenAuthReadiness();
  return {
    runtime_mode: readiness.runtime_mode,
    status: readiness.status,
    transport_ready: readiness.transport_ready,
    token_accepted: readiness.token_accepted,
    token_issued: readiness.token_issued,
    token_revoked: readiness.token_revoked,
    required_for_transports: readiness.required_for_transports,
    lifecycle_status: readiness.lifecycle.status,
    token_format: readiness.lifecycle.token_format,
    fingerprint_algorithm: readiness.lifecycle.fingerprint_algorithm,
    token_ttl_seconds: readiness.lifecycle.token_ttl_seconds,
    expiry_required: readiness.lifecycle.expiry_required,
    revocation_supported: readiness.lifecycle.revocation_supported,
    preview_capabilities: readiness.preview_capabilities,
    blocking_gates: readiness.blocking_gates,
  };
}

export function validateLocalMcpCapabilityToken(
  token: string | undefined | null,
): LocalMcpCapabilityTokenValidation {
  return {
    valid: false,
    status: "disabled",
    reason: token ? "capability_token_not_enabled" : "capability_token_missing",
  };
}

export function validateLocalMcpCapabilityTokenRequest(
  request: LocalMcpCapabilityTokenRequest,
): LocalMcpCapabilityTokenRequestValidation {
  const challenge = buildLocalMcpCapabilityTokenRequestChallenge();

  if (!request.request_id.trim()) {
    return localMcpCapabilityTokenRequestRejection("request_id_required", "invalid_request");
  }
  if (!request.created_at.trim()) {
    return localMcpCapabilityTokenRequestRejection("created_at_required", "invalid_request");
  }
  if (Number.isNaN(Date.parse(request.created_at))) {
    return localMcpCapabilityTokenRequestRejection("created_at_invalid", "invalid_request");
  }
  if (request.runtime_mode !== "local_vault") {
    return localMcpCapabilityTokenRequestRejection("runtime_mode_mismatch", "invalid_request");
  }
  if (!isLocalMcpTransportKind(request.transport)) {
    return localMcpCapabilityTokenRequestRejection("transport_unsupported", "invalid_request");
  }
  if (request.scope !== "notes:read") {
    return localMcpCapabilityTokenRequestRejection("scope_unsupported", "invalid_request");
  }
  if (!request.vault_fingerprint.trim()) {
    return localMcpCapabilityTokenRequestRejection("vault_fingerprint_required", "invalid_request");
  }
  if (request.confirmation_text !== challenge.confirmation_text) {
    return localMcpCapabilityTokenRequestRejection("confirmation_mismatch", "invalid_request");
  }

  return localMcpCapabilityTokenRequestRejection(
    "capability_token_not_enabled",
    "capability_token_not_enabled",
  );
}

function localMcpCapabilityTokenRequestRejection(
  reason: LocalMcpCapabilityTokenRequestValidationReason,
  errorCode: LocalMcpErrorCode,
): LocalMcpCapabilityTokenRequestValidation {
  return {
    valid: false,
    token_issued: false,
    reason,
    error_code: errorCode,
  };
}

export function previewLocalMcpCapabilityTokenIssuance(
  request: LocalMcpCapabilityTokenRequest,
): LocalMcpCapabilityTokenIssuancePreview {
  const requestValidation = validateLocalMcpCapabilityTokenRequest(request);
  const preview: LocalMcpCapabilityTokenIssuancePreview = {
    token_issued: false,
    runtime_mode: "local_vault",
    reason: requestValidation.reason,
    error_code: requestValidation.error_code,
    request_validation: requestValidation,
  };

  if (requestValidation.reason !== "capability_token_not_enabled") {
    return preview;
  }

  const lifecycle = localMcpCapabilityTokenLifecyclePolicy();
  const issuedAtMs = Date.parse(request.created_at);

  return {
    ...preview,
    planned_record: {
      token_id: `local-mcp-token:${request.request_id}`,
      issued_at: new Date(issuedAtMs).toISOString(),
      expires_at: new Date(issuedAtMs + lifecycle.token_ttl_seconds * 1000).toISOString(),
      runtime_mode: request.runtime_mode,
      transport: request.transport,
      scope: request.scope,
      vault_fingerprint: request.vault_fingerprint,
      token_fingerprint: null,
      revoked_at: null,
    },
  };
}

function localMcpCapabilityTokenStorage(): Storage | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage;
}

function parseLocalMcpCapabilityTokenRecord(
  value: unknown,
): LocalMcpCapabilityTokenRecord | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Partial<LocalMcpCapabilityTokenRecord>;
  if (
    typeof record.token_id !== "string" ||
    typeof record.issued_at !== "string" ||
    typeof record.expires_at !== "string" ||
    record.runtime_mode !== "local_vault" ||
    (record.transport !== "stdio" && record.transport !== "sse") ||
    record.scope !== "notes:read" ||
    typeof record.vault_fingerprint !== "string" ||
    (typeof record.token_fingerprint !== "string" && record.token_fingerprint !== null) ||
    (typeof record.revoked_at !== "string" && record.revoked_at !== null) ||
    record.persisted !== true ||
    typeof record.accepted !== "boolean" ||
    (typeof record.accepted_at !== "string" &&
      record.accepted_at !== null &&
      typeof record.accepted_at !== "undefined") ||
    record.raw_token_stored !== false ||
    (record.reason !== "capability_token_not_enabled" &&
      record.reason !== "capability_token_issued" &&
      record.reason !== "capability_token_accepted" &&
      record.reason !== "capability_token_revoked")
  ) {
    return null;
  }
  return record as LocalMcpCapabilityTokenRecord;
}

export function listLocalMcpCapabilityTokenRecords(): LocalMcpCapabilityTokenRecord[] {
  const storage = localMcpCapabilityTokenStorage();
  if (!storage) return [];
  try {
    const parsed = JSON.parse(
      storage.getItem(LOCAL_MCP_CAPABILITY_TOKEN_STORAGE_KEY) ?? "[]",
    ) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map(parseLocalMcpCapabilityTokenRecord)
      .filter((record): record is LocalMcpCapabilityTokenRecord => Boolean(record));
  } catch {
    return [];
  }
}

function writeLocalMcpCapabilityTokenRecords(records: LocalMcpCapabilityTokenRecord[]): void {
  const storage = localMcpCapabilityTokenStorage();
  if (!storage) return;
  storage.setItem(LOCAL_MCP_CAPABILITY_TOKEN_STORAGE_KEY, JSON.stringify(records));
}

export function clearLocalMcpCapabilityTokenRecords(): void {
  localMcpCapabilityTokenStorage()?.removeItem(LOCAL_MCP_CAPABILITY_TOKEN_STORAGE_KEY);
}

function generateLocalMcpOpaqueCapabilityToken(rawToken?: string): string | null {
  if (rawToken?.trim()) return rawToken;
  if (typeof crypto === "undefined") return null;
  if (typeof crypto.randomUUID === "function") {
    return `local-mcp-${crypto.randomUUID()}`;
  }
  if (typeof crypto.getRandomValues !== "function") return null;

  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  const hex = Array.from(bytes)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  return `local-mcp-${hex}`;
}

export function findLocalMcpCapabilityTokenRecord(
  tokenId: string,
): LocalMcpCapabilityTokenRecord | null {
  const normalizedTokenId = tokenId.trim();
  if (!normalizedTokenId) return null;
  return (
    listLocalMcpCapabilityTokenRecords().find(
      (record) => record.token_id === normalizedTokenId,
    ) ?? null
  );
}

export function persistLocalMcpCapabilityTokenRecord(
  request: LocalMcpCapabilityTokenRequest,
): LocalMcpCapabilityTokenRecordPersistResult {
  const existing = listLocalMcpCapabilityTokenRecords();
  const preview = previewLocalMcpCapabilityTokenIssuance(request);
  if (!preview.planned_record) {
    return {
      record_persisted: false,
      reason: preview.reason,
      records: existing,
    };
  }

  const record: LocalMcpCapabilityTokenRecord = {
    ...preview.planned_record,
    persisted: true,
    accepted: false,
    raw_token_stored: false,
    reason: "capability_token_not_enabled",
  };
  const nextRecords = [
    ...existing.filter((candidate) => candidate.token_id !== record.token_id),
    record,
  ];
  writeLocalMcpCapabilityTokenRecords(nextRecords);
  return { record_persisted: true, record, records: nextRecords };
}

export async function issueLocalMcpCapabilityToken(
  request: LocalMcpCapabilityTokenRequest,
  rawToken?: string,
): Promise<LocalMcpCapabilityTokenIssueResult> {
  const existing = listLocalMcpCapabilityTokenRecords();
  const preview = previewLocalMcpCapabilityTokenIssuance(request);
  if (!preview.planned_record) {
    return { token_issued: false, reason: preview.reason, records: existing };
  }

  const issuedRawToken = generateLocalMcpOpaqueCapabilityToken(rawToken);
  if (!issuedRawToken) {
    return { token_issued: false, reason: "token_generation_unavailable", records: existing };
  }

  const tokenFingerprint = await fingerprintLocalMcpCapabilityToken(issuedRawToken);
  if (!tokenFingerprint) {
    return { token_issued: false, reason: "token_fingerprint_unavailable", records: existing };
  }

  const record: LocalMcpCapabilityTokenRecord = {
    ...preview.planned_record,
    token_fingerprint: tokenFingerprint,
    persisted: true,
    accepted: false,
    raw_token_stored: false,
    reason: "capability_token_issued",
  };
  const nextRecords = [
    ...existing.filter((candidate) => candidate.token_id !== record.token_id),
    record,
  ];
  writeLocalMcpCapabilityTokenRecords(nextRecords);
  return {
    token_issued: true,
    raw_token: issuedRawToken,
    record,
    records: nextRecords,
  };
}

export async function previewLocalMcpCapabilityTokenValidation(
  token: string | undefined | null,
  checkedAt: string,
): Promise<LocalMcpCapabilityTokenValidationPreview> {
  if (!token?.trim()) {
    return {
      token_accepted: false,
      token_present: false,
      token_record_found: false,
      fingerprint_matched: false,
      expired: false,
      reason: "capability_token_missing",
    };
  }

  const tokenFingerprint = await fingerprintLocalMcpCapabilityToken(token);
  if (!tokenFingerprint) {
    return {
      token_accepted: false,
      token_present: true,
      token_record_found: false,
      fingerprint_matched: false,
      expired: false,
      reason: "token_fingerprint_unavailable",
    };
  }

  const records = listLocalMcpCapabilityTokenRecords();
  const fingerprintRecords = records.filter((record) => Boolean(record.token_fingerprint));
  const record =
    fingerprintRecords.find((candidate) => candidate.token_fingerprint === tokenFingerprint) ??
    null;

  if (!record) {
    return {
      token_accepted: false,
      token_present: true,
      token_record_found: false,
      fingerprint_matched: false,
      expired: false,
      reason:
        fingerprintRecords.length > 0 ? "token_fingerprint_mismatch" : "token_record_not_found",
      token_fingerprint: tokenFingerprint,
    };
  }

  if (record.revoked_at) {
    return {
      token_accepted: false,
      token_present: true,
      token_record_found: true,
      fingerprint_matched: true,
      expired: false,
      reason: "capability_token_revoked",
      token_fingerprint: tokenFingerprint,
      record,
    };
  }

  const expiryPreview = previewLocalMcpCapabilityTokenExpiry({
    token_id: record.token_id,
    issued_at: record.issued_at,
    expires_at: record.expires_at,
    checked_at: checkedAt,
  });

  return {
    token_accepted: record.accepted === true,
    token_present: true,
    token_record_found: true,
    fingerprint_matched: true,
    expired: expiryPreview.expired,
    reason: expiryPreview.expired ? "capability_token_expired" : "capability_token_not_enabled",
    token_fingerprint: tokenFingerprint,
    record,
    expiry_preview: expiryPreview,
  };
}

export async function previewLocalMcpCapabilityTokenAuthGate(
  token: string | undefined | null,
  checkedAt: string,
): Promise<LocalMcpCapabilityTokenAuthGatePreview> {
  const validationPreview = await previewLocalMcpCapabilityTokenValidation(token, checkedAt);
  const tokenChecksPassed =
    validationPreview.token_present &&
    validationPreview.token_record_found &&
    validationPreview.fingerprint_matched &&
    !validationPreview.expired &&
    validationPreview.reason === "capability_token_not_enabled" &&
    !validationPreview.record?.revoked_at;
  const tokenAccepted = tokenChecksPassed && validationPreview.record?.accepted === true;
  const tokenBlockingGate: LocalMcpCapabilityTokenAuthGateBlockingGate | null =
    tokenChecksPassed
      ? null
      : {
          gate: "capability_token",
          status: "blocked",
          reason: validationPreview.reason,
          error_code: localMcpCapabilityTokenAuthGateErrorCode(validationPreview.reason),
        };

  return {
    runtime_mode: "local_vault",
    token_checks_passed: tokenChecksPassed,
    token_accepted: tokenAccepted,
    transport_dispatch_allowed: false,
    token_present: validationPreview.token_present,
    token_record_found: validationPreview.token_record_found,
    fingerprint_matched: validationPreview.fingerprint_matched,
    expired: validationPreview.expired,
    revoked: validationPreview.reason === "capability_token_revoked",
    reason: tokenChecksPassed ? "capability_token_ready" : validationPreview.reason,
    blocking_gates: [
      ...(tokenBlockingGate ? [tokenBlockingGate] : []),
      {
        gate: "transport",
        status: "disabled",
        reason: "transport_not_enabled",
        error_code: "transport_disabled",
      },
      {
        gate: "operation_journal",
        status: "planned",
        reason: "operation_journal_not_enabled",
        error_code: "transport_disabled",
      },
    ],
    validation_preview: validationPreview,
  };
}

export async function previewLocalMcpCapabilityTokenAcceptanceGate(
  token: string | undefined | null,
  checkedAt: string,
): Promise<LocalMcpCapabilityTokenAcceptanceGatePreview> {
  const authGatePreview = await previewLocalMcpCapabilityTokenAuthGate(token, checkedAt);
  return localMcpCapabilityTokenAcceptanceGateFromAuthPreview(authGatePreview);
}

function localMcpCapabilityTokenAcceptanceGateFromAuthPreview(
  authGatePreview: LocalMcpCapabilityTokenAuthGatePreview,
): LocalMcpCapabilityTokenAcceptanceGatePreview {
  const tokenAccepted = authGatePreview.token_checks_passed && authGatePreview.token_accepted;
  const tokenAcceptanceGate: LocalMcpCapabilityTokenAcceptanceGateBlockingGate = {
    gate: "token_acceptance",
    status: "disabled",
    reason: "capability_token_acceptance_not_enabled",
    error_code: "capability_token_not_enabled",
  };
  const authBlockingGates: LocalMcpCapabilityTokenAcceptanceGateBlockingGate[] =
    authGatePreview.blocking_gates.map((gate) => ({
      gate: gate.gate,
      status: gate.status,
      reason: gate.reason,
      error_code: gate.error_code,
    }));

  return {
    runtime_mode: "local_vault",
    token_acceptance_enabled: tokenAccepted,
    token_checks_passed: authGatePreview.token_checks_passed,
    token_accepted: tokenAccepted,
    acceptance_allowed: tokenAccepted,
    transport_dispatch_allowed: false,
    listener_binding_allowed: false,
    reason: tokenAccepted
      ? "capability_token_accepted"
      : authGatePreview.token_checks_passed || authGatePreview.reason === "capability_token_ready"
        ? "capability_token_acceptance_not_enabled"
        : authGatePreview.reason,
    blocking_gates: tokenAccepted
      ? authBlockingGates
      : authGatePreview.token_checks_passed
        ? [tokenAcceptanceGate, ...authBlockingGates]
        : authBlockingGates,
    auth_gate_preview: authGatePreview,
  };
}

function localMcpCapabilityTokenAuthGateErrorCode(
  reason: LocalMcpCapabilityTokenValidationPreviewReason,
): LocalMcpErrorCode {
  return reason === "capability_token_missing"
    ? "capability_token_required"
    : "capability_token_not_enabled";
}

export function previewLocalMcpCapabilityTokenRecordExpiry(
  tokenId: string,
  checkedAt: string,
): LocalMcpCapabilityTokenRecordExpiryPreview {
  const record = findLocalMcpCapabilityTokenRecord(tokenId);
  if (!record) {
    return { token_record_found: false, reason: "token_record_not_found" };
  }

  return {
    token_record_found: true,
    record,
    preview: previewLocalMcpCapabilityTokenExpiry({
      token_id: record.token_id,
      issued_at: record.issued_at,
      expires_at: record.expires_at,
      checked_at: checkedAt,
    }),
  };
}

export function validateLocalMcpCapabilityTokenRevocationRequest(
  request: LocalMcpCapabilityTokenRevocationRequest,
): LocalMcpCapabilityTokenRevocationValidation {
  if (!request.token_id.trim()) {
    return localMcpCapabilityTokenRevocationRejection("token_id_required", "invalid_request");
  }
  if (!request.requested_at.trim()) {
    return localMcpCapabilityTokenRevocationRejection("requested_at_required", "invalid_request");
  }
  if (Number.isNaN(Date.parse(request.requested_at))) {
    return localMcpCapabilityTokenRevocationRejection("requested_at_invalid", "invalid_request");
  }
  if (!request.token_fingerprint.trim()) {
    return localMcpCapabilityTokenRevocationRejection(
      "token_fingerprint_required",
      "invalid_request",
    );
  }
  if (request.confirmation_text !== "confirm local mcp capability token revocation") {
    return localMcpCapabilityTokenRevocationRejection("confirmation_mismatch", "invalid_request");
  }

  return localMcpCapabilityTokenRevocationRejection(
    "capability_token_not_enabled",
    "capability_token_not_enabled",
  );
}

function localMcpCapabilityTokenRevocationRejection(
  reason: LocalMcpCapabilityTokenRevocationValidationReason,
  errorCode: LocalMcpErrorCode,
): LocalMcpCapabilityTokenRevocationValidation {
  return {
    valid: false,
    token_revoked: false,
    reason,
    error_code: errorCode,
  };
}

export function previewLocalMcpCapabilityTokenRevocation(
  request: LocalMcpCapabilityTokenRevocationRequest,
): LocalMcpCapabilityTokenRevocationPreview {
  const requestValidation = validateLocalMcpCapabilityTokenRevocationRequest(request);
  const preview: LocalMcpCapabilityTokenRevocationPreview = {
    token_revoked: false,
    runtime_mode: "local_vault",
    reason: requestValidation.reason,
    error_code: requestValidation.error_code,
    request_validation: requestValidation,
  };

  if (requestValidation.reason !== "capability_token_not_enabled") {
    return preview;
  }

  return {
    ...preview,
    planned_record: {
      token_id: request.token_id,
      token_fingerprint: request.token_fingerprint,
      revoked_at: new Date(Date.parse(request.requested_at)).toISOString(),
      revocation_supported: false,
    },
  };
}

export function previewLocalMcpCapabilityTokenRecordRevocation(
  request: LocalMcpCapabilityTokenRevocationRequest,
): LocalMcpCapabilityTokenRecordRevocationPreview {
  const record = findLocalMcpCapabilityTokenRecord(request.token_id);
  if (!record) {
    return { token_record_found: false, reason: "token_record_not_found" };
  }

  return {
    token_record_found: true,
    record,
    preview: previewLocalMcpCapabilityTokenRevocation(request),
  };
}

export function revokeLocalMcpCapabilityTokenRecord(
  request: LocalMcpCapabilityTokenRevocationRequest,
): LocalMcpCapabilityTokenRevokeResult {
  const existing = listLocalMcpCapabilityTokenRecords();
  const preview = previewLocalMcpCapabilityTokenRevocation(request);
  if (!preview.planned_record) {
    return { token_revoked: false, reason: preview.reason, records: existing };
  }

  const record = existing.find((candidate) => candidate.token_id === request.token_id) ?? null;
  if (!record) {
    return { token_revoked: false, reason: "token_record_not_found", records: existing };
  }
  if (record.token_fingerprint !== request.token_fingerprint) {
    return { token_revoked: false, reason: "token_fingerprint_mismatch", records: existing };
  }

  const revokedRecord: LocalMcpCapabilityTokenRecord = {
    ...record,
    revoked_at: preview.planned_record.revoked_at,
    accepted: false,
    raw_token_stored: false,
    reason: "capability_token_revoked",
  };
  const nextRecords = [
    ...existing.filter((candidate) => candidate.token_id !== revokedRecord.token_id),
    revokedRecord,
  ];
  writeLocalMcpCapabilityTokenRecords(nextRecords);
  return { token_revoked: true, record: revokedRecord, records: nextRecords };
}

export function validateLocalMcpCapabilityTokenExpiry(
  request: LocalMcpCapabilityTokenExpiryRequest,
): LocalMcpCapabilityTokenExpiryValidation {
  if (!request.token_id.trim()) {
    return localMcpCapabilityTokenExpiryRejection("token_id_required", "invalid_request", false);
  }
  if (!request.issued_at.trim()) {
    return localMcpCapabilityTokenExpiryRejection("issued_at_required", "invalid_request", false);
  }
  const issuedAtMs = Date.parse(request.issued_at);
  if (Number.isNaN(issuedAtMs)) {
    return localMcpCapabilityTokenExpiryRejection("issued_at_invalid", "invalid_request", false);
  }
  if (!request.expires_at.trim()) {
    return localMcpCapabilityTokenExpiryRejection("expires_at_required", "invalid_request", false);
  }
  const expiresAtMs = Date.parse(request.expires_at);
  if (Number.isNaN(expiresAtMs)) {
    return localMcpCapabilityTokenExpiryRejection("expires_at_invalid", "invalid_request", false);
  }
  if (!request.checked_at.trim()) {
    return localMcpCapabilityTokenExpiryRejection("checked_at_required", "invalid_request", false);
  }
  const checkedAtMs = Date.parse(request.checked_at);
  if (Number.isNaN(checkedAtMs)) {
    return localMcpCapabilityTokenExpiryRejection("checked_at_invalid", "invalid_request", false);
  }
  if (expiresAtMs <= issuedAtMs) {
    return localMcpCapabilityTokenExpiryRejection("expiry_before_issue", "invalid_request", false);
  }
  if (checkedAtMs >= expiresAtMs) {
    return localMcpCapabilityTokenExpiryRejection(
      "capability_token_expired",
      "capability_token_not_enabled",
      true,
    );
  }

  return localMcpCapabilityTokenExpiryRejection(
    "capability_token_not_enabled",
    "capability_token_not_enabled",
    false,
  );
}

function localMcpCapabilityTokenExpiryRejection(
  reason: LocalMcpCapabilityTokenExpiryValidationReason,
  errorCode: LocalMcpErrorCode,
  expired: boolean,
): LocalMcpCapabilityTokenExpiryValidation {
  return {
    valid: false,
    token_accepted: false,
    expired,
    reason,
    error_code: errorCode,
  };
}

export function previewLocalMcpCapabilityTokenExpiry(
  request: LocalMcpCapabilityTokenExpiryRequest,
): LocalMcpCapabilityTokenExpiryPreview {
  const requestValidation = validateLocalMcpCapabilityTokenExpiry(request);
  const preview: LocalMcpCapabilityTokenExpiryPreview = {
    token_accepted: false,
    runtime_mode: "local_vault",
    expired: requestValidation.expired,
    reason: requestValidation.reason,
    error_code: requestValidation.error_code,
    request_validation: requestValidation,
  };

  if (
    requestValidation.reason !== "capability_token_not_enabled" &&
    requestValidation.reason !== "capability_token_expired"
  ) {
    return preview;
  }

  const expiresAtMs = Date.parse(request.expires_at);
  const checkedAtMs = Date.parse(request.checked_at);

  return {
    ...preview,
    planned_record: {
      token_id: request.token_id,
      issued_at: new Date(Date.parse(request.issued_at)).toISOString(),
      expires_at: new Date(expiresAtMs).toISOString(),
      checked_at: new Date(checkedAtMs).toISOString(),
      seconds_until_expiry: Math.floor((expiresAtMs - checkedAtMs) / 1000),
      expired: requestValidation.expired,
    },
  };
}

export function localMcpOperationJournalPolicy(): LocalMcpOperationJournalPolicy {
  return {
    status: "planned",
    required_for_writes: true,
    reason: "operation_journal_not_enabled",
    planned_fields: [
      "id",
      "timestamp",
      "runtime_mode",
      "transport",
      "tool",
      "operation",
      "target",
      "confirmation_text",
      "capability_token_fingerprint",
    ],
  };
}

export function localMcpOperationJournalContract(): LocalMcpOperationJournalContract {
  const policy = localMcpOperationJournalPolicy();
  return {
    runtime_mode: "local_vault",
    status: policy.status,
    journal_enabled: false,
    write_journal_ready: false,
    required_for_writes: policy.required_for_writes,
    reason: policy.reason,
    planned_fields: policy.planned_fields,
    blocking_gate: {
      gate: "operation_journal",
      status: "planned",
      reason: "operation_journal_not_enabled",
      error_code: "transport_disabled",
    },
  };
}

export function localMcpOperationJournalRecordContract(): LocalMcpOperationJournalRecordContract {
  const policy = localMcpOperationJournalPolicy();
  return {
    runtime_mode: "local_vault",
    status: policy.status,
    journal_enabled: false,
    record_written: false,
    record_storage: "local_vault_metadata",
    required_for_operations: ["write"],
    id_strategy: "local-mcp-journal:<request_id>",
    timestamp_source: "transport_request",
    raw_confirmation_text_stored: false,
    raw_capability_token_stored: false,
    token_fingerprint_algorithm: "sha256",
    planned_fields: policy.planned_fields,
    reason: policy.reason,
    error_code: "transport_disabled",
  };
}

export function localMcpTransportCapabilitySummary(): LocalMcpTransportCapabilitySummary {
  return {
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    transports: [...LOCAL_MCP_PLANNED_TRANSPORTS],
    executable_read_tools: listLocalMcpReadOnlyTools().length,
    blocked_write_tools: Array.from(LOCAL_MCP_WRITE_BLOCKED_TOOLS).length,
    blocking_gates: [
      {
        gate: "transport",
        status: "disabled",
        error_code: "transport_disabled",
        reason: "transport_not_enabled",
      },
      {
        gate: "capability_token",
        status: "disabled",
        error_code: "capability_token_required",
        reason: "capability_token_not_enabled",
      },
      {
        gate: "operation_journal",
        status: "planned",
        error_code: "transport_disabled",
        reason: "operation_journal_not_enabled",
      },
      {
        gate: "write_execution",
        status: "disabled",
        error_code: "write_execution_disabled",
        reason: "write_not_enabled",
      },
    ],
  };
}

export function localMcpTransportExecutionContract(): LocalMcpTransportExecutionContract {
  const capabilities = localMcpTransportCapabilitySummary();
  return {
    runtime_mode: capabilities.runtime_mode,
    transport_status: capabilities.transport_status,
    transports: capabilities.transports,
    transport_available: false,
    guard_enforced: true,
    auth_required: true,
    operation_journal_required_for_writes:
      localMcpOperationJournalContract().required_for_writes,
    request_trace_available: true,
    executable_read_tools: capabilities.executable_read_tools,
    blocked_write_tools: capabilities.blocked_write_tools,
    blocking_gates: capabilities.blocking_gates,
  };
}

export function localMcpWriteBlockContract(): LocalMcpWriteBlockContract {
  return {
    runtime_mode: "local_vault",
    write_execution_enabled: false,
    confirmation_required: true,
    operation_journal_required: localMcpOperationJournalContract().required_for_writes,
    blocked_write_tools: Array.from(LOCAL_MCP_WRITE_BLOCKED_TOOLS),
    not_implemented_tools: Array.from(LOCAL_MCP_NOT_IMPLEMENTED_TOOLS),
    reason: "write_not_enabled",
    error_code: "write_execution_disabled",
  };
}

export function localMcpTransportRequestTracePolicy(): LocalMcpTransportRequestTracePolicy {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    status: "preview",
    persisted: false,
    journaled: false,
    storage: "response_only",
    retention: "none",
    raw_values_stored: false,
    value_policy: "presence_only",
    stages: ["envelope_validation", "transport_preflight"],
    blocking_stages: ["envelope_validation", "capability_token", "transport", "tool_guard"],
    primary_blocking_stages: ["envelope_validation", "capability_token", "transport"],
    secondary_blocking_stages: ["tool_guard"],
    attaches_to: ["valid_preflight_response", "invalid_envelope_response"],
    redacted_fields: ["input", "confirmation_text", "capability_token"],
    reason: "operation_journal_not_enabled",
  };
}

export function localMcpTransportRequestTraceContract(): LocalMcpTransportRequestTraceContract {
  const policy = localMcpTransportRequestTracePolicy();
  return {
    schema_version: policy.schema_version,
    runtime_mode: policy.runtime_mode,
    status: policy.status,
    storage: policy.storage,
    retention: policy.retention,
    persisted: policy.persisted,
    journaled: policy.journaled,
    raw_values_stored: policy.raw_values_stored,
    value_policy: policy.value_policy,
    stages: policy.stages,
    primary_blocking_stages: policy.primary_blocking_stages,
    secondary_blocking_stages: policy.secondary_blocking_stages,
    attaches_to: policy.attaches_to,
  };
}

export function localMcpTransportEnvelopeContract(): LocalMcpTransportEnvelopeContract {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    transports: [...LOCAL_MCP_PLANNED_TRANSPORTS],
    transport_available: false,
    execution_enabled: false,
    request_fields: [
      "id",
      "transport",
      "tool",
      "request.input",
      "request.confirmation_text",
      "request.capability_token",
    ],
    response_fields: [
      "id",
      "transport",
      "runtime_mode",
      "executed",
      "reason",
      "error_code",
      "preflight",
      "validation",
      "request_trace",
    ],
    validation_reasons: ["request_id_required", "transport_unsupported", "tool_required"],
    response_reasons: [
      "request_invalid",
      "transport_not_enabled",
      "capability_token_not_enabled",
    ],
    stable_error_codes: [
      "invalid_request",
      "transport_disabled",
      "capability_token_required",
      "capability_token_not_enabled",
      "write_execution_disabled",
      "tool_not_implemented",
      "tool_unknown",
    ],
    trace_attached: true,
    result_returned: false,
  };
}

export function localMcpTransportDispatchContract(): LocalMcpTransportDispatchContract {
  return {
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    transports: [...LOCAL_MCP_PLANNED_TRANSPORTS],
    helper_dispatch_enabled: true,
    transport_dispatch_enabled: false,
    preflight_required: true,
    envelope_required: true,
    guard_required: true,
    auth_required_for_transports: true,
    operation_journal_required_for_writes:
      localMcpOperationJournalContract().required_for_writes,
    read_tools_dispatched: listLocalMcpReadOnlyTools().length,
    write_tools_dispatched: 0,
    blocked_write_tools: Array.from(LOCAL_MCP_WRITE_BLOCKED_TOOLS).length,
    not_implemented_tools: Array.from(LOCAL_MCP_NOT_IMPLEMENTED_TOOLS).length,
    disabled_error_code: "transport_disabled",
    write_error_code: "write_execution_disabled",
  };
}

export function localMcpTransportErrorContract(): LocalMcpTransportErrorContract {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    stable_errors: [
      {
        error_code: "invalid_request",
        stage: "envelope_validation",
        blocking_stage: "envelope_validation",
        blocking_priority: "primary",
        reasons: ["request_id_required", "transport_unsupported", "tool_required"],
      },
      {
        error_code: "capability_token_required",
        stage: "transport_preflight",
        blocking_stage: "capability_token",
        blocking_priority: "primary",
        reasons: ["capability_token_missing"],
      },
      {
        error_code: "capability_token_not_enabled",
        stage: "transport_preflight",
        blocking_stage: "capability_token",
        blocking_priority: "primary",
        reasons: ["capability_token_not_enabled"],
      },
      {
        error_code: "transport_disabled",
        stage: "transport_preflight",
        blocking_stage: "transport",
        blocking_priority: "primary",
        reasons: ["transport_not_enabled"],
      },
      {
        error_code: "write_execution_disabled",
        stage: "transport_preflight",
        blocking_stage: "tool_guard",
        blocking_priority: "secondary",
        reasons: ["write_execution_disabled"],
      },
      {
        error_code: "tool_not_implemented",
        stage: "transport_preflight",
        blocking_stage: "tool_guard",
        blocking_priority: "secondary",
        reasons: ["tool_not_implemented"],
      },
      {
        error_code: "tool_unknown",
        stage: "transport_preflight",
        blocking_stage: "tool_guard",
        blocking_priority: "secondary",
        reasons: ["tool_unknown"],
      },
    ],
    default_invalid_request_error: "invalid_request",
    default_preflight_error: "transport_disabled",
    tool_error_field: "tool_error_code",
  };
}

export function localMcpTransportResultContract(): LocalMcpTransportResultContract {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    helper_read_results_enabled: true,
    transport_results_enabled: false,
    transport_result_returned: false,
    invalid_envelope_result_returned: false,
    preflight_result_returned: false,
    result_field: "result",
    blocked_result_sources: ["envelope_validation", "transport_preflight"],
    successful_helper_tools: listLocalMcpReadOnlyTools().length,
    blocked_write_tools: Array.from(LOCAL_MCP_WRITE_BLOCKED_TOOLS).length,
    not_implemented_tools: Array.from(LOCAL_MCP_NOT_IMPLEMENTED_TOOLS).length,
  };
}

export function localMcpTransportRetryContract(): LocalMcpTransportRetryContract {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    automatic_retries_enabled: false,
    retry_after_ms: null,
    retry_entries: [
      {
        error_code: "invalid_request",
        retryable: false,
        automatic_retry: false,
        condition: "fix_invalid_envelope",
      },
      {
        error_code: "transport_disabled",
        retryable: true,
        automatic_retry: false,
        condition: "enable_transport",
      },
      {
        error_code: "capability_token_required",
        retryable: true,
        automatic_retry: false,
        condition: "issue_capability_token",
      },
      {
        error_code: "capability_token_not_enabled",
        retryable: true,
        automatic_retry: false,
        condition: "issue_capability_token",
      },
      {
        error_code: "write_execution_disabled",
        retryable: false,
        automatic_retry: false,
        condition: "enable_write_execution",
      },
      {
        error_code: "tool_not_implemented",
        retryable: false,
        automatic_retry: false,
        condition: "implement_tool",
      },
      {
        error_code: "tool_unknown",
        retryable: false,
        automatic_retry: false,
        condition: "implement_tool",
      },
    ],
  };
}

export function localMcpTransportIdempotencyContract(): LocalMcpTransportIdempotencyContract {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    request_id_required: true,
    replay_cache_enabled: false,
    write_idempotency_enabled: false,
    operation_journal_required: true,
    idempotency_key_field: "id",
    protected_scopes: ["request_envelope", "write_execution"],
    duplicate_policy: "not_enforced_until_transport_enabled",
    reason: "operation_journal_not_enabled",
  };
}

export function localMcpTransportSideEffectContract(): LocalMcpTransportSideEffectContract {
  const boundaries: LocalMcpTransportSideEffectBoundary[] = [
    "envelope_validation",
    "transport_preflight",
    "helper_read_dispatch",
    "write_execution",
    "operation_journal",
    "replay_cache",
  ];

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    transport_side_effects_enabled: false,
    helper_read_side_effects_enabled: false,
    write_side_effects_enabled: false,
    persistence_enabled: false,
    journal_writes_enabled: false,
    replay_cache_writes_enabled: false,
    boundaries,
    non_mutating_boundaries: boundaries,
    mutating_boundaries: [],
  };
}

export function localMcpTransportAdapterContract(): LocalMcpTransportAdapterContract {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    adapters_enabled: false,
    adapters: listLocalMcpTransportStatuses().map<LocalMcpTransportAdapterContractEntry>(
      ({ transport }) => ({
        transport,
        runtime_mode: "local_vault",
        adapter_available: false,
        bindable: false,
        accepts_requests: false,
        emits_responses: false,
        requires_capability_token: true,
        requires_operation_journal: true,
        reason: "transport_not_enabled",
      }),
    ),
  };
}

export function localMcpTransportListenerConfigContract(): LocalMcpTransportListenerConfigContract {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    listener_configuration_enabled: false,
    transports: ["stdio", "sse"],
    config_fields: [
      "transport",
      "stdio.stdin",
      "stdio.stdout",
      "stdio.stderr",
      "sse.endpoint",
      "sse.method",
      "sse.headers",
      "capability_token",
    ],
    requires_capability_token: true,
    requires_operation_journal: true,
    stdio_stream_binding_enabled: false,
    sse_endpoint_registration_enabled: false,
    reason: "transport_not_enabled",
  };
}

export function previewLocalMcpTransportAdapterBindPreflight(
  transport: LocalMcpTransportKind,
  tool = "starlane.spaces.list",
): LocalMcpTransportAdapterBindPreflight {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    adapter_available: false,
    bindable: false,
    bind_attempted: false,
    bound: false,
    listener_started: false,
    request_loop_started: false,
    accepts_requests: false,
    emits_responses: false,
    requires_capability_token: true,
    requires_operation_journal: true,
    transport_readiness: localMcpTransportReadiness(transport, tool),
    capability_token_auth_readiness: localMcpCapabilityTokenAuthReadiness(),
    operation_journal_readiness: localMcpOperationJournalReadiness(tool),
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };
}

export async function previewLocalMcpTransportAdapterBindPreflightWithAcceptance(
  transport: LocalMcpTransportKind,
  tool = "starlane.spaces.list",
  request: LocalMcpToolExecutionRequest = {},
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportAdapterBindPreflightWithAcceptance> {
  const capabilityTokenAcceptanceGatePreview =
    await previewLocalMcpCapabilityTokenAcceptanceGate(request.capability_token, checkedAt);
  const dispatchGate = await previewLocalMcpTransportDispatchGate(
    {
      id: `bind-preflight:${transport}:${tool}`,
      transport,
      tool,
      request,
    },
    checkedAt,
    options,
  );
  const bindBlockingGate = dispatchGate.blocking_gates[0] ?? {
    gate: "transport",
    status: "disabled",
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };

  return {
    ...previewLocalMcpTransportAdapterBindPreflight(transport, tool),
    listener_binding_allowed: false,
    capability_token_acceptance_gate_preview: capabilityTokenAcceptanceGatePreview,
    dispatch_gate: dispatchGate,
    bind_blocking_gate: bindBlockingGate,
  };
}

export function previewLocalMcpTransportListenerConfig(
  transport: LocalMcpTransportKind,
  tool = "starlane.spaces.list",
): LocalMcpTransportListenerConfigPreview {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    listener_configuration_enabled: false,
    listener_configured: false,
    stdio_stream_binding_enabled: false,
    stdio_stdin_required: transport === "stdio",
    stdio_stdout_required: transport === "stdio",
    stdio_stderr_required: transport === "stdio",
    stdio_streams_attached: false,
    sse_endpoint_registration_enabled: false,
    sse_endpoint_required: transport === "sse",
    sse_method: transport === "sse" ? "GET" : null,
    sse_headers_required: transport === "sse",
    sse_endpoint_registered: false,
    endpoint: null,
    bind_preflight: previewLocalMcpTransportAdapterBindPreflight(transport, tool),
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };
}

export function validateLocalMcpTransportListenerConfig(
  request: LocalMcpTransportListenerConfigRequest,
): LocalMcpTransportListenerConfigValidation {
  if (!isLocalMcpTransportKind(request.transport)) {
    return {
      valid: false,
      reason: "transport_unsupported",
      error_code: "invalid_request",
      listener_configuration_enabled: false,
    };
  }

  if (
    request.transport === "stdio" &&
    (!request.stdio?.stdin || !request.stdio.stdout || !request.stdio.stderr)
  ) {
    return {
      valid: false,
      transport: request.transport,
      reason: "stdio_streams_required",
      error_code: "invalid_request",
      listener_configuration_enabled: false,
    };
  }

  if (request.transport === "sse" && !request.sse?.endpoint?.trim()) {
    return {
      valid: false,
      transport: request.transport,
      reason: "sse_endpoint_required",
      error_code: "invalid_request",
      listener_configuration_enabled: false,
    };
  }

  if (
    request.transport === "sse" &&
    request.sse?.method !== undefined &&
    request.sse.method !== "GET"
  ) {
    return {
      valid: false,
      transport: request.transport,
      reason: "sse_method_invalid",
      error_code: "invalid_request",
      listener_configuration_enabled: false,
    };
  }

  return {
    valid: false,
    transport: request.transport,
    reason: "listener_configuration_not_enabled",
    error_code: "transport_disabled",
    listener_configuration_enabled: false,
  };
}

export async function previewLocalMcpTransportListenerConfigValidation(
  request: LocalMcpTransportListenerConfigRequest,
  checkedAt = new Date(0).toISOString(),
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportListenerConfigValidationPreview> {
  const validation = validateLocalMcpTransportListenerConfig(request);
  const configPreview = validation.transport
    ? previewLocalMcpTransportListenerConfig(validation.transport, request.tool)
    : undefined;
  const bindPreflight =
    validation.reason === "listener_configuration_not_enabled" && validation.transport
      ? await previewLocalMcpTransportAdapterBindPreflightWithAcceptance(
          validation.transport,
          request.tool,
          request.capability_token ? { capability_token: request.capability_token } : {},
          checkedAt,
          options,
        )
      : undefined;
  const listenerBindDescriptor =
    validation.reason === "listener_configuration_not_enabled" && validation.transport && bindPreflight
      ? localMcpTransportListenerBindDescriptor(request, validation.transport, bindPreflight)
      : undefined;
  const startupPreview =
    validation.reason === "listener_configuration_not_enabled" && validation.transport && bindPreflight
      ? {
          ...previewLocalMcpTransportAdapterStartup(validation.transport, request.tool),
          bind_preflight: bindPreflight,
          listener_binding_allowed: false as const,
          listener_bind_descriptor: listenerBindDescriptor,
          capability_token_acceptance_gate_preview:
            bindPreflight.capability_token_acceptance_gate_preview,
        }
      : undefined;
  const requestLoopRegistration =
    validation.reason === "listener_configuration_not_enabled" &&
    validation.transport &&
    startupPreview
      ? localMcpTransportRequestLoopRegistrationPreview(validation.transport, startupPreview)
      : undefined;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport: validation.transport,
    listener_configuration_enabled: false,
    listener_configured: false,
    validation,
    config_preview: configPreview,
    bind_preflight: bindPreflight,
    listener_bind_descriptor: listenerBindDescriptor,
    startup_preview: startupPreview,
    request_loop_registration: requestLoopRegistration,
    reason: validation.reason,
    error_code: validation.error_code,
  };
}

function localMcpTransportRequestLoopRegistrationPreview(
  transport: LocalMcpTransportKind,
  startupPreview: LocalMcpTransportAdapterStartupWithAcceptancePreview,
): LocalMcpTransportRequestLoopRegistrationPreview {
  const bindPreflight = startupPreview.bind_preflight;
  const firstBlockingGate = bindPreflight.bind_blocking_gate;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    request_loop_registration_enabled: false,
    request_loop_registered: false,
    request_loop_started: false,
    accepts_requests: false,
    emits_responses: false,
    handler_available: false,
    listener_binding_allowed: false,
    startup_preview: startupPreview,
    bind_preflight: bindPreflight,
    dispatch_gate: bindPreflight.dispatch_gate,
    first_blocking_stage: firstBlockingGate.gate,
    reason: firstBlockingGate.reason,
    error_code: firstBlockingGate.error_code,
  };
}

function localMcpTransportListenerBindDescriptor(
  request: LocalMcpTransportListenerConfigRequest,
  transport: LocalMcpTransportKind,
  bindPreflight: LocalMcpTransportAdapterBindPreflightWithAcceptance,
): LocalMcpTransportListenerBindDescriptor {
  const endpoint = transport === "sse" ? request.sse?.endpoint?.trim() || null : null;
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    listener_bind_descriptor_available: true,
    listener_binding_allowed: false,
    bind_attempted: false,
    listener_started: false,
    request_loop_started: false,
    stdio_streams_attached: false,
    sse_endpoint_registered: false,
    endpoint,
    stdio_streams_present:
      transport === "stdio" &&
      Boolean(request.stdio?.stdin && request.stdio.stdout && request.stdio.stderr),
    sse_endpoint_present: transport === "sse" && Boolean(endpoint),
    bind_preflight: bindPreflight,
    bind_blocking_gate: bindPreflight.bind_blocking_gate,
    reason: bindPreflight.bind_blocking_gate.reason,
    error_code: bindPreflight.bind_blocking_gate.error_code,
  };
}

export function previewLocalMcpTransportAdapterStartup(
  transport: LocalMcpTransportKind,
  tool = "starlane.spaces.list",
): LocalMcpTransportAdapterStartupPreview {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    startup_attempted: false,
    adapter_available: false,
    listener_configured: false,
    listener_started: false,
    request_loop_started: false,
    stdio_streams_attached: false,
    sse_endpoint_registered: false,
    endpoint: null,
    bind_preflight: previewLocalMcpTransportAdapterBindPreflight(transport, tool),
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };
}

export async function previewLocalMcpTransportAdapterStartupWithAcceptance(
  transport: LocalMcpTransportKind,
  tool = "starlane.spaces.list",
  request: LocalMcpToolExecutionRequest = {},
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportAdapterStartupWithAcceptancePreview> {
  const bindPreflight = await previewLocalMcpTransportAdapterBindPreflightWithAcceptance(
    transport,
    tool,
    request,
    checkedAt,
    options,
  );

  return {
    ...previewLocalMcpTransportAdapterStartup(transport, tool),
    bind_preflight: bindPreflight,
    listener_binding_allowed: false,
    capability_token_acceptance_gate_preview:
      bindPreflight.capability_token_acceptance_gate_preview,
  };
}

export async function previewLocalMcpTransportRequestLoopRegistration(
  transport: LocalMcpTransportKind,
  tool = "starlane.spaces.list",
  request: LocalMcpToolExecutionRequest = {},
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportRequestLoopRegistrationPreview> {
  const startupPreview = await previewLocalMcpTransportAdapterStartupWithAcceptance(
    transport,
    tool,
    request,
    checkedAt,
    options,
  );
  return localMcpTransportRequestLoopRegistrationPreview(transport, startupPreview);
}

export async function previewLocalMcpTransportRequestLoopDispatch(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportRequestLoopDispatchPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const request = envelope.request ?? {};
  const registrationPreview = await previewLocalMcpTransportRequestLoopRegistration(
    transport,
    tool,
    request,
    checkedAt,
    options,
  );
  const dispatchEnvelope = {
    ...envelope,
    transport,
    tool,
    request,
  };
  const dispatchGate = await previewLocalMcpTransportDispatchGate(
    dispatchEnvelope,
    checkedAt,
    options,
  );
  const confirmedWriteJournalGate = previewLocalMcpConfirmedWriteJournalGate(dispatchEnvelope);
  const shouldInvokeHandler =
    options.transport_enabled === true &&
    ((dispatchGate.dispatch_allowed && isLocalMcpReadOnlyTool(tool)) ||
      (dispatchGate.acceptance_gate?.acceptance_allowed === true &&
        confirmedWriteJournalGate.would_record_journal));
  const handlerResponse =
    shouldInvokeHandler
      ? await handleLocalMcpTransportRequestEnvelopeWithAuthPreview(
          dispatchEnvelope,
          checkedAt,
          options,
        )
      : undefined;
  const executed = handlerResponse?.executed === true;
  const journaled =
    handlerResponse !== undefined &&
    "journaled_preflight" in handlerResponse &&
    handlerResponse.journaled_preflight?.confirmed_write_closeout.journal_recorded === true;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    request_loop_dispatch_enabled: options.transport_enabled === true,
    request_loop_registered: false,
    request_loop_started: false,
    dispatch_attempted: options.transport_enabled === true,
    request_accepted: Boolean(handlerResponse),
    handler_invoked: Boolean(handlerResponse),
    response_emitted: false,
    executed,
    journaled,
    registration_preview: registrationPreview,
    dispatch_gate: dispatchGate,
    handler_response: handlerResponse,
    first_blocking_stage: dispatchGate.blocking_gates[0]?.gate ?? null,
    reason: dispatchGate.reason,
    error_code: dispatchGate.error_code,
  };
}

export async function previewLocalMcpTransportHandlerInvocation(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportHandlerInvocationPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const requestLoopDispatch = await previewLocalMcpTransportRequestLoopDispatch(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const handlerInvoked = requestLoopDispatch.handler_invoked;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    handler_invocation_enabled: options.transport_enabled === true,
    handler_available: handlerInvoked,
    request_loop_started: false,
    request_accepted: requestLoopDispatch.request_accepted,
    handler_invoked: handlerInvoked,
    response_emitted: false,
    executed: requestLoopDispatch.executed,
    journaled: requestLoopDispatch.journaled,
    request_loop_dispatch: requestLoopDispatch,
    dispatch_gate: requestLoopDispatch.dispatch_gate,
    first_blocking_stage: requestLoopDispatch.first_blocking_stage,
    reason: requestLoopDispatch.reason,
    error_code: requestLoopDispatch.error_code,
  };
}

export async function previewLocalMcpTransportHandlerResponseFrame(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportHandlerResponseFramePreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const handlerInvocation = await previewLocalMcpTransportHandlerInvocation(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const responseEnvelope =
    handlerInvocation.request_loop_dispatch.handler_response?.executed === true
      ? handlerInvocation.request_loop_dispatch.handler_response
      : null;
  const responseEnvelopeCreated = responseEnvelope !== null;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    response_frame_enabled: options.transport_enabled === true,
    handler_available: handlerInvocation.handler_available,
    handler_invoked: handlerInvocation.handler_invoked,
    handler_result_available: responseEnvelopeCreated,
    response_envelope_created: responseEnvelopeCreated,
    response_emitted: false,
    executed: handlerInvocation.executed,
    journaled: handlerInvocation.journaled,
    response_frame: responseEnvelope,
    response_envelope: responseEnvelope,
    handler_invocation: handlerInvocation,
    dispatch_gate: handlerInvocation.dispatch_gate,
    first_blocking_stage: handlerInvocation.first_blocking_stage,
    reason: handlerInvocation.reason,
    error_code: handlerInvocation.error_code,
  };
}

export async function previewLocalMcpTransportResponseEmission(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportResponseEmissionPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const handlerResponseFrame = await previewLocalMcpTransportHandlerResponseFrame(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const responseFrameAvailable = handlerResponseFrame.response_frame !== null;
  const responseEmitted =
    options.transport_enabled === true &&
    responseFrameAvailable &&
    handlerResponseFrame.response_envelope?.executed === true;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    response_emission_enabled: options.transport_enabled === true,
    transport_output_available: responseFrameAvailable,
    response_frame_available: responseFrameAvailable,
    response_envelope_created: handlerResponseFrame.response_envelope_created,
    response_emitted: responseEmitted,
    stdio_stdout_written: responseEmitted && transport === "stdio",
    sse_event_sent: responseEmitted && transport === "sse",
    executed: handlerResponseFrame.executed,
    journaled: handlerResponseFrame.journaled,
    response_frame: handlerResponseFrame.response_frame,
    response_envelope: handlerResponseFrame.response_envelope,
    handler_response_frame: handlerResponseFrame,
    dispatch_gate: handlerResponseFrame.dispatch_gate,
    first_blocking_stage: handlerResponseFrame.first_blocking_stage,
    reason: handlerResponseFrame.reason,
    error_code: handlerResponseFrame.error_code,
  };
}

export async function previewLocalMcpTransportResponseCompletion(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportResponseCompletionPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const responseEmission = await previewLocalMcpTransportResponseEmission(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const responseCompleted = responseEmission.response_emitted === true;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    response_completion_enabled: options.transport_enabled === true,
    response_emitted: responseEmission.response_emitted,
    response_completed: responseCompleted,
    request_completed: responseCompleted,
    lifecycle_completed: false,
    cleanup_scheduled: false,
    executed: responseEmission.executed,
    journaled: responseEmission.journaled,
    response_emission: responseEmission,
    response_envelope: responseCompleted ? responseEmission.response_envelope : null,
    first_blocking_stage: responseEmission.first_blocking_stage,
    reason: responseEmission.reason,
    error_code: responseEmission.error_code,
  };
}

export async function previewLocalMcpTransportErrorEnvelopeFrame(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
): Promise<LocalMcpTransportErrorEnvelopeFramePreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const responseCompletion = await previewLocalMcpTransportResponseCompletion(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
  );

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    error_envelope_framing_enabled: false,
    error_detected: true,
    error_envelope_created: false,
    error_envelope_emitted: false,
    response_completed: false,
    request_completed: false,
    lifecycle_completed: false,
    executed: false,
    journaled: false,
    error_envelope: null,
    response_completion: responseCompletion,
    first_blocking_stage: responseCompletion.first_blocking_stage ?? "transport",
    reason: responseCompletion.reason,
    error_code: responseCompletion.error_code ?? "transport_disabled",
  };
}

export async function previewLocalMcpTransportErrorEmission(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
): Promise<LocalMcpTransportErrorEmissionPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const errorEnvelopeFrame = await previewLocalMcpTransportErrorEnvelopeFrame(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
  );

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    error_emission_enabled: false,
    error_envelope_available: false,
    error_envelope_emitted: false,
    stdio_stderr_written: false,
    sse_error_event_sent: false,
    response_completed: false,
    request_completed: false,
    lifecycle_completed: false,
    executed: false,
    journaled: false,
    error_envelope: null,
    error_envelope_frame: errorEnvelopeFrame,
    first_blocking_stage: errorEnvelopeFrame.first_blocking_stage,
    reason: errorEnvelopeFrame.reason,
    error_code: errorEnvelopeFrame.error_code,
  };
}

export async function previewLocalMcpTransportLifecycleAudit(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportLifecycleAuditPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const responseCompletion = await previewLocalMcpTransportResponseCompletion(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const errorEmission = await previewLocalMcpTransportErrorEmission(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
  );
  const responsePathCompleted = responseCompletion.response_completed === true;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    lifecycle_audit_enabled: options.transport_enabled === true,
    cleanup_scheduling_enabled: false,
    response_path_completed: responsePathCompleted,
    error_path_completed: false,
    response_completed: responseCompletion.response_completed,
    request_completed: responseCompletion.request_completed,
    lifecycle_completed: false,
    cleanup_scheduled: false,
    lifecycle_audit_recorded: false,
    executed: responseCompletion.executed,
    journaled: responseCompletion.journaled,
    response_completion: responseCompletion,
    error_emission: errorEmission,
    first_blocking_stage: responsePathCompleted
      ? responseCompletion.first_blocking_stage
      : errorEmission.first_blocking_stage,
    reason: responsePathCompleted ? responseCompletion.reason : errorEmission.reason,
    error_code: responsePathCompleted ? responseCompletion.error_code : errorEmission.error_code,
  };
}

export async function previewLocalMcpTransportRequestCloseout(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportRequestCloseoutPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const lifecycleAudit = await previewLocalMcpTransportLifecycleAudit(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const closeoutAttempted = lifecycleAudit.request_completed === true;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    request_closeout_enabled: options.transport_enabled === true,
    cleanup_queue_handoff_enabled: false,
    closeout_attempted: closeoutAttempted,
    cleanup_queue_handoff_attempted: false,
    cleanup_queue_enqueued: false,
    response_completed: lifecycleAudit.response_completed,
    request_completed: lifecycleAudit.request_completed,
    lifecycle_completed: false,
    cleanup_scheduled: false,
    lifecycle_audit_recorded: false,
    executed: lifecycleAudit.executed,
    journaled: lifecycleAudit.journaled,
    lifecycle_audit: lifecycleAudit,
    first_blocking_stage: lifecycleAudit.first_blocking_stage,
    reason: lifecycleAudit.reason,
    error_code: lifecycleAudit.error_code,
  };
}

export async function previewLocalMcpTransportTerminalCleanup(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportTerminalCleanupPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const requestCloseout = await previewLocalMcpTransportRequestCloseout(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const terminalCleanupAttempted = requestCloseout.closeout_attempted === true;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    terminal_cleanup_enabled: options.transport_enabled === true,
    adapter_closeout_handoff_enabled: false,
    terminal_cleanup_attempted: terminalCleanupAttempted,
    adapter_closeout_handoff_attempted: false,
    adapter_closeout_handed_off: false,
    cleanup_queue_enqueued: false,
    request_completed: requestCloseout.request_completed,
    lifecycle_completed: false,
    cleanup_scheduled: false,
    lifecycle_audit_recorded: false,
    executed: requestCloseout.executed,
    journaled: requestCloseout.journaled,
    request_closeout: requestCloseout,
    first_blocking_stage: requestCloseout.first_blocking_stage,
    reason: requestCloseout.reason,
    error_code: requestCloseout.error_code,
  };
}

export async function previewLocalMcpTransportHandlerTerminalState(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportHandlerTerminalStatePreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const terminalCleanup = await previewLocalMcpTransportTerminalCleanup(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const terminalStateReady =
    options.transport_enabled === true &&
    terminalCleanup.terminal_cleanup_attempted &&
    terminalCleanup.request_completed;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    handler_terminal_state_enabled: options.transport_enabled === true,
    terminal_state_recorded: false,
    handler_available: terminalStateReady,
    handled: terminalStateReady,
    request_accepted: terminalStateReady,
    response_emitted: terminalStateReady,
    request_completed: terminalCleanup.request_completed,
    lifecycle_completed: false,
    cleanup_scheduled: false,
    cleanup_queue_enqueued: false,
    executed: terminalCleanup.executed,
    journaled: terminalCleanup.journaled,
    terminal_cleanup: terminalCleanup,
    first_blocking_stage: terminalCleanup.first_blocking_stage,
    reason: terminalCleanup.reason,
    error_code: terminalCleanup.error_code,
  };
}

export async function previewLocalMcpTransportEnablementReadiness(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportEnablementReadinessPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const transportReadiness = localMcpTransportReadiness(transport, tool);
  const handlerTerminalState = await previewLocalMcpTransportHandlerTerminalState(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const terminalCleanup = await previewLocalMcpTransportTerminalCleanup(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const dispatchGate = await previewLocalMcpTransportDispatchGate(
    {
      ...envelope,
      transport,
      tool,
      request: envelope.request ?? {},
    },
    checkedAt,
    options,
  );
  const firstBlockingGate = dispatchGate.blocking_gates[0] ?? null;
  const acceptedReadReady =
    options.transport_enabled === true &&
    dispatchGate.dispatch_allowed &&
    terminalCleanup.terminal_cleanup_attempted &&
    terminalCleanup.request_completed;
  const operationJournalReady =
    acceptedReadReady &&
    !dispatchGate.blocking_gates.some((gate) => gate.gate === "operation_journal");

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    transport_enablement_readiness_enabled: options.transport_enabled === true,
    transport_enablement_allowed: acceptedReadReady,
    stdio_enablement_allowed: acceptedReadReady && transport === "stdio",
    sse_enablement_allowed: acceptedReadReady && transport === "sse",
    handler_terminal_state_ready: acceptedReadReady,
    capability_token_acceptance_ready:
      dispatchGate.acceptance_gate?.acceptance_allowed === true,
    operation_journal_ready: operationJournalReady,
    confirmed_write_ready: false,
    listener_binding_ready: acceptedReadReady,
    request_loop_ready: acceptedReadReady,
    response_pipeline_ready: acceptedReadReady,
    cleanup_pipeline_ready: acceptedReadReady,
    executed: terminalCleanup.executed,
    journaled: terminalCleanup.journaled,
    transport_readiness: transportReadiness,
    handler_terminal_state: handlerTerminalState,
    dispatch_gate: dispatchGate,
    blocking_gates: dispatchGate.blocking_gates,
    first_blocking_stage: firstBlockingGate?.gate ?? null,
    reason: dispatchGate.reason,
    error_code: dispatchGate.error_code ?? firstBlockingGate?.error_code,
  };
}

export async function previewLocalMcpCapabilityTokenAcceptanceReadiness(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
): Promise<LocalMcpCapabilityTokenAcceptanceReadinessPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const request = envelope.request ?? {};
  const acceptanceGate = await previewLocalMcpCapabilityTokenAcceptanceGate(
    request.capability_token,
    checkedAt,
  );
  const record = acceptanceGate.auth_gate_preview.validation_preview.record;
  const firstBlockingGate = acceptanceGate.blocking_gates[0] ?? {
    gate: "token_acceptance" as const,
    status: "disabled" as const,
    reason: "capability_token_acceptance_not_enabled" as const,
    error_code: "capability_token_not_enabled" as const,
  };
  const acceptedTokenPersisted = record?.accepted === true;
  const canPersistAcceptance = acceptanceGate.token_checks_passed && Boolean(record) && !acceptedTokenPersisted;

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    acceptance_persistence_enabled: false,
    persistence_allowed: false,
    can_persist_acceptance: canPersistAcceptance,
    token_record_found: Boolean(record),
    token_checks_passed: acceptanceGate.token_checks_passed,
    token_accepted: acceptedTokenPersisted,
    accepted_token_persisted: acceptedTokenPersisted,
    accepted_at: record?.accepted_at ?? null,
    raw_token_stored: false,
    requires_existing_record: true,
    requires_token_checks_passed: true,
    requires_unexpired_token: true,
    requires_unrevoked_token: true,
    requires_accepted_false: true,
    requires_raw_token_storage_false: true,
    accepted_at_field_required: true,
    listener_binding_allowed: false,
    transport_dispatch_allowed: false,
    executed: false,
    journaled: false,
    ...(record ? { record } : {}),
    acceptance_gate: acceptanceGate,
    blocking_gates: acceptanceGate.blocking_gates,
    first_blocking_stage: firstBlockingGate.gate,
    reason: acceptanceGate.reason,
    error_code: firstBlockingGate.error_code,
  };
}

export async function acceptLocalMcpCapabilityTokenRecord(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpCapabilityTokenAcceptOptions = {},
): Promise<LocalMcpCapabilityTokenAcceptResult> {
  const existing = listLocalMcpCapabilityTokenRecords();
  const acceptanceReadiness =
    await previewLocalMcpCapabilityTokenAcceptanceReadiness(envelope, checkedAt);
  const acceptanceEnabled = options.acceptance_enabled === true;
  if (!acceptanceEnabled) {
    return {
      token_accepted: false,
      acceptance_enabled: false,
      reason: acceptanceReadiness.reason,
      records: existing,
      acceptance_readiness: acceptanceReadiness,
    };
  }

  const acceptedAtMs = Date.parse(options.accepted_at ?? checkedAt);
  if (!Number.isFinite(acceptedAtMs)) {
    return {
      token_accepted: false,
      acceptance_enabled: true,
      reason: "accepted_at_invalid",
      records: existing,
      acceptance_readiness: acceptanceReadiness,
    };
  }

  const record = acceptanceReadiness.record;
  if (!record) {
    return {
      token_accepted: false,
      acceptance_enabled: true,
      reason: acceptanceReadiness.reason,
      records: existing,
      acceptance_readiness: acceptanceReadiness,
    };
  }
  if (!acceptanceReadiness.can_persist_acceptance) {
    return {
      token_accepted: false,
      acceptance_enabled: true,
      reason: record.accepted ? "accepted_token_already_persisted" : acceptanceReadiness.reason,
      ...(record.accepted ? { record } : {}),
      records: existing,
      acceptance_readiness: acceptanceReadiness,
    };
  }

  const acceptedRecord: LocalMcpCapabilityTokenRecord = {
    ...record,
    accepted: true,
    accepted_at: new Date(acceptedAtMs).toISOString(),
    raw_token_stored: false,
    reason: "capability_token_accepted",
  };
  const nextRecords = [
    ...existing.filter((candidate) => candidate.token_id !== acceptedRecord.token_id),
    acceptedRecord,
  ];
  writeLocalMcpCapabilityTokenRecords(nextRecords);
  return {
    token_accepted: true,
    acceptance_enabled: true,
    record: acceptedRecord,
    records: nextRecords,
    acceptance_readiness: acceptanceReadiness,
  };
}

export function previewLocalMcpTransportAdapterShutdown(
  transport: LocalMcpTransportKind,
  tool = "starlane.spaces.list",
): LocalMcpTransportAdapterShutdownPreview {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    shutdown_attempted: false,
    adapter_available: false,
    listener_was_started: false,
    request_loop_was_started: false,
    listener_closed: false,
    request_loop_stopped: false,
    stdio_streams_detached: false,
    sse_endpoint_unregistered: false,
    pending_requests_drained: false,
    error_state_recorded: false,
    startup_preview: previewLocalMcpTransportAdapterStartup(transport, tool),
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };
}

export async function previewLocalMcpTransportAdapterRuntimeState(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportAdapterRuntimeStatePreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const request = envelope.request ?? {};
  const bindPreflight = await previewLocalMcpTransportAdapterBindPreflightWithAcceptance(
    transport,
    tool,
    request,
    checkedAt,
    options,
  );
  const startupPreview = await previewLocalMcpTransportAdapterStartupWithAcceptance(
    transport,
    tool,
    request,
    checkedAt,
    options,
  );
  const shutdownPreview = {
    ...previewLocalMcpTransportAdapterShutdown(transport, tool),
    startup_preview: startupPreview,
  };
  const responsePreview = await previewLocalMcpTransportAdapterResponse(envelope, checkedAt, options);
  const lifecyclePreview = await previewLocalMcpTransportAdapterLifecycle(
    envelope,
    checkedAt,
    options,
  );

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    adapter_available: false,
    runtime_active: false,
    bound: false,
    listener_started: false,
    request_loop_started: false,
    request_accepted: false,
    response_emitted: false,
    lifecycle_completed: false,
    executed: false,
    journaled: false,
    bind_preflight: bindPreflight,
    startup_preview: startupPreview,
    shutdown_preview: shutdownPreview,
    request_preview: responsePreview.request_preview,
    response_preview: responsePreview,
    lifecycle_preview: lifecyclePreview,
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };
}

export async function previewLocalMcpTransportHandlerSkeleton(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportHandlerSkeletonPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  const tool = envelope.tool || "starlane.spaces.list";
  const request = envelope.request ?? {};
  const bindPreflight = await previewLocalMcpTransportAdapterBindPreflightWithAcceptance(
    transport,
    tool,
    request,
    checkedAt,
    options,
  );
  const startupPreview = await previewLocalMcpTransportAdapterStartupWithAcceptance(
    transport,
    tool,
    request,
    checkedAt,
    options,
  );
  const requestLoopRegistration = await previewLocalMcpTransportRequestLoopRegistration(
    transport,
    tool,
    request,
    checkedAt,
    options,
  );
  const requestLoopDispatch = await previewLocalMcpTransportRequestLoopDispatch(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const handlerInvocation = await previewLocalMcpTransportHandlerInvocation(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const handlerResponseFrame = await previewLocalMcpTransportHandlerResponseFrame(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const responseEmission = await previewLocalMcpTransportResponseEmission(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const responseCompletion = await previewLocalMcpTransportResponseCompletion(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const errorEnvelopeFrame = await previewLocalMcpTransportErrorEnvelopeFrame(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
  );
  const errorEmission = await previewLocalMcpTransportErrorEmission(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
  );
  const lifecycleAudit = await previewLocalMcpTransportLifecycleAudit(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const requestCloseout = await previewLocalMcpTransportRequestCloseout(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const terminalCleanup = await previewLocalMcpTransportTerminalCleanup(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const handlerTerminalState = await previewLocalMcpTransportHandlerTerminalState(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const transportEnablementReadiness = await previewLocalMcpTransportEnablementReadiness(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );
  const capabilityTokenAcceptanceReadiness =
    await previewLocalMcpCapabilityTokenAcceptanceReadiness(
      {
        ...envelope,
        transport,
        tool,
      },
      checkedAt,
    );
  const dispatchGate = await previewLocalMcpTransportDispatchGate(envelope, checkedAt, options);
  const preflight = preflightLocalMcpTransportRequest(transport, tool, request);
  const journalGatePreview = previewLocalMcpConfirmedWriteJournalGate({
    ...envelope,
    transport,
    tool,
  });
  const journalReplayPreview = previewLocalMcpOperationJournalReplay(envelope.id);
  const runtimeState = await previewLocalMcpTransportAdapterRuntimeState(
    {
      ...envelope,
      transport,
      tool,
    },
    checkedAt,
    options,
  );

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    handler_available: handlerTerminalState.handler_available,
    handler_started: false,
    listener_binding_allowed: false,
    request_loop_started: false,
    request_accepted: handlerTerminalState.request_accepted,
    response_emitted: handlerTerminalState.response_emitted,
    executed: handlerTerminalState.executed,
    journaled: handlerTerminalState.journaled,
    bind_preflight: bindPreflight,
    startup_preview: startupPreview,
    request_loop_registration: requestLoopRegistration,
    request_loop_dispatch: requestLoopDispatch,
    handler_invocation: handlerInvocation,
    handler_response_frame: handlerResponseFrame,
    response_emission: responseEmission,
    response_completion: responseCompletion,
    error_envelope_frame: errorEnvelopeFrame,
    error_emission: errorEmission,
    lifecycle_audit: lifecycleAudit,
    request_closeout: requestCloseout,
    terminal_cleanup: terminalCleanup,
    handler_terminal_state: handlerTerminalState,
    transport_enablement_readiness: transportEnablementReadiness,
    capability_token_acceptance_readiness: capabilityTokenAcceptanceReadiness,
    dispatch_gate: dispatchGate,
    preflight,
    operation_journal_gate_preview: journalGatePreview,
    operation_journal_replay_preview: journalReplayPreview,
    runtime_state: runtimeState,
    reason: dispatchGate.reason,
    error_code: dispatchGate.error_code,
  };
}

export async function handleLocalMcpTransportHandlerSkeleton(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
): Promise<LocalMcpTransportHandlerSkeletonResponse> {
  const skeleton = await previewLocalMcpTransportHandlerSkeleton(envelope, checkedAt);
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport: skeleton.transport,
    handler_available: false,
    handled: false,
    response_emitted: false,
    executed: false,
    journaled: false,
    response_envelope: null,
    skeleton,
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };
}

export async function previewLocalMcpTransportAdapterRequest(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportAdapterRequestPreview> {
  const transport = isLocalMcpTransportKind(envelope.transport) ? envelope.transport : "stdio";
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport,
    adapter_available: false,
    bindable: false,
    request_accepted: false,
    response_emitted: false,
    executed: false,
    dispatch_gate: await previewLocalMcpTransportDispatchGate(envelope, checkedAt, options),
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };
}

export async function previewLocalMcpTransportAdapterResponse(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportAdapterResponsePreview> {
  const requestPreview = await previewLocalMcpTransportAdapterRequest(
    envelope,
    checkedAt,
    options,
  );
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport: requestPreview.transport,
    adapter_available: false,
    request_accepted: false,
    response_emitted: false,
    executed: false,
    response_envelope: null,
    request_preview: requestPreview,
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };
}

export async function previewLocalMcpTransportAdapterLifecycle(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportAdapterLifecyclePreview> {
  const responsePreview = await previewLocalMcpTransportAdapterResponse(
    envelope,
    checkedAt,
    options,
  );
  const stages = localMcpTransportLifecycleContract().disabled_stages.map<
    LocalMcpTransportAdapterLifecycleStagePreview
  >((stage) => ({
    stage,
    enabled: false,
    completed: false,
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  }));

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport: responsePreview.transport,
    lifecycle_enabled: false,
    adapter_available: false,
    request_accepted: false,
    response_emitted: false,
    executed: false,
    journaled: false,
    completed: false,
    first_blocking_stage: "configure",
    stages,
    request_preview: responsePreview.request_preview,
    response_preview: responsePreview,
    reason: "transport_not_enabled",
    error_code: "transport_disabled",
  };
}

export function localMcpTransportLifecycleContract(): LocalMcpTransportLifecycleContract {
  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    lifecycle_enabled: false,
    enabled_stages: [],
    disabled_stages: [
      "configure",
      "bind",
      "authenticate",
      "preflight",
      "dispatch",
      "respond",
      "journal",
      "shutdown",
    ],
    first_enabled_stage: null,
    terminal_stage: "shutdown",
    reason: "transport_not_enabled",
  };
}

export function localMcpRuntimeManifest(): LocalMcpRuntimeManifest {
  const tools = listLocalMcpReadOnlyTools().map<LocalMcpToolManifestEntry>((name) => ({
    name,
    read_only: true,
    runtime_modes: ["local_vault"],
  }));

  return {
    schema_version: 1,
    runtime_mode: "local_vault",
    transport_status: "helper_only",
    read_only: true,
    tool_count: tools.length,
    tools,
    blocked_tools: listLocalMcpBlockedTools(),
    planned_transports: listLocalMcpTransportStatuses(),
    capability_token: localMcpCapabilityTokenPolicy(),
    capability_token_lifecycle: localMcpCapabilityTokenLifecyclePolicy(),
    capability_token_auth_readiness: localMcpCapabilityTokenAuthReadiness(),
    capability_token_auth_contract: localMcpCapabilityTokenAuthContract(),
    operation_journal: localMcpOperationJournalPolicy(),
    operation_journal_contract: localMcpOperationJournalContract(),
    operation_journal_record_contract: localMcpOperationJournalRecordContract(),
    transport_capabilities: localMcpTransportCapabilitySummary(),
    transport_execution_contract: localMcpTransportExecutionContract(),
    transport_envelope_contract: localMcpTransportEnvelopeContract(),
    transport_dispatch_contract: localMcpTransportDispatchContract(),
    transport_error_contract: localMcpTransportErrorContract(),
    transport_result_contract: localMcpTransportResultContract(),
    transport_retry_contract: localMcpTransportRetryContract(),
    transport_idempotency_contract: localMcpTransportIdempotencyContract(),
    transport_side_effect_contract: localMcpTransportSideEffectContract(),
    transport_adapter_contract: localMcpTransportAdapterContract(),
    transport_listener_config_contract: localMcpTransportListenerConfigContract(),
    transport_lifecycle_contract: localMcpTransportLifecycleContract(),
    write_block_contract: localMcpWriteBlockContract(),
    trace_contract: localMcpTransportRequestTraceContract(),
    request_trace_policy: localMcpTransportRequestTracePolicy(),
  };
}

export function planLocalMcpToolExecution(name: string): LocalMcpToolSafetyPlan {
  if (isLocalMcpReadOnlyTool(name)) {
    return {
      name,
      availability: "executable",
      read_only: true,
      runtime_mode: "local_vault",
      requires_confirmation: false,
    };
  }

  if (LOCAL_MCP_WRITE_BLOCKED_TOOLS.has(name)) {
    return {
      name,
      availability: "confirmation_required",
      read_only: false,
      runtime_mode: "local_vault",
      requires_confirmation: true,
      reason: "write_not_enabled",
    };
  }

  if (LOCAL_MCP_NOT_IMPLEMENTED_TOOLS.has(name)) {
    return {
      name,
      availability: "not_implemented",
      read_only: false,
      runtime_mode: "local_vault",
      requires_confirmation: false,
      reason: "not_implemented",
    };
  }

  return {
    name,
    availability: "unknown",
    read_only: false,
    runtime_mode: "local_vault",
    requires_confirmation: false,
    reason: "unknown_tool",
  };
}

export function buildLocalMcpConfirmationChallenge(
  name: string,
): LocalMcpConfirmationChallenge | null {
  const plan = planLocalMcpToolExecution(name);
  if (plan.availability !== "confirmation_required" || plan.reason !== "write_not_enabled") {
    return null;
  }

  return {
    tool: name,
    runtime_mode: "local_vault",
    required: true,
    confirmation_text: `confirm local mcp ${name}`,
    reason: "write_not_enabled",
  };
}

export function validateLocalMcpConfirmation(
  name: string,
  confirmationText: string | undefined | null,
): LocalMcpConfirmationValidationResult {
  const challenge = buildLocalMcpConfirmationChallenge(name);
  if (!challenge) {
    return {
      tool: name,
      valid: false,
      runtime_mode: "local_vault",
      reason: "confirmation_not_required",
    };
  }

  const valid = confirmationText === challenge.confirmation_text;
  return {
    tool: name,
    valid,
    runtime_mode: "local_vault",
    reason: valid ? undefined : "confirmation_mismatch",
  };
}

export function guardLocalMcpToolExecution(
  name: string,
  request: LocalMcpToolExecutionRequest = {},
): LocalMcpToolExecutionGuard {
  const plan = planLocalMcpToolExecution(name);
  if (plan.availability === "executable") {
    return {
      tool: name,
      executable: true,
      runtime_mode: "local_vault",
      plan,
    };
  }

  if (plan.availability === "confirmation_required") {
    const confirmationChallenge = buildLocalMcpConfirmationChallenge(name) ?? undefined;
    return {
      tool: name,
      executable: false,
      runtime_mode: "local_vault",
      plan,
      confirmation_challenge: confirmationChallenge,
      confirmation_validation: validateLocalMcpConfirmation(name, request.confirmation_text),
      rejection_reason: "write_execution_disabled",
    };
  }

  return {
    tool: name,
    executable: false,
    runtime_mode: "local_vault",
    plan,
    rejection_reason: plan.reason === "not_implemented" ? "not_implemented" : "unknown_tool",
  };
}

export function previewLocalMcpOperationJournal(
  name: string,
): LocalMcpOperationJournalPreview {
  const plan = planLocalMcpToolExecution(name);
  if (plan.availability === "executable") {
    return {
      journal_enabled: false,
      reason: "journal_not_required",
      runtime_mode: "local_vault",
      tool: name,
      operation: "read",
      required: false,
      planned_fields: localMcpOperationJournalPolicy().planned_fields,
    };
  }

  if (plan.availability === "confirmation_required") {
    return {
      journal_enabled: false,
      reason: "operation_journal_not_enabled",
      runtime_mode: "local_vault",
      tool: name,
      operation: "write",
      required: true,
      planned_fields: localMcpOperationJournalPolicy().planned_fields,
    };
  }

  return {
    journal_enabled: false,
    reason: "journal_not_required",
    runtime_mode: "local_vault",
    tool: name,
    operation: "unknown",
    required: false,
    planned_fields: localMcpOperationJournalPolicy().planned_fields,
  };
}

export function localMcpOperationJournalReadiness(
  tool: string,
): LocalMcpOperationJournalReadiness {
  const policy = localMcpOperationJournalPolicy();
  return {
    runtime_mode: "local_vault",
    status: policy.status,
    journal_enabled: false,
    write_journal_ready: false,
    required_for_writes: true,
    policy,
    preview: previewLocalMcpOperationJournal(tool),
    blocking_gates: [
      {
        gate: "operation_journal",
        status: "planned",
        reason: "operation_journal_not_enabled",
        error_code: "transport_disabled",
      },
    ],
  };
}

function localMcpOperationJournalStorage(): Storage | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage;
}

function parseLocalMcpOperationJournalRecord(
  value: unknown,
): LocalMcpOperationJournalRecord | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Partial<LocalMcpOperationJournalRecord>;
  if (
    typeof record.id !== "string" ||
    typeof record.timestamp !== "string" ||
    record.runtime_mode !== "local_vault" ||
    (record.transport !== "stdio" && record.transport !== "sse") ||
    typeof record.tool !== "string" ||
    record.operation !== "write" ||
    (typeof record.target !== "string" && record.target !== null) ||
    typeof record.confirmation_text_present !== "boolean" ||
    (typeof record.capability_token_fingerprint !== "string" &&
      record.capability_token_fingerprint !== null) ||
    record.raw_confirmation_text_stored !== false ||
    record.raw_capability_token_stored !== false ||
    record.executed !== false ||
    record.persisted !== true ||
    record.reason !== "write_execution_disabled"
  ) {
    return null;
  }
  return record as LocalMcpOperationJournalRecord;
}

export function listLocalMcpOperationJournalRecords(): LocalMcpOperationJournalRecord[] {
  const storage = localMcpOperationJournalStorage();
  if (!storage) return [];
  try {
    const parsed = JSON.parse(
      storage.getItem(LOCAL_MCP_OPERATION_JOURNAL_STORAGE_KEY) ?? "[]",
    ) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map(parseLocalMcpOperationJournalRecord)
      .filter((record): record is LocalMcpOperationJournalRecord => Boolean(record));
  } catch {
    return [];
  }
}

function writeLocalMcpOperationJournalRecords(records: LocalMcpOperationJournalRecord[]): void {
  const storage = localMcpOperationJournalStorage();
  if (!storage) return;
  storage.setItem(LOCAL_MCP_OPERATION_JOURNAL_STORAGE_KEY, JSON.stringify(records));
}

export function clearLocalMcpOperationJournalRecords(): void {
  localMcpOperationJournalStorage()?.removeItem(LOCAL_MCP_OPERATION_JOURNAL_STORAGE_KEY);
}

export function previewLocalMcpOperationJournalReplay(
  requestId: string | undefined | null,
): LocalMcpOperationJournalReplayPreview {
  const trimmedRequestId = requestId?.trim() ?? "";
  if (!trimmedRequestId) {
    return {
      runtime_mode: "local_vault",
      request_id_present: false,
      idempotency_key: null,
      replay_detected: false,
      record_write_allowed: false,
      reason: "request_id_required",
      error_code: "invalid_request",
    };
  }

  const idempotencyKey = `local-mcp-journal:${trimmedRequestId}`;
  const existingRecord = listLocalMcpOperationJournalRecords().find(
    (record) => record.id === idempotencyKey,
  );

  return {
    runtime_mode: "local_vault",
    request_id_present: true,
    idempotency_key: idempotencyKey,
    replay_detected: Boolean(existingRecord),
    record_write_allowed: !existingRecord,
    existing_record: existingRecord,
    reason: existingRecord ? "journal_replay_detected" : "journal_replay_not_found",
    error_code: existingRecord ? "transport_disabled" : "transport_disabled",
  };
}

async function fingerprintLocalMcpCapabilityToken(
  token?: string,
): Promise<string | null> {
  if (!token?.trim()) return null;
  if (typeof crypto === "undefined" || !crypto.subtle) return null;
  const bytes = new TextEncoder().encode(token);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const hex = Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  return `sha256:${hex}`;
}

export async function appendLocalMcpOperationJournalRecord(
  request: LocalMcpOperationJournalRecordWriteRequest,
): Promise<LocalMcpOperationJournalRecordWriteResult> {
  const existing = listLocalMcpOperationJournalRecords();
  if (!request.request_id.trim()) {
    return { record_written: false, reason: "request_id_required", records: existing };
  }
  if (!request.timestamp.trim()) {
    return { record_written: false, reason: "timestamp_required", records: existing };
  }

  const replayPreview = previewLocalMcpOperationJournalReplay(request.request_id);
  if (replayPreview.replay_detected) {
    return {
      record_written: false,
      reason: "journal_replay_detected",
      record: replayPreview.existing_record,
      records: existing,
      operation_journal_replay_preview: replayPreview,
    };
  }

  const preview = previewLocalMcpOperationJournal(request.tool);
  if (!preview.required || preview.operation !== "write") {
    return {
      record_written: false,
      reason: "journal_not_required",
      records: existing,
      operation_journal_replay_preview: replayPreview,
    };
  }

  const record: LocalMcpOperationJournalRecord = {
    id: `local-mcp-journal:${request.request_id}`,
    timestamp: request.timestamp,
    runtime_mode: "local_vault",
    transport: request.transport,
    tool: request.tool,
    operation: "write",
    target: request.target?.trim() || null,
    confirmation_text_present: Boolean(request.confirmation_text?.trim()),
    capability_token_fingerprint: await fingerprintLocalMcpCapabilityToken(
      request.capability_token,
    ),
    raw_confirmation_text_stored: false,
    raw_capability_token_stored: false,
    executed: false,
    persisted: true,
    reason: "write_execution_disabled",
  };
  const nextRecords = [...existing, record];
  writeLocalMcpOperationJournalRecords(nextRecords);
  return {
    record_written: true,
    record,
    records: nextRecords,
    operation_journal_replay_preview: replayPreview,
  };
}

export function localMcpWriteExecutionReadiness(tool: string): LocalMcpWriteExecutionReadiness {
  const plan = planLocalMcpToolExecution(tool);
  if (plan.availability === "executable") {
    return {
      runtime_mode: "local_vault",
      status: "not_required",
      write_execution_enabled: false,
      reason: "write_not_required",
    };
  }

  if (plan.availability === "confirmation_required") {
    return {
      runtime_mode: "local_vault",
      status: "disabled",
      write_execution_enabled: false,
      reason: "write_not_enabled",
      error_code: "write_execution_disabled",
    };
  }

  if (plan.availability === "not_implemented") {
    return {
      runtime_mode: "local_vault",
      status: "not_implemented",
      write_execution_enabled: false,
      reason: "tool_not_implemented",
      error_code: "tool_not_implemented",
    };
  }

  return {
    runtime_mode: "local_vault",
    status: "unknown",
    write_execution_enabled: false,
    reason: "unknown_tool",
    error_code: "tool_unknown",
  };
}

export function localMcpTransportReadiness(
  transport: LocalMcpTransportKind,
  tool: string,
): LocalMcpTransportReadiness {
  const plan = planLocalMcpToolExecution(tool);
  const auth = localMcpCapabilityTokenAuthReadiness();
  const operationJournal = localMcpOperationJournalReadiness(tool);
  const writeExecution = localMcpWriteExecutionReadiness(tool);
  const blockingGates: LocalMcpTransportBlockingGate[] = [
    {
      gate: "transport",
      status: "disabled",
      error_code: "transport_disabled",
      reason: "transport_not_enabled",
    },
    {
      gate: "capability_token",
      status: "disabled",
      error_code: "capability_token_required",
      reason: "capability_token_not_enabled",
    },
  ];

  if (operationJournal.preview.required) {
    blockingGates.push({
      gate: "operation_journal",
      status: "planned",
      error_code: "transport_disabled",
      reason: "operation_journal_not_enabled",
    });
  }

  if (writeExecution.status === "disabled") {
    blockingGates.push({
      gate: "write_execution",
      status: "disabled",
      error_code: "write_execution_disabled",
      reason: "write_not_enabled",
    });
  }

  return {
    runtime_mode: "local_vault",
    transport,
    transport_available: false,
    status: "disabled",
    tool,
    tool_availability: plan.availability,
    read_only: plan.read_only,
    executable: false,
    auth,
    operation_journal: operationJournal,
    write_execution: writeExecution,
    blocking_gates: blockingGates,
  };
}

export function isLocalMcpTransportKind(value: string): value is LocalMcpTransportKind {
  return LOCAL_MCP_PLANNED_TRANSPORTS.includes(value as LocalMcpTransportKind);
}

export function preflightLocalMcpTransportRequest(
  transport: LocalMcpTransportKind,
  tool: string,
  request: LocalMcpToolExecutionRequest = {},
): LocalMcpTransportPreflight {
  const capabilityToken = validateLocalMcpCapabilityToken(request.capability_token);
  const guard = guardLocalMcpToolExecution(tool, request);
  const toolErrorCode = localMcpGuardErrorCode(guard);
  return {
    transport,
    runtime_mode: "local_vault",
    transport_available: false,
    reason:
      capabilityToken.reason === "capability_token_missing"
        ? "capability_token_not_enabled"
        : capabilityToken.reason,
    error_code: localMcpPreflightErrorCode(capabilityToken),
    ...(toolErrorCode ? { tool_error_code: toolErrorCode } : {}),
    capability_token: capabilityToken,
    capability_token_auth_readiness: localMcpCapabilityTokenAuthReadiness(),
    operation_journal_readiness: localMcpOperationJournalReadiness(tool),
    transport_readiness: localMcpTransportReadiness(transport, tool),
    guard,
  };
}

export async function preflightLocalMcpTransportRequestWithAuthPreview(
  transport: LocalMcpTransportKind,
  tool: string,
  request: LocalMcpToolExecutionRequest = {},
  checkedAt: string,
): Promise<LocalMcpTransportAuthPreviewPreflight> {
  const capabilityTokenAuthGatePreview = await previewLocalMcpCapabilityTokenAuthGate(
    request.capability_token,
    checkedAt,
  );
  const capabilityTokenAcceptanceGatePreview =
    localMcpCapabilityTokenAcceptanceGateFromAuthPreview(capabilityTokenAuthGatePreview);
  return {
    preflight: preflightLocalMcpTransportRequest(transport, tool, request),
    capability_token_validation_preview: capabilityTokenAuthGatePreview.validation_preview,
    capability_token_auth_gate_preview: capabilityTokenAuthGatePreview,
    capability_token_acceptance_gate_preview: capabilityTokenAcceptanceGatePreview,
  };
}

export async function previewLocalMcpTransportDispatchGate(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportDispatchGatePreview> {
  const envelopeValidation = validateLocalMcpTransportRequestEnvelope(envelope);
  if (!envelopeValidation.valid) {
    const reason = envelopeValidation.reason ?? "request_id_required";
    return {
      runtime_mode: "local_vault",
      dispatch_allowed: false,
      executed: false,
      transport: isLocalMcpTransportKind(envelope.transport) ? envelope.transport : undefined,
      tool: envelope.tool,
      envelope_validation: envelopeValidation,
      blocking_gates: [
        {
          gate: "envelope",
          status: "blocked",
          reason,
          error_code: envelopeValidation.error_code ?? "invalid_request",
        },
      ],
      reason,
      error_code: envelopeValidation.error_code ?? "invalid_request",
    };
  }

  const request = envelope.request ?? {};
  const acceptanceGate = await previewLocalMcpCapabilityTokenAcceptanceGate(
    request.capability_token,
    checkedAt,
  );
  const authGate = acceptanceGate.auth_gate_preview;
  const transportReadiness = localMcpTransportReadiness(envelope.transport, envelope.tool);
  const operationJournalReadiness = localMcpOperationJournalReadiness(envelope.tool);
  const guard = guardLocalMcpToolExecution(envelope.tool, request);
  const blockingGates: LocalMcpTransportDispatchGateBlockingGate[] = [];

  if (!authGate.token_checks_passed) {
    const capabilityGate = authGate.blocking_gates.find(
      (gate) => gate.gate === "capability_token",
    );
    const reason =
      capabilityGate?.reason ??
      (authGate.reason === "capability_token_ready"
        ? "capability_token_not_enabled"
        : authGate.reason);
    blockingGates.push({
      gate: "capability_token",
      status: "blocked",
      reason,
      error_code: capabilityGate?.error_code ?? "capability_token_not_enabled",
    });
  } else if (!acceptanceGate.acceptance_allowed) {
    const tokenAcceptanceGate = acceptanceGate.blocking_gates.find(
      (gate) => gate.gate === "token_acceptance",
    );
    blockingGates.push({
      gate: "token_acceptance",
      status: "disabled",
      reason:
        !tokenAcceptanceGate || tokenAcceptanceGate.reason === "capability_token_accepted"
          ? "capability_token_acceptance_not_enabled"
          : tokenAcceptanceGate.reason,
      error_code: tokenAcceptanceGate?.error_code ?? "capability_token_not_enabled",
    });
  }

  if (options.transport_enabled !== true) {
    blockingGates.push({
      gate: "transport",
      status: "disabled",
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
    });
  }

  if (operationJournalReadiness.preview.required) {
    blockingGates.push({
      gate: "operation_journal",
      status: "planned",
      reason: "operation_journal_not_enabled",
      error_code: "transport_disabled",
    });
  }

  if (!guard.executable) {
    const guardErrorCode = localMcpGuardErrorCode(guard) ?? "tool_unknown";
    blockingGates.push({
      gate: guard.rejection_reason === "write_execution_disabled" ? "write_execution" : "tool_guard",
      status:
        guard.rejection_reason === "write_execution_disabled" ? "disabled" : "blocked",
      reason:
        guard.rejection_reason === "write_execution_disabled"
          ? "write_execution_disabled"
          : guard.rejection_reason === "not_implemented"
            ? "tool_not_implemented"
            : "tool_unknown",
      error_code: guardErrorCode,
    });
  }

  return {
    runtime_mode: "local_vault",
    dispatch_allowed: blockingGates.length === 0,
    executed: false,
    transport: envelope.transport,
    tool: envelope.tool,
    envelope_validation: envelopeValidation,
    auth_gate: authGate,
    acceptance_gate: acceptanceGate,
    transport_readiness: transportReadiness,
    operation_journal_readiness: operationJournalReadiness,
    guard,
    blocking_gates: blockingGates,
    reason: blockingGates[0]?.reason ?? "dispatch_allowed",
    error_code: blockingGates[0]?.error_code,
    transport_gate_enabled: options.transport_enabled === true,
  };
}

function localMcpOperationJournalTarget(input: unknown): string | undefined {
  if (!input || typeof input !== "object") return undefined;
  const record = input as Record<string, unknown>;
  for (const key of ["document_id", "doc_id", "id", "space_id", "folder_id"]) {
    const value = record[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return undefined;
}

export function previewLocalMcpConfirmedWriteJournalGate(
  envelope: LocalMcpTransportRequestEnvelope,
): LocalMcpConfirmedWriteJournalGatePreview {
  const request = envelope.request ?? {};
  const operationJournalReadiness = localMcpOperationJournalReadiness(envelope.tool);
  const guard = guardLocalMcpToolExecution(envelope.tool, request);
  const confirmationValid = guard.confirmation_validation?.valid === true;
  const journalRequired =
    operationJournalReadiness.preview.required &&
    operationJournalReadiness.preview.operation === "write";
  const wouldRecordJournal = journalRequired && confirmationValid;
  const reason: LocalMcpConfirmedWriteJournalGateReason = !journalRequired
    ? "journal_not_required"
    : confirmationValid
      ? "operation_journal_not_enabled"
      : "confirmation_required";
  const blockingGate = wouldRecordJournal
    ? operationJournalReadiness.blocking_gates[0]
    : undefined;

  return {
    runtime_mode: "local_vault",
    transport: envelope.transport,
    tool: envelope.tool,
    operation: operationJournalReadiness.preview.operation,
    confirmation_valid: confirmationValid,
    journal_required: journalRequired,
    journal_enabled: false,
    journal_write_allowed: false,
    would_record_journal: wouldRecordJournal,
    record_written: false,
    executed: false,
    reason,
    error_code: reason === "journal_not_required" ? "transport_disabled" : "transport_disabled",
    operation_journal_readiness: operationJournalReadiness,
    guard,
    blocking_gate: blockingGate,
  };
}

export async function preflightLocalMcpTransportRequestWithJournal(
  envelope: LocalMcpTransportRequestEnvelope,
  timestamp = new Date().toISOString(),
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportJournaledPreflight> {
  const request = envelope.request ?? {};
  const preflight = preflightLocalMcpTransportRequest(
    envelope.transport,
    envelope.tool,
    request,
  );
  const dispatchGate = await previewLocalMcpTransportDispatchGate(envelope, timestamp, options);
  const operationJournalGatePreview = previewLocalMcpConfirmedWriteJournalGate(envelope);
  const operationJournalRecord = operationJournalGatePreview.would_record_journal
    ? await appendLocalMcpOperationJournalRecord({
        request_id: envelope.id,
        timestamp,
        transport: envelope.transport,
        tool: envelope.tool,
        target: localMcpOperationJournalTarget(request.input),
        confirmation_text: request.confirmation_text,
        capability_token: request.capability_token,
      })
    : {
        record_written: false,
        reason: "journal_not_required" as const,
        records: listLocalMcpOperationJournalRecords(),
      };

  return {
    preflight,
    dispatch_gate: dispatchGate,
    operation_journal_gate_preview: operationJournalGatePreview,
    operation_journal_record: operationJournalRecord,
    confirmed_write_closeout: localMcpConfirmedWriteJournalCloseout(
      envelope,
      operationJournalGatePreview,
      operationJournalRecord,
      dispatchGate,
    ),
  };
}

function localMcpConfirmedWriteJournalCloseout(
  envelope: LocalMcpTransportRequestEnvelope,
  gatePreview: LocalMcpConfirmedWriteJournalGatePreview,
  recordResult: LocalMcpOperationJournalRecordWriteResult,
  dispatchGate: LocalMcpTransportDispatchGatePreview,
): LocalMcpConfirmedWriteJournalCloseout {
  const replayPreview = recordResult.operation_journal_replay_preview;
  const replayDetected = Boolean(replayPreview?.replay_detected);
  const recordAvailable = Boolean(recordResult.record);
  const closeoutStatus: LocalMcpConfirmedWriteJournalCloseoutStatus = recordResult.record_written
    ? "journal_recorded"
    : replayDetected
      ? "journal_replay_detected"
      : gatePreview.would_record_journal
        ? "journal_blocked"
        : "journal_skipped";
  const reason = recordResult.record_written
    ? "write_execution_disabled"
    : replayDetected
      ? "journal_replay_detected"
      : gatePreview.would_record_journal
        ? recordResult.reason ?? gatePreview.reason
        : gatePreview.reason;

  return {
    runtime_mode: "local_vault",
    transport: envelope.transport,
    tool: envelope.tool,
    journal_required: gatePreview.journal_required,
    confirmation_valid: gatePreview.confirmation_valid,
    journal_recorded: recordResult.record_written,
    replay_detected: replayDetected,
    record_available: recordAvailable,
    execution_allowed: false,
    executed: false,
    closeout_status: closeoutStatus,
    reason,
    error_code:
      reason === "request_id_required" || reason === "timestamp_required"
        ? "invalid_request"
        : "write_execution_disabled",
    dispatch_blocking_gate: dispatchGate.blocking_gates[0],
    idempotency_key: replayPreview?.idempotency_key,
    record_id: recordResult.record?.id,
  };
}

export function localMcpPreflightErrorCode(
  capabilityToken: LocalMcpCapabilityTokenValidation,
): LocalMcpErrorCode {
  if (capabilityToken.reason === "capability_token_missing") {
    return "capability_token_required";
  }
  if (capabilityToken.reason === "capability_token_not_enabled") {
    return "transport_disabled";
  }

  return "transport_disabled";
}

export function validateLocalMcpTransportRequestEnvelope(
  envelope: LocalMcpTransportRequestEnvelope,
): LocalMcpTransportEnvelopeValidation {
  if (!envelope.id.trim()) {
    return { valid: false, reason: "request_id_required", error_code: "invalid_request" };
  }
  if (!isLocalMcpTransportKind(envelope.transport)) {
    return { valid: false, reason: "transport_unsupported", error_code: "invalid_request" };
  }
  if (!envelope.tool.trim()) {
    return { valid: false, reason: "tool_required", error_code: "invalid_request" };
  }

  return { valid: true };
}

export function handleLocalMcpTransportRequestEnvelope(
  envelope: LocalMcpTransportRequestEnvelope,
): LocalMcpTransportResponseEnvelope {
  const validation = validateLocalMcpTransportRequestEnvelope(envelope);
  if (!validation.valid) {
    return {
      id: envelope.id,
      transport: isLocalMcpTransportKind(envelope.transport) ? envelope.transport : undefined,
      runtime_mode: "local_vault",
      executed: false,
      reason: "request_invalid",
      error_code: "invalid_request",
      validation,
      request_trace: previewLocalMcpTransportRejectedRequestTrace(envelope, validation),
    };
  }

  const preflight = preflightLocalMcpTransportRequest(
    envelope.transport,
    envelope.tool,
    envelope.request,
  );

  return {
    id: envelope.id,
    transport: envelope.transport,
    runtime_mode: "local_vault",
    executed: false,
    reason: preflight.reason,
    error_code: preflight.error_code,
    preflight,
    request_trace: previewLocalMcpTransportRequestTrace(envelope, preflight),
  };
}

export async function handleLocalMcpTransportRequestEnvelopeWithAuthPreview(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
  options: LocalMcpTransportDispatchGateOptions = {},
): Promise<LocalMcpTransportAuthPreviewResponseEnvelope> {
  const validation = validateLocalMcpTransportRequestEnvelope(envelope);
  if (!validation.valid) {
    return {
      id: envelope.id,
      transport: isLocalMcpTransportKind(envelope.transport) ? envelope.transport : undefined,
      runtime_mode: "local_vault",
      executed: false,
      reason: "request_invalid",
      error_code: "invalid_request",
      validation,
      request_trace: previewLocalMcpTransportRejectedRequestTrace(envelope, validation),
    };
  }

  const authPreviewPreflight = await preflightLocalMcpTransportRequestWithAuthPreview(
    envelope.transport,
    envelope.tool,
    envelope.request,
    checkedAt,
  );
  const journalGatePreview = previewLocalMcpConfirmedWriteJournalGate(envelope);
  const journaledPreflight = journalGatePreview.would_record_journal
    ? await preflightLocalMcpTransportRequestWithJournal(envelope, checkedAt, options)
    : undefined;
  const transportDispatchGatePreview =
    journaledPreflight?.dispatch_gate ??
    (await previewLocalMcpTransportDispatchGate(envelope, checkedAt, options));
  const requestTrace = previewLocalMcpTransportRequestTrace(
    envelope,
    authPreviewPreflight.preflight,
  );

  if (transportDispatchGatePreview.dispatch_allowed && isLocalMcpReadOnlyTool(envelope.tool)) {
    return {
      id: envelope.id,
      transport: envelope.transport,
      runtime_mode: "local_vault",
      executed: true,
      reason: "dispatch_allowed",
      preflight: authPreviewPreflight.preflight,
      capability_token_validation_preview:
        authPreviewPreflight.capability_token_validation_preview,
      capability_token_auth_gate_preview:
        authPreviewPreflight.capability_token_auth_gate_preview,
      capability_token_acceptance_gate_preview:
        authPreviewPreflight.capability_token_acceptance_gate_preview,
      transport_dispatch_gate_preview: transportDispatchGatePreview,
      request_trace: localMcpTransportExecutedRequestTrace(envelope),
      result: await executeLocalMcpReadOnlyTool(envelope.tool, envelope.request),
    };
  }

  return {
    id: envelope.id,
    transport: envelope.transport,
    runtime_mode: "local_vault",
    executed: false,
    reason: authPreviewPreflight.preflight.reason,
    error_code: authPreviewPreflight.preflight.error_code,
    preflight: authPreviewPreflight.preflight,
    capability_token_validation_preview:
      authPreviewPreflight.capability_token_validation_preview,
    capability_token_auth_gate_preview:
      authPreviewPreflight.capability_token_auth_gate_preview,
    capability_token_acceptance_gate_preview:
      authPreviewPreflight.capability_token_acceptance_gate_preview,
    transport_dispatch_gate_preview: transportDispatchGatePreview,
    journaled_preflight: journaledPreflight,
    request_trace: requestTrace,
  };
}

export async function handleLocalMcpEnabledTransportRequestEnvelope(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
): Promise<LocalMcpTransportAuthPreviewResponseEnvelope> {
  return handleLocalMcpTransportRequestEnvelopeWithAuthPreview(envelope, checkedAt, {
    transport_enabled: true,
  });
}

export function previewLocalMcpTransportRejectedRequestTrace(
  envelope: LocalMcpTransportRequestEnvelope,
  validation: LocalMcpTransportEnvelopeValidation,
): LocalMcpTransportRejectedRequestTrace {
  return {
    schema_version: 1,
    id: envelope.id,
    transport: isLocalMcpTransportKind(envelope.transport) ? envelope.transport : undefined,
    tool: envelope.tool,
    runtime_mode: "local_vault",
    stage: "envelope_validation",
    status: "request_invalid",
    blocking_stage: "envelope_validation",
    blocking_priority: "primary",
    executed: false,
    persisted: false,
    journaled: false,
    storage: "response_only",
    retention: "none",
    raw_values_stored: false,
    value_policy: "presence_only",
    ...localMcpTransportTraceRequestShape(envelope),
    reason: "request_invalid",
    validation_reason: validation.reason,
    error_code: validation.error_code ?? "invalid_request",
  };
}

export function previewLocalMcpTransportRequestTrace(
  envelope: LocalMcpTransportRequestEnvelope,
  preflight: LocalMcpTransportPreflight,
): LocalMcpTransportRequestTrace {
  return {
    schema_version: 1,
    id: envelope.id,
    transport: envelope.transport,
    tool: envelope.tool,
    runtime_mode: "local_vault",
    stage: "transport_preflight",
    status: "preflight_only",
    blocking_stage: localMcpTransportRequestBlockingStage(preflight),
    blocking_priority: "primary",
    ...(preflight.tool_error_code ? { secondary_blocking_stage: "tool_guard" as const } : {}),
    executed: false,
    persisted: false,
    journaled: false,
    storage: "response_only",
    retention: "none",
    raw_values_stored: false,
    value_policy: "presence_only",
    ...localMcpTransportTraceRequestShape(envelope),
    reason: preflight.reason,
    error_code: preflight.error_code,
    ...(preflight.tool_error_code ? { tool_error_code: preflight.tool_error_code } : {}),
  };
}

function localMcpTransportExecutedRequestTrace(
  envelope: LocalMcpTransportRequestEnvelope,
): LocalMcpTransportRequestTrace {
  return {
    schema_version: 1,
    id: envelope.id,
    transport: envelope.transport,
    tool: envelope.tool,
    runtime_mode: "local_vault",
    stage: "transport_dispatch",
    status: "dispatch_executed",
    blocking_stage: null,
    blocking_priority: null,
    executed: true,
    persisted: false,
    journaled: false,
    storage: "response_only",
    retention: "none",
    raw_values_stored: false,
    value_policy: "presence_only",
    ...localMcpTransportTraceRequestShape(envelope),
    reason: "dispatch_allowed",
  };
}

function localMcpTransportRequestBlockingStage(
  preflight: LocalMcpTransportPreflight,
): LocalMcpTransportRequestBlockingStage {
  if (
    preflight.capability_token.reason === "capability_token_missing" ||
    preflight.capability_token.reason === "capability_token_not_enabled"
  ) {
    return "capability_token";
  }

  if (preflight.tool_error_code) {
    return "tool_guard";
  }

  return "transport";
}

function localMcpTransportTraceRequestShape(envelope: LocalMcpTransportRequestEnvelope): {
  input_present: boolean;
  confirmation_text_present: boolean;
  capability_token_present: boolean;
  redacted_fields: LocalMcpTransportRequestTraceRedactedField[];
} {
  const redactedFields: LocalMcpTransportRequestTraceRedactedField[] = [];
  if (envelope.request && "input" in envelope.request) {
    redactedFields.push("input");
  }
  if (envelope.request?.confirmation_text) {
    redactedFields.push("confirmation_text");
  }
  if (envelope.request?.capability_token) {
    redactedFields.push("capability_token");
  }

  return {
    input_present: envelope.request ? "input" in envelope.request : false,
    confirmation_text_present: Boolean(envelope.request?.confirmation_text),
    capability_token_present: Boolean(envelope.request?.capability_token),
    redacted_fields: redactedFields,
  };
}

export function localMcpGuardErrorCode(guard: LocalMcpToolExecutionGuard): LocalMcpErrorCode | undefined {
  switch (guard.rejection_reason) {
    case "write_execution_disabled":
      return "write_execution_disabled";
    case "not_implemented":
      return "tool_not_implemented";
    case "unknown_tool":
      return "tool_unknown";
    default:
      return undefined;
  }
}

export function isLocalMcpReadOnlyTool(name: string): name is LocalMcpReadOnlyToolName {
  return LOCAL_MCP_READ_ONLY_TOOLS.has(name as LocalMcpReadOnlyToolName);
}

export async function executeLocalMcpTool(
  tool: string,
  request: LocalMcpToolExecutionRequest = {},
): Promise<LocalMcpToolExecutionResponse> {
  const guard = guardLocalMcpToolExecution(tool, request);
  if (guard.rejection_reason === "write_execution_disabled") {
    throw new Error(`MCP local tool requires write/confirmation support and is not enabled: ${tool}`);
  }
  if (guard.rejection_reason === "not_implemented") {
    throw new Error(`MCP local tool is cataloged but not implemented locally: ${tool}`);
  }
  if (guard.rejection_reason === "unknown_tool" || !isLocalMcpReadOnlyTool(tool)) {
    throw new Error(`MCP local tool is unknown: ${tool}`);
  }

  return executeLocalMcpReadOnlyTool(tool, request);
}

/**
 * Host-only write boundary for the first enabled local MCP mutation. Transport
 * listeners remain read-only until their embedding host explicitly opts into
 * this function. The durable journal is written before the local vault change
 * so a repeated request ID cannot create a second document.
 */
export async function executeConfirmedLocalMcpDocumentCreate(
  envelope: LocalMcpTransportRequestEnvelope,
  checkedAt: string,
): Promise<{ tool: "starlane.documents.create"; read_only: false; runtime_mode: LocalMcpRuntimeMode; result: { id: string } }> {
  if (envelope.tool !== "starlane.documents.create") {
    throw new Error("MCP confirmed write executor only supports starlane.documents.create");
  }
  const validation = validateLocalMcpTransportRequestEnvelope(envelope);
  if (!validation.valid) throw new Error("MCP local write request is invalid");

  const acceptance = await previewLocalMcpCapabilityTokenAcceptanceGate(
    envelope.request?.capability_token,
    checkedAt,
  );
  if (!acceptance.acceptance_allowed) {
    throw new Error("MCP local write requires an accepted capability token");
  }
  const confirmation = validateLocalMcpConfirmation(
    envelope.tool,
    envelope.request?.confirmation_text,
  );
  if (!confirmation.valid) throw new Error("MCP local write requires exact confirmation");

  const journal = await appendLocalMcpOperationJournalRecord({
    request_id: envelope.id,
    timestamp: checkedAt,
    transport: envelope.transport,
    tool: envelope.tool,
    target: localMcpOperationJournalTarget(envelope.request?.input),
    confirmation_text: envelope.request?.confirmation_text,
    capability_token: envelope.request?.capability_token,
  });
  if (!journal.record_written) {
    throw new Error(
      journal.reason === "journal_replay_detected"
        ? "MCP local write request was already processed"
        : "MCP local write journal could not be recorded",
    );
  }

  const input = envelope.request?.input;
  const created = await dataClient.createDocument(readRequiredString(input, "space_id"), {
    title: readRequiredString(input, "title"),
    content: readOptionalString(input, "content") ?? "",
    folder_id: readOptionalString(input, "folder_id") ?? null,
  });
  return {
    tool: "starlane.documents.create",
    read_only: false,
    runtime_mode: "local_vault",
    result: created,
  };
}

export async function executeLocalMcpReadOnlyTool(
  tool: LocalMcpReadOnlyToolName,
  request: LocalMcpToolExecutionRequest = {},
): Promise<LocalMcpToolExecutionResponse> {
  switch (tool) {
    case "starlane.spaces.list":
      return localMcpResponse(tool, await dataClient.listSpaces());
    case "starlane.spaces.get":
      return localMcpResponse(tool, await dataClient.getSpace(readRequiredString(request.input, "space_id")));
    case "starlane.folders.list":
      return localMcpResponse(tool, await dataClient.listFolders(readRequiredString(request.input, "space_id")));
    case "starlane.documents.get":
      return localMcpResponse(tool, await readLocalMcpDocument(request.input));
    case "starlane.search.query":
      return localMcpResponse(tool, await searchLocalMcpDocuments(request.input));
    case "starlane.graph.get":
      return localMcpResponse(tool, await dataClient.getGraph(readRequiredString(request.input, "space_id")));
    case "starlane.tags.list":
      return localMcpResponse(tool, await dataClient.listTags(readRequiredString(request.input, "space_id")));
    case "starlane.properties.list":
      return localMcpResponse(
        tool,
        await dataClient.listPropertySchemas(readRequiredString(request.input, "space_id")),
      );
    case "starlane.templates.list":
      return localMcpResponse(tool, await dataClient.listTemplates(readRequiredString(request.input, "space_id")));
  }
}

function localMcpResponse(
  tool: LocalMcpReadOnlyToolName,
  result: LocalMcpToolResult,
): LocalMcpToolExecutionResponse {
  return {
    tool,
    read_only: true,
    runtime_mode: "local_vault",
    result,
  };
}

type LocalMcpToolResult =
  | DocumentRecord
  | FolderRecord[]
  | GraphRecord
  | PropertySchemaRecord[]
  | SearchResultRecord[]
  | SpaceRecord
  | SpaceRecord[]
  | TagRecord[]
  | TemplateRecord[]
  | (DocumentRecord & { content: string });

async function readLocalMcpDocument(input: unknown): Promise<DocumentRecord & { content: string }> {
  const spaceId = readRequiredString(input, "space_id");
  const documentId = readRequiredString(input, "document_id");
  const [metadata, content] = await Promise.all([
    dataClient.getDocument(spaceId, documentId),
    dataClient.getDocumentContent(spaceId, documentId),
  ]);

  return {
    ...metadata,
    content: content.content,
  };
}

async function searchLocalMcpDocuments(input: unknown): Promise<SearchResultRecord[]> {
  const q = readRequiredString(input, "q");
  const limit = readOptionalLimit(input, 25);
  const spaceId = readOptionalString(input, "space_id");

  return dataClient.searchDocuments(q, limit, spaceId ? { space_id: spaceId } : {});
}

function readRequiredString(input: unknown, key: string): string {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new Error(`MCP local tool input must include ${key}`);
  }

  const value = (input as Record<string, unknown>)[key];
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`MCP local tool input must include ${key}`);
  }

  return value;
}

function readOptionalString(input: unknown, key: string): string | undefined {
  if (!input || typeof input !== "object" || Array.isArray(input)) return undefined;

  const value = (input as Record<string, unknown>)[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`MCP local tool input ${key} must be a non-empty string`);
  }

  return value;
}

function readOptionalLimit(input: unknown, fallback: number): number {
  if (!input || typeof input !== "object" || Array.isArray(input)) return fallback;

  const value = (input as Record<string, unknown>).limit;
  if (value === undefined || value === null) return fallback;
  if (typeof value !== "number" || !Number.isInteger(value) || value < 1 || value > 100) {
    throw new Error("MCP local search limit must be an integer from 1 to 100");
  }

  return value;
}
