import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { __resetLocalDataForTests, dataClient } from "./dataClient";
import {
  acceptLocalMcpCapabilityTokenRecord,
  appendLocalMcpOperationJournalRecord,
  buildLocalMcpCapabilityTokenRequestChallenge,
  buildLocalMcpConfirmationChallenge,
  clearLocalMcpCapabilityTokenRecords,
  clearLocalMcpOperationJournalRecords,
  executeLocalMcpTool,
  executeConfirmedLocalMcpDocumentCreate,
  executeLocalMcpReadOnlyTool,
  findLocalMcpCapabilityTokenRecord,
  guardLocalMcpToolExecution,
  handleLocalMcpEnabledTransportRequestEnvelope,
  handleLocalMcpTransportRequestEnvelope,
  handleLocalMcpTransportRequestEnvelopeWithAuthPreview,
  handleLocalMcpTransportHandlerSkeleton,
  issueLocalMcpCapabilityToken,
  isLocalMcpReadOnlyTool,
  listLocalMcpBlockedTools,
  listLocalMcpCapabilityTokenRecords,
  listLocalMcpReadOnlyTools,
  listLocalMcpOperationJournalRecords,
  listLocalMcpTransportStatuses,
  localMcpGuardErrorCode,
  localMcpCapabilityTokenAuthContract,
  localMcpCapabilityTokenAuthReadiness,
  localMcpCapabilityTokenLifecyclePolicy,
  localMcpCapabilityTokenPolicy,
  localMcpPreflightErrorCode,
  localMcpOperationJournalContract,
  localMcpOperationJournalPolicy,
  localMcpOperationJournalRecordContract,
  localMcpOperationJournalReadiness,
  localMcpRuntimeManifest,
  localMcpTransportAdapterContract,
  localMcpTransportCapabilitySummary,
  localMcpTransportDispatchContract,
  localMcpTransportEnvelopeContract,
  localMcpTransportErrorContract,
  localMcpTransportExecutionContract,
  localMcpTransportIdempotencyContract,
  localMcpTransportLifecycleContract,
  localMcpTransportListenerConfigContract,
  localMcpTransportResultContract,
  localMcpTransportRetryContract,
  localMcpTransportRequestTraceContract,
  localMcpTransportRequestTracePolicy,
  localMcpTransportReadiness,
  localMcpTransportSideEffectContract,
  localMcpWriteBlockContract,
  localMcpWriteExecutionReadiness,
  planLocalMcpToolExecution,
  persistLocalMcpCapabilityTokenRecord,
  preflightLocalMcpTransportRequest,
  preflightLocalMcpTransportRequestWithAuthPreview,
  preflightLocalMcpTransportRequestWithJournal,
  previewLocalMcpTransportAdapterRequest,
  previewLocalMcpTransportAdapterResponse,
  previewLocalMcpTransportAdapterLifecycle,
  previewLocalMcpTransportDispatchGate,
  previewLocalMcpTransportRejectedRequestTrace,
  previewLocalMcpTransportRequestTrace,
  previewLocalMcpCapabilityTokenAuthGate,
  previewLocalMcpCapabilityTokenAcceptanceGate,
  previewLocalMcpCapabilityTokenAcceptanceReadiness,
  previewLocalMcpCapabilityTokenExpiry,
  previewLocalMcpCapabilityTokenIssuance,
  previewLocalMcpCapabilityTokenRecordExpiry,
  previewLocalMcpCapabilityTokenRecordRevocation,
  previewLocalMcpCapabilityTokenValidation,
  previewLocalMcpCapabilityTokenRevocation,
  previewLocalMcpConfirmedWriteJournalGate,
  previewLocalMcpOperationJournal,
  previewLocalMcpOperationJournalReplay,
  previewLocalMcpTransportAdapterBindPreflight,
  previewLocalMcpTransportAdapterBindPreflightWithAcceptance,
  previewLocalMcpTransportAdapterRuntimeState,
  previewLocalMcpTransportErrorEnvelopeFrame,
  previewLocalMcpTransportErrorEmission,
  previewLocalMcpTransportHandlerInvocation,
  previewLocalMcpTransportHandlerResponseFrame,
  previewLocalMcpTransportHandlerTerminalState,
  previewLocalMcpTransportEnablementReadiness,
  previewLocalMcpTransportLifecycleAudit,
  previewLocalMcpTransportRequestCloseout,
  previewLocalMcpTransportResponseCompletion,
  previewLocalMcpTransportResponseEmission,
  previewLocalMcpTransportTerminalCleanup,
  previewLocalMcpTransportRequestLoopDispatch,
  previewLocalMcpTransportRequestLoopRegistration,
  previewLocalMcpTransportHandlerSkeleton,
  previewLocalMcpTransportAdapterShutdown,
  previewLocalMcpTransportAdapterStartup,
  previewLocalMcpTransportAdapterStartupWithAcceptance,
  previewLocalMcpTransportListenerConfig,
  previewLocalMcpTransportListenerConfigValidation,
  revokeLocalMcpCapabilityTokenRecord,
  validateLocalMcpCapabilityToken,
  validateLocalMcpCapabilityTokenExpiry,
  validateLocalMcpCapabilityTokenRequest,
  validateLocalMcpCapabilityTokenRevocationRequest,
  validateLocalMcpConfirmation,
  validateLocalMcpTransportRequestEnvelope,
  validateLocalMcpTransportListenerConfig,
} from "./mcpLocalRuntime";
import { setCurrentLaunchMode } from "./launchMode";
import { setSyncProvider } from "./storageProviders";

describe("mcpLocalRuntime", () => {
  const storage = new Map<string, string>();

  beforeEach(async () => {
    storage.clear();
    const localStorageMock = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
      removeItem: (key: string) => {
        storage.delete(key);
      },
    };
    (globalThis as { window?: unknown }).window = {
      localStorage: localStorageMock,
    };
    (globalThis as { localStorage?: unknown }).localStorage = localStorageMock;
    setCurrentLaunchMode("local");
    setSyncProvider("none");
    await __resetLocalDataForTests();
  });

  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
    delete (globalThis as { localStorage?: unknown }).localStorage;
  });

  it("identifies the first local-vault read-only tools", () => {
    expect(isLocalMcpReadOnlyTool("starlane.spaces.list")).toBe(true);
    expect(isLocalMcpReadOnlyTool("starlane.spaces.get")).toBe(true);
    expect(isLocalMcpReadOnlyTool("starlane.folders.list")).toBe(true);
    expect(isLocalMcpReadOnlyTool("starlane.documents.get")).toBe(true);
    expect(isLocalMcpReadOnlyTool("starlane.search.query")).toBe(true);
    expect(isLocalMcpReadOnlyTool("starlane.graph.get")).toBe(true);
    expect(isLocalMcpReadOnlyTool("starlane.tags.list")).toBe(true);
    expect(isLocalMcpReadOnlyTool("starlane.properties.list")).toBe(true);
    expect(isLocalMcpReadOnlyTool("starlane.templates.list")).toBe(true);
    expect(isLocalMcpReadOnlyTool("starlane.documents.create")).toBe(false);
    expect(isLocalMcpReadOnlyTool("starlane.unknown")).toBe(false);
  });

  it("exposes local-vault helper discovery metadata", () => {
    expect(listLocalMcpReadOnlyTools()).toEqual([
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
    expect(localMcpRuntimeManifest()).toEqual({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport_status: "helper_only",
      read_only: true,
      tool_count: 9,
      tools: listLocalMcpReadOnlyTools().map((name) => ({
        name,
        read_only: true,
        runtime_modes: ["local_vault"],
      })),
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
    });
    expect(listLocalMcpBlockedTools()).toEqual(
      expect.arrayContaining([
        { name: "starlane.documents.create", reason: "write_not_enabled" },
        { name: "starlane.import.execute", reason: "write_not_enabled" },
        { name: "starlane.import.preview", reason: "not_implemented" },
      ]),
    );
    expect(localMcpWriteBlockContract()).toEqual({
      runtime_mode: "local_vault",
      write_execution_enabled: false,
      confirmation_required: true,
      operation_journal_required: true,
      blocked_write_tools: [
        "starlane.documents.create",
        "starlane.documents.update",
        "starlane.documents.delete",
        "starlane.workflows.record_event",
        "starlane.export.snapshot",
        "starlane.import.execute",
      ],
      not_implemented_tools: [
        "starlane.import.preview",
        "starlane.admin.users.list",
        "starlane.admin.audit.export",
      ],
      reason: "write_not_enabled",
      error_code: "write_execution_disabled",
    });
  });

  it("exposes planned local transports without enabling them", () => {
    expect(listLocalMcpTransportStatuses()).toEqual([
      {
        transport: "stdio",
        runtime_mode: "local_vault",
        available: false,
        status: "planned",
        reason: "transport_not_enabled",
        guard_enforced: true,
        executable_tools: listLocalMcpReadOnlyTools(),
      },
      {
        transport: "sse",
        runtime_mode: "local_vault",
        available: false,
        status: "planned",
        reason: "transport_not_enabled",
        guard_enforced: true,
        executable_tools: listLocalMcpReadOnlyTools(),
      },
    ]);
  });

  it("summarizes local transport capabilities and disabled gates", () => {
    expect(localMcpTransportEnvelopeContract()).toEqual({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport_status: "helper_only",
      transports: ["stdio", "sse"],
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
    });

    expect(localMcpTransportDispatchContract()).toEqual({
      runtime_mode: "local_vault",
      transport_status: "helper_only",
      transports: ["stdio", "sse"],
      helper_dispatch_enabled: true,
      transport_dispatch_enabled: false,
      preflight_required: true,
      envelope_required: true,
      guard_required: true,
      auth_required_for_transports: true,
      operation_journal_required_for_writes: true,
      read_tools_dispatched: 9,
      write_tools_dispatched: 0,
      blocked_write_tools: 6,
      not_implemented_tools: 3,
      disabled_error_code: "transport_disabled",
      write_error_code: "write_execution_disabled",
    });

    expect(localMcpTransportErrorContract()).toEqual({
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
    });

    expect(localMcpTransportResultContract()).toEqual({
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
      successful_helper_tools: 9,
      blocked_write_tools: 6,
      not_implemented_tools: 3,
    });

    expect(localMcpTransportRetryContract()).toEqual({
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
    });

    expect(localMcpTransportIdempotencyContract()).toEqual({
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
    });

    expect(localMcpTransportSideEffectContract()).toEqual({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport_status: "helper_only",
      transport_side_effects_enabled: false,
      helper_read_side_effects_enabled: false,
      write_side_effects_enabled: false,
      persistence_enabled: false,
      journal_writes_enabled: false,
      replay_cache_writes_enabled: false,
      boundaries: [
        "envelope_validation",
        "transport_preflight",
        "helper_read_dispatch",
        "write_execution",
        "operation_journal",
        "replay_cache",
      ],
      non_mutating_boundaries: [
        "envelope_validation",
        "transport_preflight",
        "helper_read_dispatch",
        "write_execution",
        "operation_journal",
        "replay_cache",
      ],
      mutating_boundaries: [],
    });

    expect(localMcpTransportAdapterContract()).toEqual({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport_status: "helper_only",
      adapters_enabled: false,
      adapters: [
        {
          transport: "stdio",
          runtime_mode: "local_vault",
          adapter_available: false,
          bindable: false,
          accepts_requests: false,
          emits_responses: false,
          requires_capability_token: true,
          requires_operation_journal: true,
          reason: "transport_not_enabled",
        },
        {
          transport: "sse",
          runtime_mode: "local_vault",
          adapter_available: false,
          bindable: false,
          accepts_requests: false,
          emits_responses: false,
          requires_capability_token: true,
          requires_operation_journal: true,
          reason: "transport_not_enabled",
        },
      ],
    });

    expect(localMcpTransportLifecycleContract()).toEqual({
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
    });

    expect(localMcpTransportExecutionContract()).toEqual({
      runtime_mode: "local_vault",
      transport_status: "helper_only",
      transports: ["stdio", "sse"],
      transport_available: false,
      guard_enforced: true,
      auth_required: true,
      operation_journal_required_for_writes: true,
      request_trace_available: true,
      executable_read_tools: 9,
      blocked_write_tools: 6,
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
    });

    expect(localMcpTransportCapabilitySummary()).toEqual({
      runtime_mode: "local_vault",
      transport_status: "helper_only",
      transports: ["stdio", "sse"],
      executable_read_tools: 9,
      blocked_write_tools: 6,
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
    });
  });

  it("previews disabled local MCP transport adapter requests without binding or responding", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpTransportAdapterRequest(
        {
          id: "adapter-preview-1",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      adapter_available: false,
      bindable: false,
      request_accepted: false,
      response_emitted: false,
      executed: false,
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      dispatch_gate: {
        dispatch_allowed: false,
        executed: false,
        transport: "stdio",
        tool: "starlane.spaces.list",
        envelope_validation: {
          valid: true,
        },
        auth_gate: {
          token_checks_passed: true,
          reason: "capability_token_ready",
        },
        acceptance_gate: {
          token_checks_passed: true,
          acceptance_allowed: false,
          reason: "capability_token_acceptance_not_enabled",
        },
        blocking_gates: [
          {
            gate: "token_acceptance",
            status: "disabled",
            reason: "capability_token_acceptance_not_enabled",
            error_code: "capability_token_not_enabled",
          },
          {
            gate: "transport",
            status: "disabled",
            reason: "transport_not_enabled",
            error_code: "transport_disabled",
          },
        ],
      },
    });

    await expect(
      previewLocalMcpTransportAdapterRequest(
        {
          id: "adapter-preview-invalid",
          transport: "smtp" as "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      adapter_available: false,
      bindable: false,
      request_accepted: false,
      response_emitted: false,
      executed: false,
      dispatch_gate: {
        dispatch_allowed: false,
        executed: false,
        envelope_validation: {
          valid: false,
          reason: "transport_unsupported",
        },
        blocking_gates: [
          {
            gate: "envelope",
            status: "blocked",
            reason: "transport_unsupported",
            error_code: "invalid_request",
          },
        ],
      },
    });
  });

  it("previews disabled local MCP transport adapter bind preflight without starting listeners", () => {
    expect(previewLocalMcpTransportAdapterBindPreflight("stdio")).toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
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
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      transport_readiness: {
        transport: "stdio",
        transport_available: false,
        status: "disabled",
      },
      capability_token_auth_readiness: {
        status: "disabled",
        token_accepted: false,
        transport_ready: false,
      },
      operation_journal_readiness: {
        status: "planned",
        journal_enabled: false,
        preview: {
          tool: "starlane.spaces.list",
          required: false,
        },
      },
    });

    expect(
      previewLocalMcpTransportAdapterBindPreflight("sse", "starlane.documents.delete"),
    ).toMatchObject({
      transport: "sse",
      adapter_available: false,
      bound: false,
      listener_started: false,
      request_loop_started: false,
      transport_readiness: {
        transport: "sse",
        tool: "starlane.documents.delete",
        write_execution: {
          write_execution_enabled: false,
        },
      },
      operation_journal_readiness: {
        status: "planned",
        preview: {
          tool: "starlane.documents.delete",
          required: true,
        },
      },
    });
  });

  it("previews local MCP adapter binding blocked by token acceptance", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await expect(
      previewLocalMcpTransportAdapterBindPreflightWithAcceptance(
        "stdio",
        "starlane.spaces.list",
        {},
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      bindable: false,
      bind_attempted: false,
      bound: false,
      listener_started: false,
      request_loop_started: false,
      listener_binding_allowed: false,
      capability_token_acceptance_gate_preview: {
        token_checks_passed: false,
        token_accepted: false,
        acceptance_allowed: false,
        listener_binding_allowed: false,
        reason: "capability_token_missing",
      },
      dispatch_gate: {
        dispatch_allowed: false,
        blocking_gates: [
          {
            gate: "capability_token",
            status: "blocked",
            reason: "capability_token_missing",
            error_code: "capability_token_required",
          },
          {
            gate: "transport",
            status: "disabled",
            reason: "transport_not_enabled",
            error_code: "transport_disabled",
          },
        ],
      },
      bind_blocking_gate: {
        gate: "capability_token",
        status: "blocked",
        reason: "capability_token_missing",
        error_code: "capability_token_required",
      },
    });

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpTransportAdapterBindPreflightWithAcceptance(
        "stdio",
        "starlane.spaces.list",
        {
          capability_token: "local-mcp-raw-token-1",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      bindable: false,
      bind_attempted: false,
      bound: false,
      listener_started: false,
      request_loop_started: false,
      listener_binding_allowed: false,
      capability_token_acceptance_gate_preview: {
        token_checks_passed: true,
        token_accepted: false,
        acceptance_allowed: false,
        listener_binding_allowed: false,
        transport_dispatch_allowed: false,
        reason: "capability_token_acceptance_not_enabled",
      },
      dispatch_gate: {
        dispatch_allowed: false,
        blocking_gates: [
          {
            gate: "token_acceptance",
            status: "disabled",
            reason: "capability_token_acceptance_not_enabled",
            error_code: "capability_token_not_enabled",
          },
          {
            gate: "transport",
            status: "disabled",
            reason: "transport_not_enabled",
            error_code: "transport_disabled",
          },
        ],
      },
      bind_blocking_gate: {
        gate: "token_acceptance",
        status: "disabled",
        reason: "capability_token_acceptance_not_enabled",
        error_code: "capability_token_not_enabled",
      },
    });

    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "bind-accepted-token",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-raw-token-1",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportAdapterBindPreflightWithAcceptance(
        "stdio",
        "starlane.spaces.list",
        {
          capability_token: "local-mcp-raw-token-1",
        },
        "2026-07-13T13:32:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      bindable: false,
      bind_attempted: false,
      bound: false,
      listener_started: false,
      request_loop_started: false,
      listener_binding_allowed: false,
      capability_token_acceptance_gate_preview: {
        token_checks_passed: true,
        token_accepted: true,
        acceptance_allowed: true,
        listener_binding_allowed: false,
        transport_dispatch_allowed: false,
        reason: "capability_token_accepted",
      },
      dispatch_gate: {
        dispatch_allowed: false,
        blocking_gates: [
          {
            gate: "transport",
            status: "disabled",
            reason: "transport_not_enabled",
            error_code: "transport_disabled",
          },
        ],
      },
      bind_blocking_gate: {
        gate: "transport",
        status: "disabled",
        reason: "transport_not_enabled",
        error_code: "transport_disabled",
      },
    });

    await expect(
      previewLocalMcpTransportAdapterBindPreflightWithAcceptance(
        "stdio",
        "starlane.spaces.list",
        {
          capability_token: "local-mcp-raw-token-1",
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      bindable: false,
      bind_attempted: false,
      bound: false,
      listener_started: false,
      request_loop_started: false,
      listener_binding_allowed: false,
      dispatch_gate: {
        dispatch_allowed: true,
        transport_gate_enabled: true,
        blocking_gates: [],
        reason: "dispatch_allowed",
      },
      bind_blocking_gate: {
        gate: "transport",
        reason: "transport_not_enabled",
        error_code: "transport_disabled",
      },
    });
  });

  it("previews disabled local MCP transport listener configuration without binding", () => {
    expect(localMcpTransportListenerConfigContract()).toEqual({
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
    });

    expect(previewLocalMcpTransportListenerConfig("stdio")).toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      listener_configuration_enabled: false,
      listener_configured: false,
      stdio_stream_binding_enabled: false,
      stdio_stdin_required: true,
      stdio_stdout_required: true,
      stdio_stderr_required: true,
      stdio_streams_attached: false,
      sse_endpoint_registration_enabled: false,
      sse_endpoint_required: false,
      sse_method: null,
      sse_headers_required: false,
      sse_endpoint_registered: false,
      endpoint: null,
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      bind_preflight: {
        transport: "stdio",
        bound: false,
      },
    });

    expect(
      previewLocalMcpTransportListenerConfig("sse", "starlane.documents.delete"),
    ).toMatchObject({
      transport: "sse",
      listener_configuration_enabled: false,
      listener_configured: false,
      stdio_stdin_required: false,
      stdio_stdout_required: false,
      stdio_stderr_required: false,
      stdio_streams_attached: false,
      sse_endpoint_registration_enabled: false,
      sse_endpoint_required: true,
      sse_method: "GET",
      sse_headers_required: true,
      sse_endpoint_registered: false,
      endpoint: null,
      bind_preflight: {
        transport: "sse",
        operation_journal_readiness: {
          preview: {
            tool: "starlane.documents.delete",
            required: true,
          },
        },
      },
    });
  });

  it("validates local MCP transport listener configuration before disabled binding", async () => {
    expect(
      validateLocalMcpTransportListenerConfig({
        transport: "smtp",
      }),
    ).toEqual({
      valid: false,
      reason: "transport_unsupported",
      error_code: "invalid_request",
      listener_configuration_enabled: false,
    });

    expect(
      validateLocalMcpTransportListenerConfig({
        transport: "stdio",
        stdio: {
          stdin: {},
          stdout: {},
        },
      }),
    ).toEqual({
      valid: false,
      transport: "stdio",
      reason: "stdio_streams_required",
      error_code: "invalid_request",
      listener_configuration_enabled: false,
    });

    expect(
      validateLocalMcpTransportListenerConfig({
        transport: "sse",
        sse: {
          endpoint: "",
        },
      }),
    ).toEqual({
      valid: false,
      transport: "sse",
      reason: "sse_endpoint_required",
      error_code: "invalid_request",
      listener_configuration_enabled: false,
    });

    expect(
      validateLocalMcpTransportListenerConfig({
        transport: "sse",
        sse: {
          endpoint: "/mcp/events",
          method: "POST",
        },
      }),
    ).toEqual({
      valid: false,
      transport: "sse",
      reason: "sse_method_invalid",
      error_code: "invalid_request",
      listener_configuration_enabled: false,
    });

    const request = {
      request_id: "listener-config-token",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };
    await issueLocalMcpCapabilityToken(request, "listener-config-token-raw");
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "listener-config-accept-token",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "listener-config-token-raw",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportListenerConfigValidation(
        {
          transport: "sse",
          tool: "starlane.spaces.list",
          capability_token: "listener-config-token-raw",
          sse: {
            endpoint: "/mcp/events",
            method: "GET",
            headers: {
              accept: "text/event-stream",
            },
          },
        },
        "2026-07-13T13:32:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "sse",
      listener_configuration_enabled: false,
      listener_configured: false,
      reason: "listener_configuration_not_enabled",
      error_code: "transport_disabled",
      validation: {
        valid: false,
        transport: "sse",
        reason: "listener_configuration_not_enabled",
        error_code: "transport_disabled",
      },
      config_preview: {
        transport: "sse",
        sse_endpoint_required: true,
        sse_method: "GET",
        endpoint: null,
        bind_preflight: {
          operation_journal_readiness: {
            preview: {
              tool: "starlane.spaces.list",
              required: false,
            },
          },
        },
      },
      bind_preflight: {
        transport: "sse",
        listener_binding_allowed: false,
        capability_token_acceptance_gate_preview: {
          token_checks_passed: true,
          token_accepted: true,
          acceptance_allowed: true,
          reason: "capability_token_accepted",
        },
        dispatch_gate: {
          dispatch_allowed: false,
          blocking_gates: [
            {
              gate: "transport",
              status: "disabled",
              reason: "transport_not_enabled",
              error_code: "transport_disabled",
            },
          ],
        },
        bind_blocking_gate: {
          gate: "transport",
          status: "disabled",
          reason: "transport_not_enabled",
          error_code: "transport_disabled",
        },
      },
      listener_bind_descriptor: {
        schema_version: 1,
        runtime_mode: "local_vault",
        transport: "sse",
        listener_bind_descriptor_available: true,
        listener_binding_allowed: false,
        bind_attempted: false,
        listener_started: false,
        request_loop_started: false,
        stdio_streams_attached: false,
        sse_endpoint_registered: false,
        endpoint: "/mcp/events",
        stdio_streams_present: false,
        sse_endpoint_present: true,
        bind_blocking_gate: {
          gate: "transport",
          status: "disabled",
          reason: "transport_not_enabled",
          error_code: "transport_disabled",
        },
        reason: "transport_not_enabled",
        error_code: "transport_disabled",
      },
      startup_preview: {
        transport: "sse",
        startup_attempted: false,
        listener_configured: false,
        listener_started: false,
        request_loop_started: false,
        stdio_streams_attached: false,
        sse_endpoint_registered: false,
        endpoint: null,
        listener_binding_allowed: false,
        listener_bind_descriptor: {
          transport: "sse",
          endpoint: "/mcp/events",
          listener_binding_allowed: false,
          listener_started: false,
          request_loop_started: false,
          reason: "transport_not_enabled",
        },
        bind_preflight: {
          bind_blocking_gate: {
            gate: "transport",
            reason: "transport_not_enabled",
          },
        },
      },
      request_loop_registration: {
        transport: "sse",
        request_loop_registration_enabled: false,
        request_loop_registered: false,
        request_loop_started: false,
        accepts_requests: false,
        emits_responses: false,
        handler_available: false,
        listener_binding_allowed: false,
        startup_preview: {
          listener_bind_descriptor: {
            endpoint: "/mcp/events",
            listener_started: false,
          },
        },
        bind_preflight: {
          bind_blocking_gate: {
            gate: "transport",
            reason: "transport_not_enabled",
          },
        },
        dispatch_gate: {
          dispatch_allowed: false,
          blocking_gates: [
            {
              gate: "transport",
              reason: "transport_not_enabled",
            },
          ],
        },
        first_blocking_stage: "transport",
        reason: "transport_not_enabled",
        error_code: "transport_disabled",
      },
    });

    await expect(
      previewLocalMcpTransportListenerConfigValidation(
        {
          transport: "stdio",
          tool: "starlane.spaces.list",
          capability_token: "listener-config-token-raw",
          stdio: {
            stdin: {},
            stdout: {},
            stderr: {},
          },
        },
        "2026-07-13T13:32:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      listener_configuration_enabled: false,
      listener_configured: false,
      bind_preflight: {
        transport: "stdio",
        listener_binding_allowed: false,
        bind_blocking_gate: {
          gate: "transport",
          reason: "transport_not_enabled",
        },
      },
      listener_bind_descriptor: {
        transport: "stdio",
        listener_bind_descriptor_available: true,
        listener_binding_allowed: false,
        bind_attempted: false,
        listener_started: false,
        request_loop_started: false,
        stdio_streams_attached: false,
        sse_endpoint_registered: false,
        endpoint: null,
        stdio_streams_present: true,
        sse_endpoint_present: false,
        reason: "transport_not_enabled",
        error_code: "transport_disabled",
      },
      startup_preview: {
        transport: "stdio",
        startup_attempted: false,
        listener_configured: false,
        listener_started: false,
        request_loop_started: false,
        listener_binding_allowed: false,
        listener_bind_descriptor: {
          transport: "stdio",
          stdio_streams_present: true,
          listener_started: false,
          reason: "transport_not_enabled",
        },
      },
      request_loop_registration: {
        transport: "stdio",
        request_loop_registration_enabled: false,
        request_loop_registered: false,
        request_loop_started: false,
        accepts_requests: false,
        emits_responses: false,
        handler_available: false,
        listener_binding_allowed: false,
        startup_preview: {
          listener_bind_descriptor: {
            stdio_streams_present: true,
            listener_started: false,
          },
        },
        dispatch_gate: {
          dispatch_allowed: false,
          blocking_gates: [
            {
              gate: "transport",
              reason: "transport_not_enabled",
            },
          ],
        },
        first_blocking_stage: "transport",
        reason: "transport_not_enabled",
        error_code: "transport_disabled",
      },
    });

    await expect(
      previewLocalMcpTransportListenerConfigValidation(
        {
          transport: "stdio",
          tool: "starlane.spaces.list",
          capability_token: "listener-config-token-raw",
          stdio: {
            stdin: {},
            stdout: {},
            stderr: {},
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      listener_configuration_enabled: false,
      listener_configured: false,
      bind_preflight: {
        transport: "stdio",
        listener_binding_allowed: false,
        dispatch_gate: {
          dispatch_allowed: true,
          transport_gate_enabled: true,
          blocking_gates: [],
          reason: "dispatch_allowed",
        },
      },
      listener_bind_descriptor: {
        transport: "stdio",
        listener_binding_allowed: false,
        bind_attempted: false,
        listener_started: false,
        request_loop_started: false,
        stdio_streams_present: true,
      },
      request_loop_registration: {
        request_loop_registration_enabled: false,
        request_loop_registered: false,
        request_loop_started: false,
        dispatch_gate: {
          dispatch_allowed: true,
          transport_gate_enabled: true,
          blocking_gates: [],
          reason: "dispatch_allowed",
        },
        first_blocking_stage: "transport",
        reason: "transport_not_enabled",
      },
    });
  });

  it("previews disabled local MCP transport adapter startup without configuring listeners", () => {
    expect(previewLocalMcpTransportAdapterStartup("stdio")).toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      startup_attempted: false,
      adapter_available: false,
      listener_configured: false,
      listener_started: false,
      request_loop_started: false,
      stdio_streams_attached: false,
      sse_endpoint_registered: false,
      endpoint: null,
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      bind_preflight: {
        transport: "stdio",
        bind_attempted: false,
        bound: false,
        listener_started: false,
        request_loop_started: false,
        transport_readiness: {
          transport: "stdio",
          status: "disabled",
        },
      },
    });

    expect(
      previewLocalMcpTransportAdapterStartup("sse", "starlane.documents.delete"),
    ).toMatchObject({
      transport: "sse",
      startup_attempted: false,
      listener_configured: false,
      listener_started: false,
      request_loop_started: false,
      stdio_streams_attached: false,
      sse_endpoint_registered: false,
      endpoint: null,
      bind_preflight: {
        transport: "sse",
        operation_journal_readiness: {
          preview: {
            tool: "starlane.documents.delete",
            required: true,
          },
        },
        transport_readiness: {
          tool: "starlane.documents.delete",
          write_execution: {
            write_execution_enabled: false,
          },
        },
      },
    });
  });

  it("previews local MCP adapter startup blocked by token acceptance", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpTransportAdapterStartupWithAcceptance(
        "sse",
        "starlane.documents.delete",
        {
          capability_token: "local-mcp-raw-token-1",
          confirmation_text: "confirm local mcp starlane.documents.delete",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      startup_attempted: false,
      adapter_available: false,
      listener_configured: false,
      listener_started: false,
      request_loop_started: false,
      stdio_streams_attached: false,
      sse_endpoint_registered: false,
      endpoint: null,
      listener_binding_allowed: false,
      capability_token_acceptance_gate_preview: {
        token_checks_passed: true,
        acceptance_allowed: false,
        listener_binding_allowed: false,
        reason: "capability_token_acceptance_not_enabled",
      },
      bind_preflight: {
        transport: "sse",
        listener_binding_allowed: false,
        bind_blocking_gate: {
          gate: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        operation_journal_readiness: {
          preview: {
            tool: "starlane.documents.delete",
            required: true,
          },
        },
      },
    });
  });

  it("previews disabled local MCP request-loop registration before accepting requests", async () => {
    await expect(
      previewLocalMcpTransportRequestLoopRegistration(
        "stdio",
        "starlane.spaces.list",
        {},
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      request_loop_registration_enabled: false,
      request_loop_registered: false,
      request_loop_started: false,
      accepts_requests: false,
      emits_responses: false,
      handler_available: false,
      listener_binding_allowed: false,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      startup_preview: {
        startup_attempted: false,
        listener_started: false,
        request_loop_started: false,
      },
      bind_preflight: {
        listener_binding_allowed: false,
        bind_blocking_gate: {
          gate: "capability_token",
          reason: "capability_token_missing",
        },
      },
      dispatch_gate: {
        dispatch_allowed: false,
        auth_gate: {
          token_checks_passed: false,
          reason: "capability_token_missing",
        },
      },
    });

    const request = {
      request_id: "token-request-2",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-2");

    await expect(
      previewLocalMcpTransportRequestLoopRegistration(
        "sse",
        "starlane.documents.delete",
        {
          capability_token: "local-mcp-raw-token-2",
          confirmation_text: "confirm local mcp starlane.documents.delete",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      request_loop_registration_enabled: false,
      request_loop_registered: false,
      request_loop_started: false,
      accepts_requests: false,
      emits_responses: false,
      handler_available: false,
      listener_binding_allowed: false,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      startup_preview: {
        listener_binding_allowed: false,
        listener_started: false,
        sse_endpoint_registered: false,
      },
      bind_preflight: {
        listener_binding_allowed: false,
        bind_blocking_gate: {
          gate: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
      },
      dispatch_gate: {
        dispatch_allowed: false,
        acceptance_gate: {
          token_checks_passed: true,
          acceptance_allowed: false,
          reason: "capability_token_acceptance_not_enabled",
        },
      },
    });
  });

  it("previews disabled local MCP request-loop dispatch before invoking handlers", async () => {
    await expect(
      previewLocalMcpTransportRequestLoopDispatch(
        {
          id: "loop-dispatch-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      request_loop_dispatch_enabled: false,
      request_loop_registered: false,
      request_loop_started: false,
      dispatch_attempted: false,
      request_accepted: false,
      handler_invoked: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      registration_preview: {
        request_loop_registration_enabled: false,
        request_loop_registered: false,
        request_loop_started: false,
        dispatch_gate: {
          dispatch_allowed: false,
          auth_gate: {
            reason: "capability_token_missing",
          },
        },
      },
      dispatch_gate: {
        dispatch_allowed: false,
        envelope_validation: {
          valid: true,
        },
        auth_gate: {
          token_checks_passed: false,
          reason: "capability_token_missing",
        },
      },
    });

    const request = {
      request_id: "token-request-3",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-3");

    await expect(
      previewLocalMcpTransportRequestLoopDispatch(
        {
          id: "loop-dispatch-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-3",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      request_loop_dispatch_enabled: false,
      request_loop_registered: false,
      request_loop_started: false,
      dispatch_attempted: false,
      request_accepted: false,
      handler_invoked: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      registration_preview: {
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
        dispatch_gate: {
          dispatch_allowed: false,
          acceptance_gate: {
            reason: "capability_token_acceptance_not_enabled",
          },
        },
      },
      dispatch_gate: {
        dispatch_allowed: false,
        acceptance_gate: {
          token_checks_passed: true,
          acceptance_allowed: false,
          reason: "capability_token_acceptance_not_enabled",
        },
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-loop-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-loop-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "loop-dispatch-read-accept",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-loop-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportRequestLoopDispatch(
        {
          id: "loop-dispatch-read-execute",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-loop-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      request_loop_dispatch_enabled: true,
      request_loop_registered: false,
      request_loop_started: false,
      dispatch_attempted: true,
      request_accepted: true,
      handler_invoked: true,
      response_emitted: false,
      executed: true,
      journaled: false,
      first_blocking_stage: null,
      reason: "dispatch_allowed",
      error_code: undefined,
      dispatch_gate: {
        dispatch_allowed: true,
        transport_gate_enabled: true,
        blocking_gates: [],
        reason: "dispatch_allowed",
      },
      registration_preview: {
        request_loop_registration_enabled: false,
        request_loop_registered: false,
        request_loop_started: false,
        listener_binding_allowed: false,
        first_blocking_stage: "transport",
        reason: "transport_not_enabled",
        dispatch_gate: {
          dispatch_allowed: true,
          transport_gate_enabled: true,
          blocking_gates: [],
          reason: "dispatch_allowed",
        },
        bind_preflight: {
          listener_binding_allowed: false,
          dispatch_gate: {
            dispatch_allowed: true,
            transport_gate_enabled: true,
            blocking_gates: [],
            reason: "dispatch_allowed",
          },
        },
        startup_preview: {
          listener_binding_allowed: false,
          listener_started: false,
          request_loop_started: false,
          bind_preflight: {
            dispatch_gate: {
              dispatch_allowed: true,
              transport_gate_enabled: true,
              blocking_gates: [],
              reason: "dispatch_allowed",
            },
          },
        },
      },
      handler_response: {
        id: "loop-dispatch-read-execute",
        executed: true,
        reason: "dispatch_allowed",
        request_trace: {
          stage: "transport_dispatch",
          status: "dispatch_executed",
          executed: true,
          reason: "dispatch_allowed",
        },
        result: {
          tool: "starlane.spaces.list",
          read_only: true,
          runtime_mode: "local_vault",
        },
      },
    });

    const acceptedWriteTokenRequest = {
      request_id: "token-request-loop-write",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedWriteTokenRequest,
      "local-mcp-loop-write-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "loop-dispatch-write-accept",
        transport: "sse",
        tool: "starlane.documents.delete",
        request: {
          capability_token: "local-mcp-loop-write-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportRequestLoopDispatch(
        {
          id: "loop-dispatch-write-journal",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-loop-write-token",
            confirmation_text: "confirm local mcp starlane.documents.delete",
            input: { document_id: "doc-loop-write" },
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      request_loop_dispatch_enabled: true,
      request_loop_registered: false,
      request_loop_started: false,
      dispatch_attempted: true,
      request_accepted: true,
      handler_invoked: true,
      response_emitted: false,
      executed: false,
      journaled: true,
      first_blocking_stage: "operation_journal",
      reason: "operation_journal_not_enabled",
      error_code: "transport_disabled",
      dispatch_gate: {
        dispatch_allowed: false,
        transport_gate_enabled: true,
        acceptance_gate: {
          acceptance_allowed: true,
          reason: "capability_token_accepted",
        },
        blocking_gates: [
          {
            gate: "operation_journal",
            reason: "operation_journal_not_enabled",
          },
          {
            gate: "write_execution",
            reason: "write_execution_disabled",
          },
        ],
      },
      handler_response: {
        id: "loop-dispatch-write-journal",
        executed: false,
        journaled_preflight: {
          operation_journal_record: {
            record_written: true,
            record: {
              id: "local-mcp-journal:loop-dispatch-write-journal",
              target: "doc-loop-write",
              raw_confirmation_text_stored: false,
              raw_capability_token_stored: false,
              executed: false,
            },
          },
          confirmed_write_closeout: {
            journal_recorded: true,
            execution_allowed: false,
            executed: false,
            record_id: "local-mcp-journal:loop-dispatch-write-journal",
          },
        },
      },
    });
  });

  it("previews disabled local MCP handler invocation before executing tools", async () => {
    await expect(
      previewLocalMcpTransportHandlerInvocation(
        {
          id: "handler-invocation-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      handler_invocation_enabled: false,
      handler_available: false,
      request_loop_started: false,
      request_accepted: false,
      handler_invoked: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      request_loop_dispatch: {
        request_loop_dispatch_enabled: false,
        handler_invoked: false,
        reason: "capability_token_missing",
      },
      dispatch_gate: {
        dispatch_allowed: false,
        auth_gate: {
          reason: "capability_token_missing",
        },
      },
    });

    const request = {
      request_id: "token-request-4",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-4");

    await expect(
      previewLocalMcpTransportHandlerInvocation(
        {
          id: "handler-invocation-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-4",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      handler_invocation_enabled: false,
      handler_available: false,
      request_loop_started: false,
      request_accepted: false,
      handler_invoked: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      request_loop_dispatch: {
        request_loop_dispatch_enabled: false,
        request_accepted: false,
        handler_invoked: false,
        reason: "capability_token_acceptance_not_enabled",
      },
      dispatch_gate: {
        dispatch_allowed: false,
        acceptance_gate: {
          token_checks_passed: true,
          acceptance_allowed: false,
          reason: "capability_token_acceptance_not_enabled",
        },
      },
    });
  });

  it("previews disabled local MCP handler response framing without emitting envelopes", async () => {
    await expect(
      previewLocalMcpTransportHandlerResponseFrame(
        {
          id: "handler-response-frame-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      response_frame_enabled: false,
      handler_available: false,
      handler_invoked: false,
      handler_result_available: false,
      response_envelope_created: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      response_frame: null,
      response_envelope: null,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      handler_invocation: {
        handler_invocation_enabled: false,
        handler_invoked: false,
        reason: "capability_token_missing",
      },
      dispatch_gate: {
        dispatch_allowed: false,
        auth_gate: {
          reason: "capability_token_missing",
        },
      },
    });

    const request = {
      request_id: "token-request-5",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-5");

    await expect(
      previewLocalMcpTransportHandlerResponseFrame(
        {
          id: "handler-response-frame-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-5",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      response_frame_enabled: false,
      handler_available: false,
      handler_invoked: false,
      handler_result_available: false,
      response_envelope_created: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      response_frame: null,
      response_envelope: null,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      handler_invocation: {
        handler_invocation_enabled: false,
        handler_invoked: false,
        reason: "capability_token_acceptance_not_enabled",
      },
      dispatch_gate: {
        dispatch_allowed: false,
        acceptance_gate: {
          token_checks_passed: true,
          acceptance_allowed: false,
          reason: "capability_token_acceptance_not_enabled",
        },
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-response-frame-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-response-frame-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "handler-response-frame-read-accept",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-response-frame-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportHandlerResponseFrame(
        {
          id: "handler-response-frame-read-execute",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-response-frame-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      response_frame_enabled: true,
      handler_available: true,
      handler_invoked: true,
      handler_result_available: true,
      response_envelope_created: true,
      response_emitted: false,
      executed: true,
      journaled: false,
      response_frame: {
        executed: true,
        reason: "dispatch_allowed",
        result: {
          tool: "starlane.spaces.list",
          read_only: true,
          runtime_mode: "local_vault",
        },
      },
      response_envelope: {
        executed: true,
        reason: "dispatch_allowed",
        request_trace: {
          stage: "transport_dispatch",
          status: "dispatch_executed",
          executed: true,
          raw_values_stored: false,
        },
      },
      first_blocking_stage: null,
      reason: "dispatch_allowed",
      error_code: undefined,
      handler_invocation: {
        handler_invocation_enabled: true,
        handler_available: true,
        request_accepted: true,
        handler_invoked: true,
        executed: true,
      },
      dispatch_gate: {
        dispatch_allowed: true,
        transport_gate_enabled: true,
        blocking_gates: [],
        reason: "dispatch_allowed",
      },
    });
  });

  it("previews disabled local MCP response emission without transport output", async () => {
    await expect(
      previewLocalMcpTransportResponseEmission(
        {
          id: "response-emission-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      response_emission_enabled: false,
      transport_output_available: false,
      response_frame_available: false,
      response_envelope_created: false,
      response_emitted: false,
      stdio_stdout_written: false,
      sse_event_sent: false,
      executed: false,
      journaled: false,
      response_frame: null,
      response_envelope: null,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      handler_response_frame: {
        response_frame_enabled: false,
        response_envelope: null,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-6",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-6");

    await expect(
      previewLocalMcpTransportResponseEmission(
        {
          id: "response-emission-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-6",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      response_emission_enabled: false,
      transport_output_available: false,
      response_frame_available: false,
      response_envelope_created: false,
      response_emitted: false,
      stdio_stdout_written: false,
      sse_event_sent: false,
      executed: false,
      journaled: false,
      response_frame: null,
      response_envelope: null,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      handler_response_frame: {
        response_frame_enabled: false,
        response_envelope_created: false,
        response_envelope: null,
        reason: "capability_token_acceptance_not_enabled",
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-response-emission-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-response-emission-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "response-emission-read-accept",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-response-emission-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportResponseEmission(
        {
          id: "response-emission-read-execute",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-response-emission-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      response_emission_enabled: true,
      transport_output_available: true,
      response_frame_available: true,
      response_envelope_created: true,
      response_emitted: true,
      stdio_stdout_written: false,
      sse_event_sent: true,
      executed: true,
      journaled: false,
      response_frame: {
        executed: true,
        reason: "dispatch_allowed",
      },
      response_envelope: {
        executed: true,
        reason: "dispatch_allowed",
        result: {
          tool: "starlane.spaces.list",
          read_only: true,
        },
      },
      first_blocking_stage: null,
      reason: "dispatch_allowed",
      error_code: undefined,
      handler_response_frame: {
        response_frame_enabled: true,
        response_envelope_created: true,
        response_envelope: {
          executed: true,
          reason: "dispatch_allowed",
        },
      },
    });
  });

  it("previews disabled local MCP response completion without lifecycle completion", async () => {
    await expect(
      previewLocalMcpTransportResponseCompletion(
        {
          id: "response-completion-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      response_completion_enabled: false,
      response_emitted: false,
      response_completed: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      executed: false,
      journaled: false,
      response_envelope: null,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      response_emission: {
        response_emission_enabled: false,
        response_emitted: false,
        response_envelope: null,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-7",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-7");

    await expect(
      previewLocalMcpTransportResponseCompletion(
        {
          id: "response-completion-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-7",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      response_completion_enabled: false,
      response_emitted: false,
      response_completed: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      executed: false,
      journaled: false,
      response_envelope: null,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      response_emission: {
        response_emission_enabled: false,
        response_emitted: false,
        response_envelope: null,
        reason: "capability_token_acceptance_not_enabled",
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-response-completion-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-response-completion-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "response-completion-read-accept",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-response-completion-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportResponseCompletion(
        {
          id: "response-completion-read-execute",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-response-completion-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      response_completion_enabled: true,
      response_emitted: true,
      response_completed: true,
      request_completed: true,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      executed: true,
      journaled: false,
      response_envelope: {
        executed: true,
        reason: "dispatch_allowed",
        result: {
          tool: "starlane.spaces.list",
          read_only: true,
        },
      },
      first_blocking_stage: null,
      reason: "dispatch_allowed",
      error_code: undefined,
      response_emission: {
        response_emission_enabled: true,
        response_emitted: true,
        stdio_stdout_written: true,
        sse_event_sent: false,
      },
    });
  });

  it("previews disabled local MCP error-envelope framing without emitting errors", async () => {
    await expect(
      previewLocalMcpTransportErrorEnvelopeFrame(
        {
          id: "error-envelope-frame-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
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
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      response_completion: {
        response_completion_enabled: false,
        response_completed: false,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-8",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-8");

    await expect(
      previewLocalMcpTransportErrorEnvelopeFrame(
        {
          id: "error-envelope-frame-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-8",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
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
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      response_completion: {
        response_completion_enabled: false,
        response_completed: false,
        reason: "capability_token_acceptance_not_enabled",
      },
    });
  });

  it("previews disabled local MCP transport error emission without emitting error output", async () => {
    await expect(
      previewLocalMcpTransportErrorEmission(
        {
          id: "error-emission-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:35:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
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
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      error_envelope_frame: {
        error_envelope_framing_enabled: false,
        error_detected: true,
        error_envelope_created: false,
        error_envelope_emitted: false,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-9",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-9");

    await expect(
      previewLocalMcpTransportErrorEmission(
        {
          id: "error-emission-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-9",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:35:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
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
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      error_envelope_frame: {
        error_envelope_framing_enabled: false,
        error_detected: true,
        error_envelope_created: false,
        error_envelope_emitted: false,
        reason: "capability_token_acceptance_not_enabled",
      },
    });
  });

  it("previews disabled local MCP lifecycle audit without cleanup scheduling", async () => {
    await expect(
      previewLocalMcpTransportLifecycleAudit(
        {
          id: "lifecycle-audit-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:40:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      lifecycle_audit_enabled: false,
      cleanup_scheduling_enabled: false,
      response_path_completed: false,
      error_path_completed: false,
      response_completed: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      lifecycle_audit_recorded: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      response_completion: {
        response_completion_enabled: false,
        response_completed: false,
        cleanup_scheduled: false,
        reason: "capability_token_missing",
      },
      error_emission: {
        error_emission_enabled: false,
        error_envelope_emitted: false,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-10",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-10");

    await expect(
      previewLocalMcpTransportLifecycleAudit(
        {
          id: "lifecycle-audit-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-10",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:40:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      lifecycle_audit_enabled: false,
      cleanup_scheduling_enabled: false,
      response_path_completed: false,
      error_path_completed: false,
      response_completed: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      lifecycle_audit_recorded: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      response_completion: {
        response_completion_enabled: false,
        response_completed: false,
        cleanup_scheduled: false,
        reason: "capability_token_acceptance_not_enabled",
      },
      error_emission: {
        error_emission_enabled: false,
        error_envelope_emitted: false,
        reason: "capability_token_acceptance_not_enabled",
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-lifecycle-audit-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-lifecycle-audit-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "lifecycle-audit-read-accept",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-lifecycle-audit-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportLifecycleAudit(
        {
          id: "lifecycle-audit-read-execute",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-lifecycle-audit-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      lifecycle_audit_enabled: true,
      cleanup_scheduling_enabled: false,
      response_path_completed: true,
      error_path_completed: false,
      response_completed: true,
      request_completed: true,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      lifecycle_audit_recorded: false,
      executed: true,
      journaled: false,
      first_blocking_stage: null,
      reason: "dispatch_allowed",
      error_code: undefined,
      response_completion: {
        response_completion_enabled: true,
        response_completed: true,
        request_completed: true,
        response_emission: {
          response_emitted: true,
          sse_event_sent: true,
        },
      },
      error_emission: {
        error_emission_enabled: false,
        error_envelope_emitted: false,
      },
    });
  });

  it("previews disabled local MCP request closeout without cleanup queue handoff", async () => {
    await expect(
      previewLocalMcpTransportRequestCloseout(
        {
          id: "request-closeout-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:45:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      request_closeout_enabled: false,
      cleanup_queue_handoff_enabled: false,
      closeout_attempted: false,
      cleanup_queue_handoff_attempted: false,
      cleanup_queue_enqueued: false,
      response_completed: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      lifecycle_audit_recorded: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      lifecycle_audit: {
        lifecycle_audit_enabled: false,
        cleanup_scheduling_enabled: false,
        lifecycle_audit_recorded: false,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-11",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-11");

    await expect(
      previewLocalMcpTransportRequestCloseout(
        {
          id: "request-closeout-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-11",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:45:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      request_closeout_enabled: false,
      cleanup_queue_handoff_enabled: false,
      closeout_attempted: false,
      cleanup_queue_handoff_attempted: false,
      cleanup_queue_enqueued: false,
      response_completed: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      lifecycle_audit_recorded: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      lifecycle_audit: {
        lifecycle_audit_enabled: false,
        cleanup_scheduling_enabled: false,
        lifecycle_audit_recorded: false,
        reason: "capability_token_acceptance_not_enabled",
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-closeout-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-closeout-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "request-closeout-read-accept",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-closeout-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportRequestCloseout(
        {
          id: "request-closeout-read-execute",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-closeout-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      request_closeout_enabled: true,
      cleanup_queue_handoff_enabled: false,
      closeout_attempted: true,
      cleanup_queue_handoff_attempted: false,
      cleanup_queue_enqueued: false,
      response_completed: true,
      request_completed: true,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      lifecycle_audit_recorded: false,
      executed: true,
      journaled: false,
      first_blocking_stage: null,
      reason: "dispatch_allowed",
      error_code: undefined,
      lifecycle_audit: {
        lifecycle_audit_enabled: true,
        response_path_completed: true,
        response_completed: true,
        request_completed: true,
        lifecycle_completed: false,
        cleanup_scheduled: false,
      },
    });
  });

  it("previews disabled local MCP terminal cleanup without adapter closeout handoff", async () => {
    await expect(
      previewLocalMcpTransportTerminalCleanup(
        {
          id: "terminal-cleanup-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:50:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      terminal_cleanup_enabled: false,
      adapter_closeout_handoff_enabled: false,
      terminal_cleanup_attempted: false,
      adapter_closeout_handoff_attempted: false,
      adapter_closeout_handed_off: false,
      cleanup_queue_enqueued: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      lifecycle_audit_recorded: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      request_closeout: {
        request_closeout_enabled: false,
        cleanup_queue_handoff_enabled: false,
        closeout_attempted: false,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-12",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-12");

    await expect(
      previewLocalMcpTransportTerminalCleanup(
        {
          id: "terminal-cleanup-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-12",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:50:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      terminal_cleanup_enabled: false,
      adapter_closeout_handoff_enabled: false,
      terminal_cleanup_attempted: false,
      adapter_closeout_handoff_attempted: false,
      adapter_closeout_handed_off: false,
      cleanup_queue_enqueued: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      lifecycle_audit_recorded: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      request_closeout: {
        request_closeout_enabled: false,
        cleanup_queue_handoff_enabled: false,
        closeout_attempted: false,
        reason: "capability_token_acceptance_not_enabled",
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-terminal-cleanup-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-terminal-cleanup-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "terminal-cleanup-read-accept",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-terminal-cleanup-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportTerminalCleanup(
        {
          id: "terminal-cleanup-read-execute",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-terminal-cleanup-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      terminal_cleanup_enabled: true,
      adapter_closeout_handoff_enabled: false,
      terminal_cleanup_attempted: true,
      adapter_closeout_handoff_attempted: false,
      adapter_closeout_handed_off: false,
      cleanup_queue_enqueued: false,
      request_completed: true,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      lifecycle_audit_recorded: false,
      executed: true,
      journaled: false,
      first_blocking_stage: null,
      reason: "dispatch_allowed",
      error_code: undefined,
      request_closeout: {
        request_closeout_enabled: true,
        closeout_attempted: true,
        request_completed: true,
        cleanup_queue_handoff_enabled: false,
        cleanup_queue_enqueued: false,
      },
    });
  });

  it("previews disabled local MCP handler terminal state without recording terminal state", async () => {
    await expect(
      previewLocalMcpTransportHandlerTerminalState(
        {
          id: "handler-terminal-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:55:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      handler_terminal_state_enabled: false,
      terminal_state_recorded: false,
      handler_available: false,
      handled: false,
      request_accepted: false,
      response_emitted: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      cleanup_queue_enqueued: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      terminal_cleanup: {
        terminal_cleanup_enabled: false,
        adapter_closeout_handoff_enabled: false,
        terminal_cleanup_attempted: false,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-13",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-13");

    await expect(
      previewLocalMcpTransportHandlerTerminalState(
        {
          id: "handler-terminal-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-13",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:55:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      handler_terminal_state_enabled: false,
      terminal_state_recorded: false,
      handler_available: false,
      handled: false,
      request_accepted: false,
      response_emitted: false,
      request_completed: false,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      cleanup_queue_enqueued: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      terminal_cleanup: {
        terminal_cleanup_enabled: false,
        adapter_closeout_handoff_enabled: false,
        terminal_cleanup_attempted: false,
        reason: "capability_token_acceptance_not_enabled",
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-handler-terminal-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-handler-terminal-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "handler-terminal-read-accept",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-handler-terminal-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportHandlerTerminalState(
        {
          id: "handler-terminal-read-execute",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-handler-terminal-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      handler_terminal_state_enabled: true,
      terminal_state_recorded: false,
      handler_available: true,
      handled: true,
      request_accepted: true,
      response_emitted: true,
      request_completed: true,
      lifecycle_completed: false,
      cleanup_scheduled: false,
      cleanup_queue_enqueued: false,
      executed: true,
      journaled: false,
      first_blocking_stage: null,
      reason: "dispatch_allowed",
      error_code: undefined,
      terminal_cleanup: {
        terminal_cleanup_enabled: true,
        terminal_cleanup_attempted: true,
        request_completed: true,
        executed: true,
        journaled: false,
        first_blocking_stage: null,
        reason: "dispatch_allowed",
      },
    });
  });

  it("previews disabled local MCP transport enablement readiness without allowing stdio or SSE", async () => {
    await expect(
      previewLocalMcpTransportEnablementReadiness(
        {
          id: "transport-enablement-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      transport_enablement_readiness_enabled: false,
      transport_enablement_allowed: false,
      stdio_enablement_allowed: false,
      sse_enablement_allowed: false,
      handler_terminal_state_ready: false,
      capability_token_acceptance_ready: false,
      operation_journal_ready: false,
      confirmed_write_ready: false,
      listener_binding_ready: false,
      request_loop_ready: false,
      response_pipeline_ready: false,
      cleanup_pipeline_ready: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      dispatch_gate: {
        dispatch_allowed: false,
        reason: "capability_token_missing",
        blocking_gates: [
          {
            gate: "capability_token",
            reason: "capability_token_missing",
            error_code: "capability_token_required",
          },
          {
            gate: "transport",
            reason: "transport_not_enabled",
            error_code: "transport_disabled",
          },
        ],
      },
      transport_readiness: {
        transport_available: false,
        status: "disabled",
        read_only: true,
        executable: false,
        blocking_gates: [
          {
            gate: "transport",
            status: "disabled",
            error_code: "transport_disabled",
          },
          {
            gate: "capability_token",
            status: "disabled",
            error_code: "capability_token_required",
          },
        ],
      },
      handler_terminal_state: {
        handler_terminal_state_enabled: false,
        terminal_state_recorded: false,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-14",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-14");

    await expect(
      previewLocalMcpTransportEnablementReadiness(
        {
          id: "transport-enablement-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-14",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      transport_enablement_readiness_enabled: false,
      transport_enablement_allowed: false,
      stdio_enablement_allowed: false,
      sse_enablement_allowed: false,
      handler_terminal_state_ready: false,
      capability_token_acceptance_ready: false,
      operation_journal_ready: false,
      confirmed_write_ready: false,
      listener_binding_ready: false,
      request_loop_ready: false,
      response_pipeline_ready: false,
      cleanup_pipeline_ready: false,
      executed: false,
      journaled: false,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      dispatch_gate: {
        dispatch_allowed: false,
        reason: "capability_token_acceptance_not_enabled",
        blocking_gates: [
          {
            gate: "token_acceptance",
            reason: "capability_token_acceptance_not_enabled",
            error_code: "capability_token_not_enabled",
          },
          {
            gate: "transport",
            reason: "transport_not_enabled",
            error_code: "transport_disabled",
          },
          {
            gate: "operation_journal",
            reason: "operation_journal_not_enabled",
            error_code: "transport_disabled",
          },
          {
            gate: "write_execution",
            reason: "write_execution_disabled",
            error_code: "write_execution_disabled",
          },
        ],
      },
      transport_readiness: {
        transport_available: false,
        status: "disabled",
        read_only: false,
        executable: false,
        write_execution: {
          status: "disabled",
          error_code: "write_execution_disabled",
        },
      },
      handler_terminal_state: {
        handler_terminal_state_enabled: false,
        terminal_state_recorded: false,
        reason: "capability_token_acceptance_not_enabled",
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-transport-enablement-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-transport-enablement-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "transport-enablement-read-accept",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-transport-enablement-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportEnablementReadiness(
        {
          id: "transport-enablement-read-execute",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-transport-enablement-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      transport_enablement_readiness_enabled: true,
      transport_enablement_allowed: true,
      stdio_enablement_allowed: true,
      sse_enablement_allowed: false,
      handler_terminal_state_ready: true,
      capability_token_acceptance_ready: true,
      operation_journal_ready: true,
      confirmed_write_ready: false,
      listener_binding_ready: true,
      request_loop_ready: true,
      response_pipeline_ready: true,
      cleanup_pipeline_ready: true,
      executed: true,
      journaled: false,
      first_blocking_stage: null,
      reason: "dispatch_allowed",
      error_code: undefined,
      dispatch_gate: {
        dispatch_allowed: true,
        blocking_gates: [],
      },
      handler_terminal_state: {
        handler_terminal_state_enabled: true,
        handler_available: true,
        handled: true,
        request_accepted: true,
        response_emitted: true,
        request_completed: true,
        executed: true,
        journaled: false,
        terminal_cleanup: {
          terminal_cleanup_enabled: true,
          terminal_cleanup_attempted: true,
          request_completed: true,
        },
      },
    });
  });

  it("previews local MCP capability-token acceptance readiness", async () => {
    await expect(
      previewLocalMcpCapabilityTokenAcceptanceReadiness(
        {
          id: "token-acceptance-readiness-missing-token",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      acceptance_persistence_enabled: false,
      persistence_allowed: false,
      can_persist_acceptance: false,
      token_record_found: false,
      token_checks_passed: false,
      token_accepted: false,
      accepted_token_persisted: false,
      accepted_at: null,
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
      first_blocking_stage: "capability_token",
      reason: "capability_token_missing",
      error_code: "capability_token_required",
      acceptance_gate: {
        token_checks_passed: false,
        token_acceptance_enabled: false,
        acceptance_allowed: false,
        reason: "capability_token_missing",
      },
    });

    const request = {
      request_id: "token-request-16",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-16");

    await expect(
      previewLocalMcpCapabilityTokenAcceptanceReadiness(
        {
          id: "token-acceptance-readiness-token-ready",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-16",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      acceptance_persistence_enabled: false,
      persistence_allowed: false,
      can_persist_acceptance: true,
      token_record_found: true,
      token_checks_passed: true,
      token_accepted: false,
      accepted_token_persisted: false,
      accepted_at: null,
      raw_token_stored: false,
      first_blocking_stage: "token_acceptance",
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      record: {
        token_id: "local-mcp-token:token-request-16",
        persisted: true,
        accepted: false,
        raw_token_stored: false,
        reason: "capability_token_issued",
      },
      acceptance_gate: {
        token_checks_passed: true,
        token_acceptance_enabled: false,
        acceptance_allowed: false,
        reason: "capability_token_acceptance_not_enabled",
      },
    });

    expect(findLocalMcpCapabilityTokenRecord("local-mcp-token:token-request-16")).toMatchObject({
      accepted: false,
      raw_token_stored: false,
      reason: "capability_token_issued",
    });
  });

  it("persists local MCP capability-token acceptance only with explicit enablement", async () => {
    const request = {
      request_id: "token-request-19",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-19");

    await expect(
      acceptLocalMcpCapabilityTokenRecord(
        {
          id: "accept-token-disabled",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-19",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      token_accepted: false,
      acceptance_enabled: false,
      reason: "capability_token_acceptance_not_enabled",
      acceptance_readiness: {
        can_persist_acceptance: true,
        token_record_found: true,
        token_checks_passed: true,
        accepted_token_persisted: false,
      },
    });

    expect(findLocalMcpCapabilityTokenRecord("local-mcp-token:token-request-19")).toMatchObject({
      accepted: false,
      raw_token_stored: false,
      reason: "capability_token_issued",
    });

    await expect(
      acceptLocalMcpCapabilityTokenRecord(
        {
          id: "accept-token-enabled",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-19",
            confirmation_text: "confirm local mcp starlane.documents.delete",
          },
        },
        "2026-07-13T13:30:00.000Z",
        { acceptance_enabled: true, accepted_at: "2026-07-13T13:31:00.000Z" },
      ),
    ).resolves.toMatchObject({
      token_accepted: true,
      acceptance_enabled: true,
      record: {
        token_id: "local-mcp-token:token-request-19",
        accepted: true,
        accepted_at: "2026-07-13T13:31:00.000Z",
        raw_token_stored: false,
        reason: "capability_token_accepted",
      },
      acceptance_readiness: {
        can_persist_acceptance: true,
        token_record_found: true,
        token_checks_passed: true,
      },
    });

    expect(findLocalMcpCapabilityTokenRecord("local-mcp-token:token-request-19")).toMatchObject({
      accepted: true,
      accepted_at: "2026-07-13T13:31:00.000Z",
      raw_token_stored: false,
      reason: "capability_token_accepted",
    });

    await expect(
      previewLocalMcpCapabilityTokenValidation(
        "local-mcp-raw-token-19",
        "2026-07-13T13:32:00.000Z",
      ),
    ).resolves.toMatchObject({
      token_accepted: true,
      token_record_found: true,
      fingerprint_matched: true,
      expired: false,
      reason: "capability_token_not_enabled",
      record: {
        accepted: true,
        accepted_at: "2026-07-13T13:31:00.000Z",
        raw_token_stored: false,
        reason: "capability_token_accepted",
      },
    });

    await expect(
      previewLocalMcpCapabilityTokenAcceptanceReadiness(
        {
          id: "accepted-token-readiness",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-19",
          },
        },
        "2026-07-13T13:32:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      can_persist_acceptance: false,
      token_record_found: true,
      token_checks_passed: true,
      token_accepted: true,
      accepted_token_persisted: true,
      accepted_at: "2026-07-13T13:31:00.000Z",
      first_blocking_stage: "transport",
      reason: "capability_token_accepted",
      error_code: "transport_disabled",
      acceptance_gate: {
        token_acceptance_enabled: true,
        token_checks_passed: true,
        token_accepted: true,
        acceptance_allowed: true,
        listener_binding_allowed: false,
        transport_dispatch_allowed: false,
        reason: "capability_token_accepted",
      },
    });

    const acceptedTokenDispatchGate = await previewLocalMcpTransportDispatchGate(
      {
        id: "accepted-token-dispatch-gate",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-raw-token-19",
        },
      },
      "2026-07-13T13:32:00.000Z",
    );
    expect(acceptedTokenDispatchGate).toMatchObject({
      runtime_mode: "local_vault",
      dispatch_allowed: false,
      executed: false,
      transport: "sse",
      tool: "starlane.spaces.list",
      auth_gate: {
        token_checks_passed: true,
        token_accepted: true,
        transport_dispatch_allowed: false,
      },
      acceptance_gate: {
        token_checks_passed: true,
        token_accepted: true,
        acceptance_allowed: true,
        transport_dispatch_allowed: false,
      },
      blocking_gates: [
        {
          gate: "transport",
          status: "disabled",
          reason: "transport_not_enabled",
          error_code: "transport_disabled",
        },
      ],
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      transport_gate_enabled: false,
    });
    expect(acceptedTokenDispatchGate.dispatch_allowed).toBe(
      acceptedTokenDispatchGate.blocking_gates.length === 0,
    );
    await expect(
      previewLocalMcpTransportDispatchGate(
        {
          id: "accepted-token-dispatch-gate-enabled-transport",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-19",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      dispatch_allowed: true,
      executed: false,
      transport_gate_enabled: true,
      blocking_gates: [],
      reason: "dispatch_allowed",
      error_code: undefined,
    });
  });

  it("previews disabled local MCP transport adapter shutdown without teardown side effects", () => {
    expect(previewLocalMcpTransportAdapterShutdown("stdio")).toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
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
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      startup_preview: {
        transport: "stdio",
        startup_attempted: false,
        listener_started: false,
        request_loop_started: false,
        stdio_streams_attached: false,
        sse_endpoint_registered: false,
        bind_preflight: {
          transport: "stdio",
          bound: false,
        },
      },
    });

    expect(
      previewLocalMcpTransportAdapterShutdown("sse", "starlane.documents.delete"),
    ).toMatchObject({
      transport: "sse",
      shutdown_attempted: false,
      listener_closed: false,
      request_loop_stopped: false,
      stdio_streams_detached: false,
      sse_endpoint_unregistered: false,
      startup_preview: {
        transport: "sse",
        sse_endpoint_registered: false,
        bind_preflight: {
          operation_journal_readiness: {
            preview: {
              tool: "starlane.documents.delete",
              required: true,
            },
          },
        },
      },
    });
  });

  it("previews disabled local MCP transport adapter runtime state without activating transport", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpTransportAdapterRuntimeState(
        {
          id: "adapter-runtime-state-1",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
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
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      bind_preflight: {
        transport: "stdio",
        bound: false,
        dispatch_gate: {
          dispatch_allowed: false,
          acceptance_gate: {
            token_checks_passed: true,
            acceptance_allowed: false,
          },
        },
      },
      startup_preview: {
        transport: "stdio",
        listener_started: false,
      },
      shutdown_preview: {
        transport: "stdio",
        listener_closed: false,
      },
      request_preview: {
        transport: "stdio",
        dispatch_gate: {
          envelope_validation: {
            valid: true,
          },
          auth_gate: {
            token_checks_passed: true,
          },
        },
      },
      response_preview: {
        transport: "stdio",
        response_envelope: null,
      },
      lifecycle_preview: {
        transport: "stdio",
        completed: false,
        first_blocking_stage: "configure",
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-runtime-state-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-runtime-state-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "runtime-state-read-accept",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-runtime-state-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportAdapterRuntimeState(
        {
          id: "adapter-runtime-state-read-execute",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-runtime-state-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "sse",
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
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      bind_preflight: {
        listener_binding_allowed: false,
        dispatch_gate: {
          dispatch_allowed: true,
          blocking_gates: [],
        },
      },
      startup_preview: {
        listener_started: false,
        listener_binding_allowed: false,
        bind_preflight: {
          dispatch_gate: {
            dispatch_allowed: true,
            blocking_gates: [],
          },
        },
      },
      shutdown_preview: {
        listener_closed: false,
        startup_preview: {
          bind_preflight: {
            dispatch_gate: {
              dispatch_allowed: true,
              blocking_gates: [],
            },
          },
        },
      },
      response_preview: {
        response_emitted: false,
        executed: false,
        response_envelope: null,
        request_preview: {
          dispatch_gate: {
            dispatch_allowed: true,
            blocking_gates: [],
          },
        },
      },
      lifecycle_preview: {
        completed: false,
        response_preview: {
          request_preview: {
            dispatch_gate: {
              dispatch_allowed: true,
              blocking_gates: [],
            },
          },
        },
      },
    });

    await expect(
      previewLocalMcpTransportAdapterRuntimeState(
        {
          id: "adapter-runtime-state-invalid",
          transport: "smtp" as "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      runtime_active: false,
      request_preview: {
        dispatch_gate: {
          envelope_validation: {
            valid: false,
            reason: "transport_unsupported",
          },
        },
      },
      response_preview: {
        response_envelope: null,
      },
      lifecycle_preview: {
        completed: false,
      },
    });
  });

  it("previews a non-running local MCP transport handler skeleton", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpTransportHandlerSkeleton(
        {
          id: "handler-skeleton-1",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-1",
            confirmation_text: "confirm local mcp starlane.documents.delete",
            input: { document_id: "doc-1" },
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "sse",
      handler_available: false,
      handler_started: false,
      listener_binding_allowed: false,
      request_loop_started: false,
      request_accepted: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
      bind_preflight: {
        listener_binding_allowed: false,
        bind_blocking_gate: {
          gate: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
      },
      startup_preview: {
        listener_binding_allowed: false,
        listener_started: false,
        sse_endpoint_registered: false,
      },
      request_loop_registration: {
        request_loop_registration_enabled: false,
        request_loop_registered: false,
        request_loop_started: false,
        accepts_requests: false,
        emits_responses: false,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      request_loop_dispatch: {
        request_loop_dispatch_enabled: false,
        request_loop_registered: false,
        request_loop_started: false,
        dispatch_attempted: false,
        request_accepted: false,
        handler_invoked: false,
        response_emitted: false,
        executed: false,
        journaled: false,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      handler_invocation: {
        handler_invocation_enabled: false,
        handler_available: false,
        request_loop_started: false,
        request_accepted: false,
        handler_invoked: false,
        response_emitted: false,
        executed: false,
        journaled: false,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      handler_response_frame: {
        response_frame_enabled: false,
        handler_available: false,
        handler_invoked: false,
        handler_result_available: false,
        response_envelope_created: false,
        response_emitted: false,
        executed: false,
        journaled: false,
        response_frame: null,
        response_envelope: null,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      response_emission: {
        response_emission_enabled: false,
        transport_output_available: false,
        response_frame_available: false,
        response_envelope_created: false,
        response_emitted: false,
        stdio_stdout_written: false,
        sse_event_sent: false,
        executed: false,
        journaled: false,
        response_frame: null,
        response_envelope: null,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      response_completion: {
        response_completion_enabled: false,
        response_emitted: false,
        response_completed: false,
        request_completed: false,
        lifecycle_completed: false,
        cleanup_scheduled: false,
        executed: false,
        journaled: false,
        response_envelope: null,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      error_envelope_frame: {
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
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      error_emission: {
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
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      lifecycle_audit: {
        lifecycle_audit_enabled: false,
        cleanup_scheduling_enabled: false,
        response_path_completed: false,
        error_path_completed: false,
        response_completed: false,
        request_completed: false,
        lifecycle_completed: false,
        cleanup_scheduled: false,
        lifecycle_audit_recorded: false,
        executed: false,
        journaled: false,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      request_closeout: {
        request_closeout_enabled: false,
        cleanup_queue_handoff_enabled: false,
        closeout_attempted: false,
        cleanup_queue_handoff_attempted: false,
        cleanup_queue_enqueued: false,
        response_completed: false,
        request_completed: false,
        lifecycle_completed: false,
        cleanup_scheduled: false,
        lifecycle_audit_recorded: false,
        executed: false,
        journaled: false,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      terminal_cleanup: {
        terminal_cleanup_enabled: false,
        adapter_closeout_handoff_enabled: false,
        terminal_cleanup_attempted: false,
        adapter_closeout_handoff_attempted: false,
        adapter_closeout_handed_off: false,
        cleanup_queue_enqueued: false,
        request_completed: false,
        lifecycle_completed: false,
        cleanup_scheduled: false,
        lifecycle_audit_recorded: false,
        executed: false,
        journaled: false,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      handler_terminal_state: {
        handler_terminal_state_enabled: false,
        terminal_state_recorded: false,
        handler_available: false,
        handled: false,
        request_accepted: false,
        response_emitted: false,
        request_completed: false,
        lifecycle_completed: false,
        cleanup_scheduled: false,
        cleanup_queue_enqueued: false,
        executed: false,
        journaled: false,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      transport_enablement_readiness: {
        transport_enablement_readiness_enabled: false,
        transport_enablement_allowed: false,
        stdio_enablement_allowed: false,
        sse_enablement_allowed: false,
        handler_terminal_state_ready: false,
        capability_token_acceptance_ready: false,
        operation_journal_ready: false,
        confirmed_write_ready: false,
        listener_binding_ready: false,
        request_loop_ready: false,
        response_pipeline_ready: false,
        cleanup_pipeline_ready: false,
        executed: false,
        journaled: false,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
      },
      capability_token_acceptance_readiness: {
        acceptance_persistence_enabled: false,
        persistence_allowed: false,
        can_persist_acceptance: true,
        token_record_found: true,
        token_checks_passed: true,
        accepted_token_persisted: false,
        accepted_at: null,
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
        token_accepted: false,
        executed: false,
        journaled: false,
        first_blocking_stage: "token_acceptance",
        reason: "capability_token_acceptance_not_enabled",
        error_code: "capability_token_not_enabled",
        acceptance_gate: {
          token_checks_passed: true,
          acceptance_allowed: false,
          listener_binding_allowed: false,
          transport_dispatch_allowed: false,
          reason: "capability_token_acceptance_not_enabled",
        },
      },
      dispatch_gate: {
        dispatch_allowed: false,
        executed: false,
        acceptance_gate: {
          acceptance_allowed: false,
          reason: "capability_token_acceptance_not_enabled",
        },
      },
      preflight: {
        transport_available: false,
        tool_error_code: "write_execution_disabled",
      },
      operation_journal_gate_preview: {
        journal_required: true,
        confirmation_valid: true,
        would_record_journal: true,
        record_written: false,
        reason: "operation_journal_not_enabled",
      },
      operation_journal_replay_preview: {
        request_id_present: true,
        idempotency_key: "local-mcp-journal:handler-skeleton-1",
        replay_detected: false,
        record_write_allowed: true,
      },
      runtime_state: {
        runtime_active: false,
        bound: false,
        request_accepted: false,
        response_emitted: false,
        executed: false,
        journaled: false,
      },
    });

    const acceptedReadTokenRequest = {
      request_id: "token-request-handler-skeleton-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      acceptedReadTokenRequest,
      "local-mcp-handler-skeleton-read-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "handler-skeleton-read-accept",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-handler-skeleton-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportHandlerSkeleton(
        {
          id: "handler-skeleton-read-execute",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-handler-skeleton-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      handler_available: true,
      handler_started: false,
      listener_binding_allowed: false,
      request_loop_started: false,
      request_accepted: true,
      response_emitted: true,
      executed: true,
      journaled: false,
      reason: "dispatch_allowed",
      error_code: undefined,
      bind_preflight: {
        dispatch_gate: {
          dispatch_allowed: true,
          blocking_gates: [],
        },
      },
      request_loop_dispatch: {
        request_loop_dispatch_enabled: true,
        dispatch_attempted: true,
        request_accepted: true,
        handler_invoked: true,
        executed: true,
        first_blocking_stage: null,
        reason: "dispatch_allowed",
      },
      handler_response_frame: {
        response_frame_enabled: true,
        handler_result_available: true,
        response_envelope_created: true,
        executed: true,
      },
      response_emission: {
        response_emission_enabled: true,
        response_emitted: true,
        sse_event_sent: true,
        executed: true,
      },
      terminal_cleanup: {
        terminal_cleanup_enabled: true,
        terminal_cleanup_attempted: true,
        request_completed: true,
        executed: true,
      },
      handler_terminal_state: {
        handler_terminal_state_enabled: true,
        handler_available: true,
        handled: true,
        request_accepted: true,
        response_emitted: true,
        request_completed: true,
        executed: true,
      },
      transport_enablement_readiness: {
        transport_enablement_readiness_enabled: true,
        transport_enablement_allowed: true,
        sse_enablement_allowed: true,
        request_loop_ready: true,
        response_pipeline_ready: true,
        cleanup_pipeline_ready: true,
        executed: true,
      },
      dispatch_gate: {
        dispatch_allowed: true,
        blocking_gates: [],
      },
      runtime_state: {
        runtime_active: false,
        bound: false,
        request_accepted: false,
        response_emitted: false,
        executed: false,
        journaled: false,
      },
    });
    expect(listLocalMcpOperationJournalRecords()).toEqual([]);
  });

  it("returns a disabled local MCP transport handler skeleton response shell", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      handleLocalMcpTransportHandlerSkeleton(
        {
          id: "handler-shell-1",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      handler_available: false,
      handled: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      response_envelope: null,
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      skeleton: {
        handler_available: false,
        handler_started: false,
        request_accepted: false,
        response_emitted: false,
        executed: false,
        journaled: false,
        request_loop_registration: {
          request_loop_registered: false,
          request_loop_started: false,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        request_loop_dispatch: {
          request_loop_dispatch_enabled: false,
          dispatch_attempted: false,
          request_accepted: false,
          handler_invoked: false,
          response_emitted: false,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        handler_invocation: {
          handler_invocation_enabled: false,
          handler_available: false,
          request_accepted: false,
          handler_invoked: false,
          response_emitted: false,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        handler_response_frame: {
          response_frame_enabled: false,
          handler_result_available: false,
          response_envelope_created: false,
          response_emitted: false,
          response_frame: null,
          response_envelope: null,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        response_emission: {
          response_emission_enabled: false,
          transport_output_available: false,
          response_frame_available: false,
          response_envelope_created: false,
          response_emitted: false,
          response_frame: null,
          response_envelope: null,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        response_completion: {
          response_completion_enabled: false,
          response_completed: false,
          request_completed: false,
          lifecycle_completed: false,
          cleanup_scheduled: false,
          response_envelope: null,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        error_envelope_frame: {
          error_envelope_framing_enabled: false,
          error_detected: true,
          error_envelope_created: false,
          error_envelope_emitted: false,
          response_completed: false,
          request_completed: false,
          lifecycle_completed: false,
          error_envelope: null,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        error_emission: {
          error_emission_enabled: false,
          error_envelope_available: false,
          error_envelope_emitted: false,
          stdio_stderr_written: false,
          sse_error_event_sent: false,
          response_completed: false,
          request_completed: false,
          lifecycle_completed: false,
          error_envelope: null,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        lifecycle_audit: {
          lifecycle_audit_enabled: false,
          cleanup_scheduling_enabled: false,
          response_path_completed: false,
          error_path_completed: false,
          response_completed: false,
          request_completed: false,
          lifecycle_completed: false,
          cleanup_scheduled: false,
          lifecycle_audit_recorded: false,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        request_closeout: {
          request_closeout_enabled: false,
          cleanup_queue_handoff_enabled: false,
          closeout_attempted: false,
          cleanup_queue_handoff_attempted: false,
          cleanup_queue_enqueued: false,
          request_completed: false,
          lifecycle_completed: false,
          cleanup_scheduled: false,
          lifecycle_audit_recorded: false,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        terminal_cleanup: {
          terminal_cleanup_enabled: false,
          adapter_closeout_handoff_enabled: false,
          terminal_cleanup_attempted: false,
          adapter_closeout_handoff_attempted: false,
          adapter_closeout_handed_off: false,
          cleanup_queue_enqueued: false,
          request_completed: false,
          lifecycle_completed: false,
          cleanup_scheduled: false,
          lifecycle_audit_recorded: false,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        handler_terminal_state: {
          handler_terminal_state_enabled: false,
          terminal_state_recorded: false,
          handler_available: false,
          handled: false,
          request_accepted: false,
          response_emitted: false,
          request_completed: false,
          lifecycle_completed: false,
          cleanup_scheduled: false,
          cleanup_queue_enqueued: false,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        transport_enablement_readiness: {
          transport_enablement_readiness_enabled: false,
          transport_enablement_allowed: false,
          stdio_enablement_allowed: false,
          sse_enablement_allowed: false,
          handler_terminal_state_ready: false,
          capability_token_acceptance_ready: false,
          operation_journal_ready: false,
          confirmed_write_ready: false,
          listener_binding_ready: false,
          request_loop_ready: false,
          response_pipeline_ready: false,
          cleanup_pipeline_ready: false,
          first_blocking_stage: "token_acceptance",
          reason: "capability_token_acceptance_not_enabled",
        },
        dispatch_gate: {
          dispatch_allowed: false,
          acceptance_gate: {
            acceptance_allowed: false,
          },
        },
        operation_journal_gate_preview: {
          journal_required: false,
          would_record_journal: false,
          record_written: false,
        },
        operation_journal_replay_preview: {
          idempotency_key: "local-mcp-journal:handler-shell-1",
          replay_detected: false,
        },
      },
    });
    expect(listLocalMcpOperationJournalRecords()).toEqual([]);
  });

  it("previews disabled local MCP transport adapter responses without emitting envelopes", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpTransportAdapterResponse(
        {
          id: "adapter-response-preview-1",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "sse",
      adapter_available: false,
      request_accepted: false,
      response_emitted: false,
      executed: false,
      response_envelope: null,
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      request_preview: {
        transport: "sse",
        adapter_available: false,
        bindable: false,
        request_accepted: false,
        response_emitted: false,
        executed: false,
        dispatch_gate: {
          dispatch_allowed: false,
          envelope_validation: {
            valid: true,
          },
          auth_gate: {
            token_checks_passed: true,
            reason: "capability_token_ready",
          },
        },
      },
    });

    await expect(
      previewLocalMcpTransportAdapterResponse(
        {
          id: "",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      adapter_available: false,
      request_accepted: false,
      response_emitted: false,
      executed: false,
      response_envelope: null,
      request_preview: {
        dispatch_gate: {
          envelope_validation: {
            valid: false,
            reason: "request_id_required",
          },
          blocking_gates: [
            {
              gate: "envelope",
              status: "blocked",
              reason: "request_id_required",
              error_code: "invalid_request",
            },
          ],
        },
      },
    });

    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "adapter-response-accept-token",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-raw-token-1",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportAdapterResponse(
        {
          id: "adapter-response-preview-enabled",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "sse",
      adapter_available: false,
      request_accepted: false,
      response_emitted: false,
      executed: false,
      response_envelope: null,
      request_preview: {
        request_accepted: false,
        response_emitted: false,
        executed: false,
        dispatch_gate: {
          dispatch_allowed: true,
          transport_gate_enabled: true,
          blocking_gates: [],
          reason: "dispatch_allowed",
        },
      },
    });
  });

  it("previews disabled local MCP transport adapter lifecycle without completing stages", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpTransportAdapterLifecycle(
        {
          id: "adapter-lifecycle-preview-1",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      schema_version: 1,
      runtime_mode: "local_vault",
      transport: "stdio",
      lifecycle_enabled: false,
      adapter_available: false,
      request_accepted: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      completed: false,
      first_blocking_stage: "configure",
      reason: "transport_not_enabled",
      error_code: "transport_disabled",
      stages: [
        { stage: "configure", enabled: false, completed: false },
        { stage: "bind", enabled: false, completed: false },
        { stage: "authenticate", enabled: false, completed: false },
        { stage: "preflight", enabled: false, completed: false },
        { stage: "dispatch", enabled: false, completed: false },
        { stage: "respond", enabled: false, completed: false },
        { stage: "journal", enabled: false, completed: false },
        { stage: "shutdown", enabled: false, completed: false },
      ],
      request_preview: {
        transport: "stdio",
        dispatch_gate: {
          dispatch_allowed: false,
          envelope_validation: {
            valid: true,
          },
          auth_gate: {
            token_checks_passed: true,
            reason: "capability_token_ready",
          },
        },
      },
      response_preview: {
        transport: "stdio",
        response_envelope: null,
        response_emitted: false,
      },
    });

    await expect(
      previewLocalMcpTransportAdapterLifecycle(
        {
          id: "",
          transport: "sse",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      lifecycle_enabled: false,
      first_blocking_stage: "configure",
      completed: false,
      request_preview: {
        dispatch_gate: {
          envelope_validation: {
            valid: false,
            reason: "request_id_required",
          },
        },
      },
      response_preview: {
        response_envelope: null,
        response_emitted: false,
      },
    });

    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "adapter-lifecycle-accept-token",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-raw-token-1",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      previewLocalMcpTransportAdapterLifecycle(
        {
          id: "adapter-lifecycle-preview-enabled",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      transport: "stdio",
      lifecycle_enabled: false,
      request_accepted: false,
      response_emitted: false,
      executed: false,
      journaled: false,
      completed: false,
      request_preview: {
        request_accepted: false,
        response_emitted: false,
        executed: false,
        dispatch_gate: {
          dispatch_allowed: true,
          transport_gate_enabled: true,
          blocking_gates: [],
          reason: "dispatch_allowed",
        },
      },
      response_preview: {
        response_envelope: null,
        response_emitted: false,
      },
    });
  });

  it("preflights planned local transports through the execution guard", () => {
    expect(preflightLocalMcpTransportRequest("stdio", "starlane.spaces.list")).toEqual({
      transport: "stdio",
      runtime_mode: "local_vault",
      transport_available: false,
      reason: "capability_token_not_enabled",
      error_code: "capability_token_required",
      capability_token: {
        valid: false,
        status: "disabled",
        reason: "capability_token_missing",
      },
      capability_token_auth_readiness: localMcpCapabilityTokenAuthReadiness(),
      operation_journal_readiness: localMcpOperationJournalReadiness("starlane.spaces.list"),
      transport_readiness: localMcpTransportReadiness("stdio", "starlane.spaces.list"),
      guard: {
        tool: "starlane.spaces.list",
        executable: true,
        runtime_mode: "local_vault",
        plan: {
          name: "starlane.spaces.list",
          availability: "executable",
          read_only: true,
          runtime_mode: "local_vault",
          requires_confirmation: false,
        },
      },
    });

    expect(
      preflightLocalMcpTransportRequest("sse", "starlane.documents.delete", {
        confirmation_text: "confirm local mcp starlane.documents.delete",
      }),
    ).toMatchObject({
      transport: "sse",
      runtime_mode: "local_vault",
      transport_available: false,
      reason: "capability_token_not_enabled",
      error_code: "capability_token_required",
      tool_error_code: "write_execution_disabled",
      capability_token: {
        valid: false,
        status: "disabled",
        reason: "capability_token_missing",
      },
      capability_token_auth_readiness: localMcpCapabilityTokenAuthReadiness(),
      operation_journal_readiness: localMcpOperationJournalReadiness("starlane.documents.delete"),
      transport_readiness: localMcpTransportReadiness("sse", "starlane.documents.delete"),
      guard: {
        tool: "starlane.documents.delete",
        executable: false,
        rejection_reason: "write_execution_disabled",
        confirmation_validation: {
          valid: true,
        },
      },
    });
  });

  it("keeps local MCP capability tokens disabled until transport auth is implemented", () => {
    expect(localMcpCapabilityTokenPolicy()).toEqual({
      status: "disabled",
      required_for_transports: ["stdio", "sse"],
      reason: "capability_token_not_enabled",
    });
    expect(validateLocalMcpCapabilityToken(undefined)).toEqual({
      valid: false,
      status: "disabled",
      reason: "capability_token_missing",
    });
    expect(validateLocalMcpCapabilityToken("candidate-token")).toEqual({
      valid: false,
      status: "disabled",
      reason: "capability_token_not_enabled",
    });
    expect(
      preflightLocalMcpTransportRequest("stdio", "starlane.spaces.list", {
        capability_token: "candidate-token",
      }),
    ).toMatchObject({
      transport_available: false,
      reason: "capability_token_not_enabled",
      capability_token: {
        valid: false,
        status: "disabled",
        reason: "capability_token_not_enabled",
      },
      capability_token_auth_readiness: localMcpCapabilityTokenAuthReadiness(),
      operation_journal_readiness: localMcpOperationJournalReadiness("starlane.spaces.list"),
      transport_readiness: localMcpTransportReadiness("stdio", "starlane.spaces.list"),
      guard: {
        executable: true,
      },
    });
  });

  it("preflights planned local transports with a non-authorizing token validation preview", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };
    const expectedFingerprint =
      "sha256:ccdf85d08ff73f41a75c7124fc02152cce978b49766f84f7fada3e2f1efa5d04";

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      preflightLocalMcpTransportRequestWithAuthPreview(
        "stdio",
        "starlane.spaces.list",
        {
          capability_token: "local-mcp-raw-token-1",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toEqual({
      preflight: preflightLocalMcpTransportRequest("stdio", "starlane.spaces.list", {
        capability_token: "local-mcp-raw-token-1",
      }),
      capability_token_validation_preview: {
        token_accepted: false,
        token_present: true,
        token_record_found: true,
        fingerprint_matched: true,
        expired: false,
        reason: "capability_token_not_enabled",
        token_fingerprint: expectedFingerprint,
        record: {
          token_id: "local-mcp-token:token-request-1",
          issued_at: "2026-07-13T13:00:00.000Z",
          expires_at: "2026-07-13T14:00:00.000Z",
          runtime_mode: "local_vault",
          transport: "stdio",
          scope: "notes:read",
          vault_fingerprint: "vault-sha256:abc123",
          token_fingerprint: expectedFingerprint,
          revoked_at: null,
          persisted: true,
          accepted: false,
          raw_token_stored: false,
          reason: "capability_token_issued",
        },
        expiry_preview: {
          token_accepted: false,
          runtime_mode: "local_vault",
          expired: false,
          reason: "capability_token_not_enabled",
          error_code: "capability_token_not_enabled",
          request_validation: {
            valid: false,
            token_accepted: false,
            expired: false,
            reason: "capability_token_not_enabled",
            error_code: "capability_token_not_enabled",
          },
          planned_record: {
            token_id: "local-mcp-token:token-request-1",
            issued_at: "2026-07-13T13:00:00.000Z",
            expires_at: "2026-07-13T14:00:00.000Z",
            checked_at: "2026-07-13T13:30:00.000Z",
            seconds_until_expiry: 1800,
            expired: false,
          },
        },
      },
      capability_token_auth_gate_preview: {
        runtime_mode: "local_vault",
        token_checks_passed: true,
        token_accepted: false,
        transport_dispatch_allowed: false,
        token_present: true,
        token_record_found: true,
        fingerprint_matched: true,
        expired: false,
        revoked: false,
        reason: "capability_token_ready",
        blocking_gates: [
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
        validation_preview: {
          token_accepted: false,
          token_present: true,
          token_record_found: true,
          fingerprint_matched: true,
          expired: false,
          reason: "capability_token_not_enabled",
          token_fingerprint: expectedFingerprint,
          record: {
            token_id: "local-mcp-token:token-request-1",
            issued_at: "2026-07-13T13:00:00.000Z",
            expires_at: "2026-07-13T14:00:00.000Z",
            runtime_mode: "local_vault",
            transport: "stdio",
            scope: "notes:read",
            vault_fingerprint: "vault-sha256:abc123",
            token_fingerprint: expectedFingerprint,
            revoked_at: null,
            persisted: true,
            accepted: false,
            raw_token_stored: false,
            reason: "capability_token_issued",
          },
          expiry_preview: {
            token_accepted: false,
            runtime_mode: "local_vault",
            expired: false,
            reason: "capability_token_not_enabled",
            error_code: "capability_token_not_enabled",
            request_validation: {
              valid: false,
              token_accepted: false,
              expired: false,
              reason: "capability_token_not_enabled",
              error_code: "capability_token_not_enabled",
            },
            planned_record: {
              token_id: "local-mcp-token:token-request-1",
              issued_at: "2026-07-13T13:00:00.000Z",
              expires_at: "2026-07-13T14:00:00.000Z",
              checked_at: "2026-07-13T13:30:00.000Z",
              seconds_until_expiry: 1800,
              expired: false,
            },
          },
        },
      },
      capability_token_acceptance_gate_preview: {
        runtime_mode: "local_vault",
        token_acceptance_enabled: false,
        token_checks_passed: true,
        token_accepted: false,
        acceptance_allowed: false,
        transport_dispatch_allowed: false,
        listener_binding_allowed: false,
        reason: "capability_token_acceptance_not_enabled",
        blocking_gates: [
          {
            gate: "token_acceptance",
            status: "disabled",
            reason: "capability_token_acceptance_not_enabled",
            error_code: "capability_token_not_enabled",
          },
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
        auth_gate_preview: {
          runtime_mode: "local_vault",
          token_checks_passed: true,
          token_accepted: false,
          transport_dispatch_allowed: false,
          token_present: true,
          token_record_found: true,
          fingerprint_matched: true,
          expired: false,
          revoked: false,
          reason: "capability_token_ready",
          blocking_gates: [
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
          validation_preview: {
            token_accepted: false,
            token_present: true,
            token_record_found: true,
            fingerprint_matched: true,
            expired: false,
            reason: "capability_token_not_enabled",
            token_fingerprint: expectedFingerprint,
            record: {
              token_id: "local-mcp-token:token-request-1",
              issued_at: "2026-07-13T13:00:00.000Z",
              expires_at: "2026-07-13T14:00:00.000Z",
              runtime_mode: "local_vault",
              transport: "stdio",
              scope: "notes:read",
              vault_fingerprint: "vault-sha256:abc123",
              token_fingerprint: expectedFingerprint,
              revoked_at: null,
              persisted: true,
              accepted: false,
              raw_token_stored: false,
              reason: "capability_token_issued",
            },
            expiry_preview: {
              token_accepted: false,
              runtime_mode: "local_vault",
              expired: false,
              reason: "capability_token_not_enabled",
              error_code: "capability_token_not_enabled",
              request_validation: {
                valid: false,
                token_accepted: false,
                expired: false,
                reason: "capability_token_not_enabled",
                error_code: "capability_token_not_enabled",
              },
              planned_record: {
                token_id: "local-mcp-token:token-request-1",
                issued_at: "2026-07-13T13:00:00.000Z",
                expires_at: "2026-07-13T14:00:00.000Z",
                checked_at: "2026-07-13T13:30:00.000Z",
                seconds_until_expiry: 1800,
                expired: false,
              },
            },
          },
        },
      },
    });
    const sharedAuthPreflight = await preflightLocalMcpTransportRequestWithAuthPreview(
      "stdio",
      "starlane.spaces.list",
      {
        capability_token: "local-mcp-raw-token-1",
      },
      "2026-07-13T13:30:00.000Z",
    );
    expect(sharedAuthPreflight.capability_token_acceptance_gate_preview.auth_gate_preview).toBe(
      sharedAuthPreflight.capability_token_auth_gate_preview,
    );
    await expect(
      preflightLocalMcpTransportRequestWithAuthPreview(
        "stdio",
        "starlane.spaces.list",
        {
          capability_token: "local-mcp-raw-token-1",
        },
        "2026-07-13T14:00:00.000Z",
      ),
    ).resolves.toMatchObject({
      preflight: {
        transport_available: false,
        capability_token: {
          valid: false,
          reason: "capability_token_not_enabled",
        },
      },
      capability_token_validation_preview: {
        token_accepted: false,
        token_record_found: true,
        fingerprint_matched: true,
        expired: true,
        reason: "capability_token_expired",
      },
      capability_token_auth_gate_preview: {
        token_checks_passed: false,
        token_accepted: false,
        transport_dispatch_allowed: false,
        expired: true,
        revoked: false,
        reason: "capability_token_expired",
        blocking_gates: [
          {
            gate: "capability_token",
            status: "blocked",
            reason: "capability_token_expired",
          },
          {
            gate: "transport",
            status: "disabled",
            reason: "transport_not_enabled",
          },
          {
            gate: "operation_journal",
            status: "planned",
            reason: "operation_journal_not_enabled",
          },
        ],
      },
    });
  });

  it("builds a disabled local MCP capability-token request challenge without issuing tokens", () => {
    expect(buildLocalMcpCapabilityTokenRequestChallenge()).toEqual({
      runtime_mode: "local_vault",
      status: "disabled",
      token_issued: false,
      required_for_transports: ["stdio", "sse"],
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
    });
  });

  it("exposes the planned local MCP capability-token lifecycle without issuing tokens", () => {
    expect(localMcpCapabilityTokenLifecyclePolicy()).toEqual({
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
    });
  });

  it("summarizes local MCP capability-token auth readiness without enabling tokens", () => {
    expect(localMcpCapabilityTokenAuthContract()).toEqual({
      runtime_mode: "local_vault",
      status: "disabled",
      transport_ready: false,
      token_accepted: false,
      token_issued: false,
      token_revoked: false,
      required_for_transports: ["stdio", "sse"],
      lifecycle_status: "planned",
      token_format: "opaque_bearer",
      fingerprint_algorithm: "sha256",
      token_ttl_seconds: 3600,
      expiry_required: true,
      revocation_supported: false,
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
    });

    expect(localMcpCapabilityTokenAuthReadiness()).toEqual({
      runtime_mode: "local_vault",
      status: "disabled",
      transport_ready: false,
      token_accepted: false,
      token_issued: false,
      token_revoked: false,
      required_for_transports: ["stdio", "sse"],
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
    });
  });

  it("validates planned local MCP capability-token requests without issuing tokens", () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    expect(validateLocalMcpCapabilityTokenRequest(request)).toEqual({
      valid: false,
      token_issued: false,
      reason: "capability_token_not_enabled",
      error_code: "capability_token_not_enabled",
    });
    expect(validateLocalMcpCapabilityTokenRequest({ ...request, request_id: " " })).toEqual({
      valid: false,
      token_issued: false,
      reason: "request_id_required",
      error_code: "invalid_request",
    });
    expect(validateLocalMcpCapabilityTokenRequest({ ...request, created_at: "not-a-date" })).toEqual({
      valid: false,
      token_issued: false,
      reason: "created_at_invalid",
      error_code: "invalid_request",
    });
    expect(
      validateLocalMcpCapabilityTokenRequest({
        ...request,
        transport: "websocket" as "stdio",
      }),
    ).toEqual({
      valid: false,
      token_issued: false,
      reason: "transport_unsupported",
      error_code: "invalid_request",
    });
    expect(
      validateLocalMcpCapabilityTokenRequest({
        ...request,
        scope: "notes:write" as "notes:read",
      }),
    ).toEqual({
      valid: false,
      token_issued: false,
      reason: "scope_unsupported",
      error_code: "invalid_request",
    });
    expect(
      validateLocalMcpCapabilityTokenRequest({
        ...request,
        confirmation_text: "confirm something else",
      }),
    ).toEqual({
      valid: false,
      token_issued: false,
      reason: "confirmation_mismatch",
      error_code: "invalid_request",
    });
  });

  it("previews planned local MCP capability-token issuance without creating tokens", () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    expect(previewLocalMcpCapabilityTokenIssuance(request)).toEqual({
      token_issued: false,
      runtime_mode: "local_vault",
      reason: "capability_token_not_enabled",
      error_code: "capability_token_not_enabled",
      request_validation: {
        valid: false,
        token_issued: false,
        reason: "capability_token_not_enabled",
        error_code: "capability_token_not_enabled",
      },
      planned_record: {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        runtime_mode: "local_vault",
        transport: "sse",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:abc123",
        token_fingerprint: null,
        revoked_at: null,
      },
    });

    expect(
      previewLocalMcpCapabilityTokenIssuance({
        ...request,
        confirmation_text: "wrong",
      }),
    ).toEqual({
      token_issued: false,
      runtime_mode: "local_vault",
      reason: "confirmation_mismatch",
      error_code: "invalid_request",
      request_validation: {
        valid: false,
        token_issued: false,
        reason: "confirmation_mismatch",
        error_code: "invalid_request",
      },
    });
  });

  it("persists planned local MCP capability-token records without storing raw tokens", () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    expect(listLocalMcpCapabilityTokenRecords()).toEqual([]);
    expect(persistLocalMcpCapabilityTokenRecord(request)).toEqual({
      record_persisted: true,
      record: {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        runtime_mode: "local_vault",
        transport: "sse",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:abc123",
        token_fingerprint: null,
        revoked_at: null,
        persisted: true,
        accepted: false,
        raw_token_stored: false,
        reason: "capability_token_not_enabled",
      },
      records: [
        {
          token_id: "local-mcp-token:token-request-1",
          issued_at: "2026-07-13T13:00:00.000Z",
          expires_at: "2026-07-13T14:00:00.000Z",
          runtime_mode: "local_vault",
          transport: "sse",
          scope: "notes:read",
          vault_fingerprint: "vault-sha256:abc123",
          token_fingerprint: null,
          revoked_at: null,
          persisted: true,
          accepted: false,
          raw_token_stored: false,
          reason: "capability_token_not_enabled",
        },
      ],
    });
    expect(listLocalMcpCapabilityTokenRecords()).toEqual([
      {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        runtime_mode: "local_vault",
        transport: "sse",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:abc123",
        token_fingerprint: null,
        revoked_at: null,
        persisted: true,
        accepted: false,
        raw_token_stored: false,
        reason: "capability_token_not_enabled",
      },
    ]);

    expect(validateLocalMcpCapabilityToken("raw-token")).toEqual({
      valid: false,
      status: "disabled",
      reason: "capability_token_not_enabled",
    });
  });

  it("replaces planned local MCP capability-token records by token id", () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    persistLocalMcpCapabilityTokenRecord(request);
    const result = persistLocalMcpCapabilityTokenRecord({
      ...request,
      created_at: "2026-07-13T13:15:00.000Z",
      vault_fingerprint: "vault-sha256:def456",
    });

    expect(result.record_persisted).toBe(true);
    expect(listLocalMcpCapabilityTokenRecords()).toEqual([
      {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:15:00.000Z",
        expires_at: "2026-07-13T14:15:00.000Z",
        runtime_mode: "local_vault",
        transport: "stdio",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:def456",
        token_fingerprint: null,
        revoked_at: null,
        persisted: true,
        accepted: false,
        raw_token_stored: false,
        reason: "capability_token_not_enabled",
      },
    ]);
  });

  it("does not persist invalid local MCP capability-token records", () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "wrong",
    };

    expect(persistLocalMcpCapabilityTokenRecord(request)).toEqual({
      record_persisted: false,
      reason: "confirmation_mismatch",
      records: [],
    });
    expect(listLocalMcpCapabilityTokenRecords()).toEqual([]);

    persistLocalMcpCapabilityTokenRecord({
      ...request,
      confirmation_text: "confirm local mcp capability token",
    });
    clearLocalMcpCapabilityTokenRecords();
    expect(listLocalMcpCapabilityTokenRecords()).toEqual([]);
  });

  it("issues local MCP capability tokens once while storing only fingerprints", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };
    const expectedFingerprint =
      "sha256:ccdf85d08ff73f41a75c7124fc02152cce978b49766f84f7fada3e2f1efa5d04";

    await expect(
      issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1"),
    ).resolves.toEqual({
      token_issued: true,
      raw_token: "local-mcp-raw-token-1",
      record: {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        runtime_mode: "local_vault",
        transport: "sse",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:abc123",
        token_fingerprint: expectedFingerprint,
        revoked_at: null,
        persisted: true,
        accepted: false,
        raw_token_stored: false,
        reason: "capability_token_issued",
      },
      records: [
        {
          token_id: "local-mcp-token:token-request-1",
          issued_at: "2026-07-13T13:00:00.000Z",
          expires_at: "2026-07-13T14:00:00.000Z",
          runtime_mode: "local_vault",
          transport: "sse",
          scope: "notes:read",
          vault_fingerprint: "vault-sha256:abc123",
          token_fingerprint: expectedFingerprint,
          revoked_at: null,
          persisted: true,
          accepted: false,
          raw_token_stored: false,
          reason: "capability_token_issued",
        },
      ],
    });
    expect(JSON.stringify(listLocalMcpCapabilityTokenRecords())).not.toContain(
      "local-mcp-raw-token-1",
    );
    expect(validateLocalMcpCapabilityToken("local-mcp-raw-token-1")).toEqual({
      valid: false,
      status: "disabled",
      reason: "capability_token_not_enabled",
    });
  });

  it("does not issue local MCP capability tokens for invalid requests", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "wrong",
    };

    await expect(
      issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1"),
    ).resolves.toEqual({
      token_issued: false,
      reason: "confirmation_mismatch",
      records: [],
    });
    expect(listLocalMcpCapabilityTokenRecords()).toEqual([]);
  });

  it("previews local MCP capability-token validation without accepting tokens", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };
    const expectedFingerprint =
      "sha256:ccdf85d08ff73f41a75c7124fc02152cce978b49766f84f7fada3e2f1efa5d04";

    await expect(
      previewLocalMcpCapabilityTokenValidation(undefined, "2026-07-13T13:30:00.000Z"),
    ).resolves.toEqual({
      token_accepted: false,
      token_present: false,
      token_record_found: false,
      fingerprint_matched: false,
      expired: false,
      reason: "capability_token_missing",
    });
    await expect(
      previewLocalMcpCapabilityTokenValidation(
        "local-mcp-raw-token-1",
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toEqual({
      token_accepted: false,
      token_present: true,
      token_record_found: false,
      fingerprint_matched: false,
      expired: false,
      reason: "token_record_not_found",
      token_fingerprint: expectedFingerprint,
    });

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpCapabilityTokenValidation(
        "local-mcp-wrong-token",
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      token_accepted: false,
      token_present: true,
      token_record_found: false,
      fingerprint_matched: false,
      expired: false,
      reason: "token_fingerprint_mismatch",
    });
    await expect(
      previewLocalMcpCapabilityTokenValidation(
        "local-mcp-raw-token-1",
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toEqual({
      token_accepted: false,
      token_present: true,
      token_record_found: true,
      fingerprint_matched: true,
      expired: false,
      reason: "capability_token_not_enabled",
      token_fingerprint: expectedFingerprint,
      record: {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        runtime_mode: "local_vault",
        transport: "sse",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:abc123",
        token_fingerprint: expectedFingerprint,
        revoked_at: null,
        persisted: true,
        accepted: false,
        raw_token_stored: false,
        reason: "capability_token_issued",
      },
      expiry_preview: {
        token_accepted: false,
        runtime_mode: "local_vault",
        expired: false,
        reason: "capability_token_not_enabled",
        error_code: "capability_token_not_enabled",
        request_validation: {
          valid: false,
          token_accepted: false,
          expired: false,
          reason: "capability_token_not_enabled",
          error_code: "capability_token_not_enabled",
        },
        planned_record: {
          token_id: "local-mcp-token:token-request-1",
          issued_at: "2026-07-13T13:00:00.000Z",
          expires_at: "2026-07-13T14:00:00.000Z",
          checked_at: "2026-07-13T13:30:00.000Z",
          seconds_until_expiry: 1800,
          expired: false,
        },
      },
    });
    await expect(
      previewLocalMcpCapabilityTokenValidation(
        "local-mcp-raw-token-1",
        "2026-07-13T14:00:00.000Z",
      ),
    ).resolves.toMatchObject({
      token_accepted: false,
      token_record_found: true,
      fingerprint_matched: true,
      expired: true,
      reason: "capability_token_expired",
    });
  });

  it("previews local MCP capability-token auth gate without enabling dispatch", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };
    const expectedFingerprint =
      "sha256:ccdf85d08ff73f41a75c7124fc02152cce978b49766f84f7fada3e2f1efa5d04";

    await expect(
      previewLocalMcpCapabilityTokenAuthGate(undefined, "2026-07-13T13:30:00.000Z"),
    ).resolves.toMatchObject({
      token_checks_passed: false,
      token_accepted: false,
      transport_dispatch_allowed: false,
      token_present: false,
      reason: "capability_token_missing",
      blocking_gates: [
        {
          gate: "capability_token",
          status: "blocked",
          reason: "capability_token_missing",
          error_code: "capability_token_required",
        },
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
    });

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpCapabilityTokenAuthGate(
        "local-mcp-raw-token-1",
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      runtime_mode: "local_vault",
      token_checks_passed: true,
      token_accepted: false,
      transport_dispatch_allowed: false,
      token_present: true,
      token_record_found: true,
      fingerprint_matched: true,
      expired: false,
      revoked: false,
      reason: "capability_token_ready",
      blocking_gates: [
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
      validation_preview: {
        token_fingerprint: expectedFingerprint,
        reason: "capability_token_not_enabled",
      },
    });

    await expect(
      previewLocalMcpCapabilityTokenAuthGate(
        "local-mcp-raw-token-1",
        "2026-07-13T14:00:00.000Z",
      ),
    ).resolves.toMatchObject({
      token_checks_passed: false,
      expired: true,
      revoked: false,
      reason: "capability_token_expired",
      blocking_gates: [
        {
          gate: "capability_token",
          status: "blocked",
          reason: "capability_token_expired",
          error_code: "capability_token_not_enabled",
        },
        {
          gate: "transport",
          status: "disabled",
          reason: "transport_not_enabled",
        },
        {
          gate: "operation_journal",
          status: "planned",
          reason: "operation_journal_not_enabled",
        },
      ],
    });

    revokeLocalMcpCapabilityTokenRecord({
      token_id: "local-mcp-token:token-request-1",
      requested_at: "2026-07-13T13:45:00.000Z",
      token_fingerprint: expectedFingerprint,
      confirmation_text: "confirm local mcp capability token revocation",
    });

    await expect(
      previewLocalMcpCapabilityTokenAuthGate(
        "local-mcp-raw-token-1",
        "2026-07-13T13:50:00.000Z",
      ),
    ).resolves.toMatchObject({
      token_checks_passed: false,
      expired: false,
      revoked: true,
      reason: "capability_token_revoked",
      blocking_gates: [
        {
          gate: "capability_token",
          status: "blocked",
          reason: "capability_token_revoked",
          error_code: "capability_token_not_enabled",
        },
        {
          gate: "transport",
          status: "disabled",
          reason: "transport_not_enabled",
        },
        {
          gate: "operation_journal",
          status: "planned",
          reason: "operation_journal_not_enabled",
        },
      ],
    });
  });

  it("previews local MCP capability-token acceptance gate without accepting tokens", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await expect(
      previewLocalMcpCapabilityTokenAcceptanceGate(undefined, "2026-07-13T13:30:00.000Z"),
    ).resolves.toMatchObject({
      runtime_mode: "local_vault",
      token_acceptance_enabled: false,
      token_checks_passed: false,
      token_accepted: false,
      acceptance_allowed: false,
      transport_dispatch_allowed: false,
      listener_binding_allowed: false,
      reason: "capability_token_missing",
      blocking_gates: [
        {
          gate: "capability_token",
          status: "blocked",
          reason: "capability_token_missing",
          error_code: "capability_token_required",
        },
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
    });

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpCapabilityTokenAcceptanceGate(
        "local-mcp-raw-token-1",
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      runtime_mode: "local_vault",
      token_acceptance_enabled: false,
      token_checks_passed: true,
      token_accepted: false,
      acceptance_allowed: false,
      transport_dispatch_allowed: false,
      listener_binding_allowed: false,
      reason: "capability_token_acceptance_not_enabled",
      blocking_gates: [
        {
          gate: "token_acceptance",
          status: "disabled",
          reason: "capability_token_acceptance_not_enabled",
          error_code: "capability_token_not_enabled",
        },
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
      auth_gate_preview: {
        token_checks_passed: true,
        token_accepted: false,
        reason: "capability_token_ready",
      },
    });
  });

  it("persists local MCP capability-token revocation without accepting tokens", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };
    const expectedFingerprint =
      "sha256:ccdf85d08ff73f41a75c7124fc02152cce978b49766f84f7fada3e2f1efa5d04";

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    expect(
      revokeLocalMcpCapabilityTokenRecord({
        token_id: "local-mcp-token:missing",
        requested_at: "2026-07-13T13:45:00.000Z",
        token_fingerprint: expectedFingerprint,
        confirmation_text: "confirm local mcp capability token revocation",
      }),
    ).toMatchObject({
      token_revoked: false,
      reason: "token_record_not_found",
    });
    expect(
      revokeLocalMcpCapabilityTokenRecord({
        token_id: "local-mcp-token:token-request-1",
        requested_at: "2026-07-13T13:45:00.000Z",
        token_fingerprint: "sha256:wrong",
        confirmation_text: "confirm local mcp capability token revocation",
      }),
    ).toMatchObject({
      token_revoked: false,
      reason: "token_fingerprint_mismatch",
    });

    expect(
      revokeLocalMcpCapabilityTokenRecord({
        token_id: "local-mcp-token:token-request-1",
        requested_at: "2026-07-13T13:45:00.000Z",
        token_fingerprint: expectedFingerprint,
        confirmation_text: "confirm local mcp capability token revocation",
      }),
    ).toEqual({
      token_revoked: true,
      record: {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        runtime_mode: "local_vault",
        transport: "stdio",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:abc123",
        token_fingerprint: expectedFingerprint,
        revoked_at: "2026-07-13T13:45:00.000Z",
        persisted: true,
        accepted: false,
        raw_token_stored: false,
        reason: "capability_token_revoked",
      },
      records: [
        {
          token_id: "local-mcp-token:token-request-1",
          issued_at: "2026-07-13T13:00:00.000Z",
          expires_at: "2026-07-13T14:00:00.000Z",
          runtime_mode: "local_vault",
          transport: "stdio",
          scope: "notes:read",
          vault_fingerprint: "vault-sha256:abc123",
          token_fingerprint: expectedFingerprint,
          revoked_at: "2026-07-13T13:45:00.000Z",
          persisted: true,
          accepted: false,
          raw_token_stored: false,
          reason: "capability_token_revoked",
        },
      ],
    });
    await expect(
      previewLocalMcpCapabilityTokenValidation(
        "local-mcp-raw-token-1",
        "2026-07-13T13:50:00.000Z",
      ),
    ).resolves.toMatchObject({
      token_accepted: false,
      token_record_found: true,
      fingerprint_matched: true,
      expired: false,
      reason: "capability_token_revoked",
      record: {
        revoked_at: "2026-07-13T13:45:00.000Z",
        reason: "capability_token_revoked",
      },
    });
  });

  it("previews persisted local MCP capability-token record expiry without accepting tokens", () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    persistLocalMcpCapabilityTokenRecord(request);

    expect(findLocalMcpCapabilityTokenRecord("local-mcp-token:token-request-1")).toEqual({
      token_id: "local-mcp-token:token-request-1",
      issued_at: "2026-07-13T13:00:00.000Z",
      expires_at: "2026-07-13T14:00:00.000Z",
      runtime_mode: "local_vault",
      transport: "sse",
      scope: "notes:read",
      vault_fingerprint: "vault-sha256:abc123",
      token_fingerprint: null,
      revoked_at: null,
      persisted: true,
      accepted: false,
      raw_token_stored: false,
      reason: "capability_token_not_enabled",
    });
    expect(
      previewLocalMcpCapabilityTokenRecordExpiry(
        "local-mcp-token:token-request-1",
        "2026-07-13T13:30:00.000Z",
      ),
    ).toEqual({
      token_record_found: true,
      record: {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        runtime_mode: "local_vault",
        transport: "sse",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:abc123",
        token_fingerprint: null,
        revoked_at: null,
        persisted: true,
        accepted: false,
        raw_token_stored: false,
        reason: "capability_token_not_enabled",
      },
      preview: {
        token_accepted: false,
        runtime_mode: "local_vault",
        expired: false,
        reason: "capability_token_not_enabled",
        error_code: "capability_token_not_enabled",
        request_validation: {
          valid: false,
          token_accepted: false,
          expired: false,
          reason: "capability_token_not_enabled",
          error_code: "capability_token_not_enabled",
        },
        planned_record: {
          token_id: "local-mcp-token:token-request-1",
          issued_at: "2026-07-13T13:00:00.000Z",
          expires_at: "2026-07-13T14:00:00.000Z",
          checked_at: "2026-07-13T13:30:00.000Z",
          seconds_until_expiry: 1800,
          expired: false,
        },
      },
    });
    expect(
      previewLocalMcpCapabilityTokenRecordExpiry(
        "local-mcp-token:missing",
        "2026-07-13T13:30:00.000Z",
      ),
    ).toEqual({
      token_record_found: false,
      reason: "token_record_not_found",
    });
  });

  it("previews persisted local MCP capability-token record revocation without revoking tokens", () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    persistLocalMcpCapabilityTokenRecord(request);

    expect(
      previewLocalMcpCapabilityTokenRecordRevocation({
        token_id: "local-mcp-token:token-request-1",
        requested_at: "2026-07-13T13:30:00.000Z",
        token_fingerprint: "sha256:abc123",
        confirmation_text: "confirm local mcp capability token revocation",
      }),
    ).toEqual({
      token_record_found: true,
      record: {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        runtime_mode: "local_vault",
        transport: "stdio",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:abc123",
        token_fingerprint: null,
        revoked_at: null,
        persisted: true,
        accepted: false,
        raw_token_stored: false,
        reason: "capability_token_not_enabled",
      },
      preview: {
        token_revoked: false,
        runtime_mode: "local_vault",
        reason: "capability_token_not_enabled",
        error_code: "capability_token_not_enabled",
        request_validation: {
          valid: false,
          token_revoked: false,
          reason: "capability_token_not_enabled",
          error_code: "capability_token_not_enabled",
        },
        planned_record: {
          token_id: "local-mcp-token:token-request-1",
          token_fingerprint: "sha256:abc123",
          revoked_at: "2026-07-13T13:30:00.000Z",
          revocation_supported: false,
        },
      },
    });
    expect(
      previewLocalMcpCapabilityTokenRecordRevocation({
        token_id: "local-mcp-token:missing",
        requested_at: "2026-07-13T13:30:00.000Z",
        token_fingerprint: "sha256:abc123",
        confirmation_text: "confirm local mcp capability token revocation",
      }),
    ).toEqual({
      token_record_found: false,
      reason: "token_record_not_found",
    });
  });

  it("previews planned local MCP capability-token revocation without revoking tokens", () => {
    const request = {
      token_id: "local-mcp-token:token-request-1",
      requested_at: "2026-07-13T13:30:00.000Z",
      token_fingerprint: "sha256:abc123",
      confirmation_text: "confirm local mcp capability token revocation",
    };

    expect(validateLocalMcpCapabilityTokenRevocationRequest(request)).toEqual({
      valid: false,
      token_revoked: false,
      reason: "capability_token_not_enabled",
      error_code: "capability_token_not_enabled",
    });
    expect(previewLocalMcpCapabilityTokenRevocation(request)).toEqual({
      token_revoked: false,
      runtime_mode: "local_vault",
      reason: "capability_token_not_enabled",
      error_code: "capability_token_not_enabled",
      request_validation: {
        valid: false,
        token_revoked: false,
        reason: "capability_token_not_enabled",
        error_code: "capability_token_not_enabled",
      },
      planned_record: {
        token_id: "local-mcp-token:token-request-1",
        token_fingerprint: "sha256:abc123",
        revoked_at: "2026-07-13T13:30:00.000Z",
        revocation_supported: false,
      },
    });

    expect(validateLocalMcpCapabilityTokenRevocationRequest({ ...request, token_id: " " })).toEqual({
      valid: false,
      token_revoked: false,
      reason: "token_id_required",
      error_code: "invalid_request",
    });
    expect(
      validateLocalMcpCapabilityTokenRevocationRequest({
        ...request,
        requested_at: "not-a-date",
      }),
    ).toEqual({
      valid: false,
      token_revoked: false,
      reason: "requested_at_invalid",
      error_code: "invalid_request",
    });
    expect(
      previewLocalMcpCapabilityTokenRevocation({
        ...request,
        confirmation_text: "wrong",
      }),
    ).toEqual({
      token_revoked: false,
      runtime_mode: "local_vault",
      reason: "confirmation_mismatch",
      error_code: "invalid_request",
      request_validation: {
        valid: false,
        token_revoked: false,
        reason: "confirmation_mismatch",
        error_code: "invalid_request",
      },
    });
  });

  it("previews planned local MCP capability-token expiry without accepting tokens", () => {
    const request = {
      token_id: "local-mcp-token:token-request-1",
      issued_at: "2026-07-13T13:00:00.000Z",
      expires_at: "2026-07-13T14:00:00.000Z",
      checked_at: "2026-07-13T13:30:00.000Z",
    };

    expect(validateLocalMcpCapabilityTokenExpiry(request)).toEqual({
      valid: false,
      token_accepted: false,
      expired: false,
      reason: "capability_token_not_enabled",
      error_code: "capability_token_not_enabled",
    });
    expect(previewLocalMcpCapabilityTokenExpiry(request)).toEqual({
      token_accepted: false,
      runtime_mode: "local_vault",
      expired: false,
      reason: "capability_token_not_enabled",
      error_code: "capability_token_not_enabled",
      request_validation: {
        valid: false,
        token_accepted: false,
        expired: false,
        reason: "capability_token_not_enabled",
        error_code: "capability_token_not_enabled",
      },
      planned_record: {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        checked_at: "2026-07-13T13:30:00.000Z",
        seconds_until_expiry: 1800,
        expired: false,
      },
    });
    expect(
      previewLocalMcpCapabilityTokenExpiry({
        ...request,
        checked_at: "2026-07-13T14:00:00.000Z",
      }),
    ).toEqual({
      token_accepted: false,
      runtime_mode: "local_vault",
      expired: true,
      reason: "capability_token_expired",
      error_code: "capability_token_not_enabled",
      request_validation: {
        valid: false,
        token_accepted: false,
        expired: true,
        reason: "capability_token_expired",
        error_code: "capability_token_not_enabled",
      },
      planned_record: {
        token_id: "local-mcp-token:token-request-1",
        issued_at: "2026-07-13T13:00:00.000Z",
        expires_at: "2026-07-13T14:00:00.000Z",
        checked_at: "2026-07-13T14:00:00.000Z",
        seconds_until_expiry: 0,
        expired: true,
      },
    });
    expect(validateLocalMcpCapabilityTokenExpiry({ ...request, expires_at: "not-a-date" })).toEqual({
      valid: false,
      token_accepted: false,
      expired: false,
      reason: "expires_at_invalid",
      error_code: "invalid_request",
    });
    expect(
      validateLocalMcpCapabilityTokenExpiry({
        ...request,
        expires_at: "2026-07-13T12:59:59.000Z",
      }),
    ).toEqual({
      valid: false,
      token_accepted: false,
      expired: false,
      reason: "expiry_before_issue",
      error_code: "invalid_request",
    });
  });

  it("previews the planned local operation journal without recording entries", () => {
    expect(localMcpOperationJournalContract()).toEqual({
      runtime_mode: "local_vault",
      status: "planned",
      journal_enabled: false,
      write_journal_ready: false,
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
      blocking_gate: {
        gate: "operation_journal",
        status: "planned",
        reason: "operation_journal_not_enabled",
        error_code: "transport_disabled",
      },
    });

    expect(localMcpOperationJournalRecordContract()).toEqual({
      runtime_mode: "local_vault",
      status: "planned",
      journal_enabled: false,
      record_written: false,
      record_storage: "local_vault_metadata",
      required_for_operations: ["write"],
      id_strategy: "local-mcp-journal:<request_id>",
      timestamp_source: "transport_request",
      raw_confirmation_text_stored: false,
      raw_capability_token_stored: false,
      token_fingerprint_algorithm: "sha256",
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
      reason: "operation_journal_not_enabled",
      error_code: "transport_disabled",
    });

    expect(localMcpOperationJournalPolicy()).toEqual({
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
    });
    expect(previewLocalMcpOperationJournal("starlane.documents.delete")).toEqual({
      journal_enabled: false,
      reason: "operation_journal_not_enabled",
      runtime_mode: "local_vault",
      tool: "starlane.documents.delete",
      operation: "write",
      required: true,
      planned_fields: localMcpOperationJournalPolicy().planned_fields,
    });
    expect(previewLocalMcpOperationJournal("starlane.spaces.list")).toEqual({
      journal_enabled: false,
      reason: "journal_not_required",
      runtime_mode: "local_vault",
      tool: "starlane.spaces.list",
      operation: "read",
      required: false,
      planned_fields: localMcpOperationJournalPolicy().planned_fields,
    });
    expect(previewLocalMcpOperationJournal("starlane.unknown")).toEqual({
      journal_enabled: false,
      reason: "journal_not_required",
      runtime_mode: "local_vault",
      tool: "starlane.unknown",
      operation: "unknown",
      required: false,
      planned_fields: localMcpOperationJournalPolicy().planned_fields,
    });
    expect(localMcpOperationJournalReadiness("starlane.documents.delete")).toEqual({
      runtime_mode: "local_vault",
      status: "planned",
      journal_enabled: false,
      write_journal_ready: false,
      required_for_writes: true,
      policy: localMcpOperationJournalPolicy(),
      preview: previewLocalMcpOperationJournal("starlane.documents.delete"),
      blocking_gates: [
        {
          gate: "operation_journal",
          status: "planned",
          reason: "operation_journal_not_enabled",
          error_code: "transport_disabled",
        },
      ],
    });
  });

  it("persists local MCP operation journal records for blocked write attempts", async () => {
    expect(listLocalMcpOperationJournalRecords()).toEqual([]);

    const result = await appendLocalMcpOperationJournalRecord({
      request_id: "request-1",
      timestamp: "2026-07-13T16:15:00.000Z",
      transport: "stdio",
      tool: "starlane.documents.delete",
      target: "doc-1",
      confirmation_text: "DELETE starlane.documents.delete",
      capability_token: "raw-token-value",
    });

    expect(result.record_written).toBe(true);
    expect(result.record).toEqual({
      id: "local-mcp-journal:request-1",
      timestamp: "2026-07-13T16:15:00.000Z",
      runtime_mode: "local_vault",
      transport: "stdio",
      tool: "starlane.documents.delete",
      operation: "write",
      target: "doc-1",
      confirmation_text_present: true,
      capability_token_fingerprint: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      raw_confirmation_text_stored: false,
      raw_capability_token_stored: false,
      executed: false,
      persisted: true,
      reason: "write_execution_disabled",
    });
    expect(JSON.stringify(listLocalMcpOperationJournalRecords())).not.toContain(
      "raw-token-value",
    );
    expect(JSON.stringify(listLocalMcpOperationJournalRecords())).not.toContain(
      "DELETE starlane.documents.delete",
    );

    expect(previewLocalMcpOperationJournalReplay("request-1")).toMatchObject({
      runtime_mode: "local_vault",
      request_id_present: true,
      idempotency_key: "local-mcp-journal:request-1",
      replay_detected: true,
      record_write_allowed: false,
      reason: "journal_replay_detected",
      error_code: "transport_disabled",
      existing_record: {
        id: "local-mcp-journal:request-1",
        timestamp: "2026-07-13T16:15:00.000Z",
        transport: "stdio",
        target: "doc-1",
      },
    });

    const replay = await appendLocalMcpOperationJournalRecord({
      request_id: "request-1",
      timestamp: "2026-07-13T16:16:00.000Z",
      transport: "sse",
      tool: "starlane.documents.delete",
      target: "doc-2",
    });

    expect(replay).toMatchObject({
      record_written: false,
      reason: "journal_replay_detected",
      record: {
        id: "local-mcp-journal:request-1",
        timestamp: "2026-07-13T16:15:00.000Z",
        transport: "stdio",
        target: "doc-1",
      },
      operation_journal_replay_preview: {
        replay_detected: true,
        record_write_allowed: false,
        reason: "journal_replay_detected",
      },
    });
    expect(replay.records).toHaveLength(1);
    expect(listLocalMcpOperationJournalRecords()).toEqual([
      {
        id: "local-mcp-journal:request-1",
        timestamp: "2026-07-13T16:15:00.000Z",
        runtime_mode: "local_vault",
        transport: "stdio",
        tool: "starlane.documents.delete",
        operation: "write",
        target: "doc-1",
        confirmation_text_present: true,
        capability_token_fingerprint: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
        raw_confirmation_text_stored: false,
        raw_capability_token_stored: false,
        executed: false,
        persisted: true,
        reason: "write_execution_disabled",
      },
    ]);

    clearLocalMcpOperationJournalRecords();
    expect(listLocalMcpOperationJournalRecords()).toEqual([]);
    expect(previewLocalMcpOperationJournalReplay(" ")).toMatchObject({
      request_id_present: false,
      idempotency_key: null,
      replay_detected: false,
      record_write_allowed: false,
      reason: "request_id_required",
      error_code: "invalid_request",
    });
    expect(previewLocalMcpOperationJournalReplay("request-1")).toMatchObject({
      request_id_present: true,
      idempotency_key: "local-mcp-journal:request-1",
      replay_detected: false,
      record_write_allowed: true,
      reason: "journal_replay_not_found",
    });
  });

  it("does not persist local MCP operation journal records for reads or invalid metadata", async () => {
    await expect(
      appendLocalMcpOperationJournalRecord({
        request_id: "request-read",
        timestamp: "2026-07-13T16:15:00.000Z",
        transport: "stdio",
        tool: "starlane.spaces.list",
      }),
    ).resolves.toEqual({
      record_written: false,
      reason: "journal_not_required",
      records: [],
      operation_journal_replay_preview: {
        runtime_mode: "local_vault",
        request_id_present: true,
        idempotency_key: "local-mcp-journal:request-read",
        replay_detected: false,
        record_write_allowed: true,
        existing_record: undefined,
        reason: "journal_replay_not_found",
        error_code: "transport_disabled",
      },
    });

    await expect(
      appendLocalMcpOperationJournalRecord({
        request_id: " ",
        timestamp: "2026-07-13T16:15:00.000Z",
        transport: "stdio",
        tool: "starlane.documents.delete",
      }),
    ).resolves.toEqual({
      record_written: false,
      reason: "request_id_required",
      records: [],
    });

    await expect(
      appendLocalMcpOperationJournalRecord({
        request_id: "request-write",
        timestamp: " ",
        transport: "stdio",
        tool: "starlane.documents.delete",
      }),
    ).resolves.toEqual({
      record_written: false,
      reason: "timestamp_required",
      records: [],
    });
    expect(listLocalMcpOperationJournalRecords()).toEqual([]);
  });

  it("previews confirmed write journal gates without writing records", () => {
    expect(
      previewLocalMcpConfirmedWriteJournalGate({
        id: "journal-gate-write",
        transport: "stdio",
        tool: "starlane.documents.delete",
        request: {
          input: { document_id: "doc-1" },
          confirmation_text: "confirm local mcp starlane.documents.delete",
          capability_token: "candidate-token",
        },
      }),
    ).toMatchObject({
      runtime_mode: "local_vault",
      transport: "stdio",
      tool: "starlane.documents.delete",
      operation: "write",
      confirmation_valid: true,
      journal_required: true,
      journal_enabled: false,
      journal_write_allowed: false,
      would_record_journal: true,
      record_written: false,
      executed: false,
      reason: "operation_journal_not_enabled",
      error_code: "transport_disabled",
      blocking_gate: {
        gate: "operation_journal",
        status: "planned",
        reason: "operation_journal_not_enabled",
        error_code: "transport_disabled",
      },
      guard: {
        executable: false,
        rejection_reason: "write_execution_disabled",
        confirmation_validation: {
          valid: true,
        },
      },
    });

    expect(
      previewLocalMcpConfirmedWriteJournalGate({
        id: "journal-gate-write-unconfirmed",
        transport: "sse",
        tool: "starlane.documents.delete",
        request: {
          input: { document_id: "doc-1" },
          confirmation_text: "wrong",
        },
      }),
    ).toMatchObject({
      transport: "sse",
      tool: "starlane.documents.delete",
      operation: "write",
      confirmation_valid: false,
      journal_required: true,
      would_record_journal: false,
      record_written: false,
      reason: "confirmation_required",
      blocking_gate: undefined,
      guard: {
        confirmation_validation: {
          valid: false,
          reason: "confirmation_mismatch",
        },
      },
    });

    expect(
      previewLocalMcpConfirmedWriteJournalGate({
        id: "journal-gate-read",
        transport: "stdio",
        tool: "starlane.spaces.list",
      }),
    ).toMatchObject({
      tool: "starlane.spaces.list",
      operation: "read",
      confirmation_valid: false,
      journal_required: false,
      would_record_journal: false,
      record_written: false,
      reason: "journal_not_required",
      blocking_gate: undefined,
    });
    expect(listLocalMcpOperationJournalRecords()).toEqual([]);
  });

  it("journals confirmed write attempts during disabled local transport preflight", async () => {
    const result = await preflightLocalMcpTransportRequestWithJournal(
      {
        id: "journal-preflight-1",
        transport: "stdio",
        tool: "starlane.documents.delete",
        request: {
          input: { document_id: "doc-1" },
          confirmation_text: "confirm local mcp starlane.documents.delete",
          capability_token: "candidate-token",
        },
      },
      "2026-07-13T16:30:00.000Z",
    );

    expect(result.preflight).toMatchObject({
      transport: "stdio",
      runtime_mode: "local_vault",
      transport_available: false,
      reason: "capability_token_not_enabled",
      error_code: "transport_disabled",
      tool_error_code: "write_execution_disabled",
    });
    expect(result.dispatch_gate).toMatchObject({
      dispatch_allowed: false,
      executed: false,
      transport: "stdio",
      tool: "starlane.documents.delete",
      blocking_gates: [
        {
          gate: "capability_token",
          status: "blocked",
          reason: "token_record_not_found",
          error_code: "capability_token_not_enabled",
        },
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
        {
          gate: "write_execution",
          status: "disabled",
          reason: "write_execution_disabled",
          error_code: "write_execution_disabled",
        },
      ],
    });
    expect(result.operation_journal_gate_preview).toMatchObject({
      journal_required: true,
      confirmation_valid: true,
      journal_write_allowed: false,
      would_record_journal: true,
      record_written: false,
      reason: "operation_journal_not_enabled",
      blocking_gate: {
        gate: "operation_journal",
        reason: "operation_journal_not_enabled",
      },
    });
    expect(result.operation_journal_record.record_written).toBe(true);
    expect(result.operation_journal_record.record).toEqual({
      id: "local-mcp-journal:journal-preflight-1",
      timestamp: "2026-07-13T16:30:00.000Z",
      runtime_mode: "local_vault",
      transport: "stdio",
      tool: "starlane.documents.delete",
      operation: "write",
      target: "doc-1",
      confirmation_text_present: true,
      capability_token_fingerprint: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      raw_confirmation_text_stored: false,
      raw_capability_token_stored: false,
      executed: false,
      persisted: true,
      reason: "write_execution_disabled",
    });
    expect(result.confirmed_write_closeout).toMatchObject({
      transport: "stdio",
      tool: "starlane.documents.delete",
      journal_required: true,
      confirmation_valid: true,
      journal_recorded: true,
      replay_detected: false,
      record_available: true,
      execution_allowed: false,
      executed: false,
      closeout_status: "journal_recorded",
      reason: "write_execution_disabled",
      error_code: "write_execution_disabled",
      dispatch_blocking_gate: {
        gate: "capability_token",
        reason: "token_record_not_found",
      },
      idempotency_key: "local-mcp-journal:journal-preflight-1",
      record_id: "local-mcp-journal:journal-preflight-1",
    });
    const replay = await preflightLocalMcpTransportRequestWithJournal(
      {
        id: "journal-preflight-1",
        transport: "stdio",
        tool: "starlane.documents.delete",
        request: {
          input: { document_id: "doc-1" },
          confirmation_text: "confirm local mcp starlane.documents.delete",
          capability_token: "candidate-token",
        },
      },
      "2026-07-13T16:30:30.000Z",
    );
    expect(replay.confirmed_write_closeout).toMatchObject({
      journal_required: true,
      confirmation_valid: true,
      journal_recorded: false,
      replay_detected: true,
      record_available: true,
      closeout_status: "journal_replay_detected",
      reason: "journal_replay_detected",
      idempotency_key: "local-mcp-journal:journal-preflight-1",
      record_id: "local-mcp-journal:journal-preflight-1",
    });
    expect(listLocalMcpOperationJournalRecords()).toHaveLength(1);
  });

  it("does not journal read or unconfirmed write transport preflights", async () => {
    await expect(
      preflightLocalMcpTransportRequestWithJournal(
        {
          id: "journal-preflight-read",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T16:31:00.000Z",
      ),
    ).resolves.toMatchObject({
      dispatch_gate: {
        dispatch_allowed: false,
        executed: false,
        blocking_gates: [
          {
            gate: "capability_token",
            reason: "capability_token_missing",
          },
          {
            gate: "transport",
            reason: "transport_not_enabled",
          },
        ],
      },
      operation_journal_record: {
        record_written: false,
        reason: "journal_not_required",
        records: [],
      },
      confirmed_write_closeout: {
        journal_required: false,
        confirmation_valid: false,
        journal_recorded: false,
        replay_detected: false,
        record_available: false,
        closeout_status: "journal_skipped",
        reason: "journal_not_required",
        executed: false,
      },
    });

    await expect(
      preflightLocalMcpTransportRequestWithJournal(
        {
          id: "journal-preflight-unconfirmed",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: { input: { document_id: "doc-2" } },
        },
        "2026-07-13T16:32:00.000Z",
      ),
    ).resolves.toMatchObject({
      operation_journal_gate_preview: {
        journal_required: true,
        confirmation_valid: false,
        would_record_journal: false,
        reason: "confirmation_required",
      },
      operation_journal_record: {
        record_written: false,
        reason: "journal_not_required",
        records: [],
      },
      confirmed_write_closeout: {
        journal_required: true,
        confirmation_valid: false,
        journal_recorded: false,
        replay_detected: false,
        record_available: false,
        closeout_status: "journal_skipped",
        reason: "confirmation_required",
        executed: false,
      },
    });
    await expect(
      preflightLocalMcpTransportRequestWithJournal(
        {
          id: "journal-preflight-mismatch",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            input: { document_id: "doc-3" },
            confirmation_text: "wrong",
          },
        },
        "2026-07-13T16:33:00.000Z",
      ),
    ).resolves.toMatchObject({
      operation_journal_gate_preview: {
        journal_required: true,
        confirmation_valid: false,
        would_record_journal: false,
        reason: "confirmation_required",
      },
      operation_journal_record: {
        record_written: false,
        reason: "journal_not_required",
        records: [],
      },
    });
    expect(listLocalMcpOperationJournalRecords()).toEqual([]);
  });

  it("groups transport-facing readiness across disabled local MCP gates", () => {
    expect(localMcpWriteExecutionReadiness("starlane.spaces.list")).toEqual({
      runtime_mode: "local_vault",
      status: "not_required",
      write_execution_enabled: false,
      reason: "write_not_required",
    });
    expect(localMcpWriteExecutionReadiness("starlane.documents.delete")).toEqual({
      runtime_mode: "local_vault",
      status: "disabled",
      write_execution_enabled: false,
      reason: "write_not_enabled",
      error_code: "write_execution_disabled",
    });
    expect(localMcpTransportReadiness("stdio", "starlane.spaces.list")).toEqual({
      runtime_mode: "local_vault",
      transport: "stdio",
      transport_available: false,
      status: "disabled",
      tool: "starlane.spaces.list",
      tool_availability: "executable",
      read_only: true,
      executable: false,
      auth: localMcpCapabilityTokenAuthReadiness(),
      operation_journal: localMcpOperationJournalReadiness("starlane.spaces.list"),
      write_execution: localMcpWriteExecutionReadiness("starlane.spaces.list"),
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
      ],
    });
    expect(localMcpTransportReadiness("sse", "starlane.documents.delete")).toMatchObject({
      runtime_mode: "local_vault",
      transport: "sse",
      transport_available: false,
      status: "disabled",
      tool: "starlane.documents.delete",
      tool_availability: "confirmation_required",
      read_only: false,
      executable: false,
      write_execution: {
        status: "disabled",
        reason: "write_not_enabled",
        error_code: "write_execution_disabled",
      },
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
    });
  });

  it("wraps planned local transport requests in a non-executing envelope response", () => {
    expect(
      handleLocalMcpTransportRequestEnvelope({
        id: "request-1",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          input: {},
        },
      }),
    ).toEqual({
      id: "request-1",
      transport: "stdio",
      runtime_mode: "local_vault",
      executed: false,
      reason: "capability_token_not_enabled",
      error_code: "capability_token_required",
      preflight: preflightLocalMcpTransportRequest("stdio", "starlane.spaces.list", {
        input: {},
      }),
      request_trace: previewLocalMcpTransportRequestTrace(
        {
          id: "request-1",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            input: {},
          },
        },
        preflightLocalMcpTransportRequest("stdio", "starlane.spaces.list", {
          input: {},
        }),
      ),
    });

    expect(
      handleLocalMcpTransportRequestEnvelope({
        id: "request-2",
        transport: "sse",
        tool: "starlane.documents.delete",
        request: {
          capability_token: "candidate-token",
          confirmation_text: "confirm local mcp starlane.documents.delete",
        },
      }),
    ).toMatchObject({
      id: "request-2",
      transport: "sse",
      runtime_mode: "local_vault",
      executed: false,
      reason: "capability_token_not_enabled",
      preflight: {
        transport_available: false,
        capability_token: {
          valid: false,
          reason: "capability_token_not_enabled",
        },
        capability_token_auth_readiness: localMcpCapabilityTokenAuthReadiness(),
        operation_journal_readiness: localMcpOperationJournalReadiness(
          "starlane.documents.delete",
        ),
        transport_readiness: localMcpTransportReadiness("sse", "starlane.documents.delete"),
        guard: {
          executable: false,
          rejection_reason: "write_execution_disabled",
          confirmation_validation: {
            valid: true,
          },
        },
      },
      request_trace: {
        schema_version: 1,
        id: "request-2",
        transport: "sse",
        tool: "starlane.documents.delete",
        runtime_mode: "local_vault",
        stage: "transport_preflight",
        status: "preflight_only",
        blocking_stage: "capability_token",
        blocking_priority: "primary",
        secondary_blocking_stage: "tool_guard",
        executed: false,
        persisted: false,
        journaled: false,
        storage: "response_only",
        retention: "none",
        raw_values_stored: false,
        value_policy: "presence_only",
        input_present: false,
        confirmation_text_present: true,
        capability_token_present: true,
        redacted_fields: ["confirmation_text", "capability_token"],
        reason: "capability_token_not_enabled",
        error_code: "transport_disabled",
        tool_error_code: "write_execution_disabled",
      },
    });
  });

  it("wraps planned local transport envelopes with non-authorizing auth previews", async () => {
    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      handleLocalMcpTransportRequestEnvelopeWithAuthPreview(
        {
          id: "request-auth-preview-1",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
            input: {},
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      id: "request-auth-preview-1",
      transport: "stdio",
      runtime_mode: "local_vault",
      executed: false,
      reason: "capability_token_not_enabled",
      error_code: "transport_disabled",
      preflight: {
        transport_available: false,
        capability_token: {
          valid: false,
          reason: "capability_token_not_enabled",
        },
      },
      capability_token_validation_preview: {
        token_accepted: false,
        token_present: true,
        token_record_found: true,
        fingerprint_matched: true,
        expired: false,
        reason: "capability_token_not_enabled",
      },
      capability_token_auth_gate_preview: {
        token_checks_passed: true,
        token_accepted: false,
        transport_dispatch_allowed: false,
        reason: "capability_token_ready",
      },
      capability_token_acceptance_gate_preview: {
        token_acceptance_enabled: false,
        token_checks_passed: true,
        token_accepted: false,
        acceptance_allowed: false,
        listener_binding_allowed: false,
        transport_dispatch_allowed: false,
        reason: "capability_token_acceptance_not_enabled",
      },
      transport_dispatch_gate_preview: {
        runtime_mode: "local_vault",
        dispatch_allowed: false,
        executed: false,
        transport: "stdio",
        tool: "starlane.spaces.list",
        envelope_validation: {
          valid: true,
        },
        auth_gate: {
          token_checks_passed: true,
          reason: "capability_token_ready",
        },
        acceptance_gate: {
          token_checks_passed: true,
          reason: "capability_token_acceptance_not_enabled",
          acceptance_allowed: false,
        },
        guard: {
          executable: true,
        },
        blocking_gates: [
          {
            gate: "token_acceptance",
            status: "disabled",
            reason: "capability_token_acceptance_not_enabled",
            error_code: "capability_token_not_enabled",
          },
          {
            gate: "transport",
            status: "disabled",
            reason: "transport_not_enabled",
            error_code: "transport_disabled",
          },
        ],
        reason: "capability_token_acceptance_not_enabled",
        error_code: "capability_token_not_enabled",
      },
      request_trace: {
        schema_version: 1,
        id: "request-auth-preview-1",
        transport: "stdio",
        tool: "starlane.spaces.list",
        stage: "transport_preflight",
        executed: false,
        persisted: false,
        journaled: false,
      },
    });

    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "request-auth-preview-read-accept",
        transport: "stdio",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-raw-token-1",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      handleLocalMcpTransportRequestEnvelopeWithAuthPreview(
        {
          id: "request-auth-preview-read-execute",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:32:00.000Z",
        { transport_enabled: true },
      ),
    ).resolves.toMatchObject({
      id: "request-auth-preview-read-execute",
      transport: "stdio",
      runtime_mode: "local_vault",
      executed: true,
      reason: "dispatch_allowed",
      capability_token_acceptance_gate_preview: {
        token_accepted: true,
        acceptance_allowed: true,
        reason: "capability_token_accepted",
      },
      transport_dispatch_gate_preview: {
        dispatch_allowed: true,
        executed: false,
        transport_gate_enabled: true,
        blocking_gates: [],
        reason: "dispatch_allowed",
      },
      request_trace: {
        schema_version: 1,
        id: "request-auth-preview-read-execute",
        transport: "stdio",
        tool: "starlane.spaces.list",
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
        input_present: false,
        confirmation_text_present: false,
        capability_token_present: true,
        redacted_fields: ["capability_token"],
        reason: "dispatch_allowed",
      },
      result: {
        tool: "starlane.spaces.list",
        read_only: true,
        runtime_mode: "local_vault",
        result: [expect.objectContaining({ id: "local-space", name: "Personal Space" })],
      },
    });

    const writeTokenRequest = {
      request_id: "token-request-auth-write",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };
    await issueLocalMcpCapabilityToken(writeTokenRequest, "local-mcp-write-token");
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "request-auth-preview-write-accept",
        transport: "sse",
        tool: "starlane.documents.delete",
        request: {
          capability_token: "local-mcp-write-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    const acceptedWriteResponse = await handleLocalMcpTransportRequestEnvelopeWithAuthPreview(
      {
        id: "request-auth-preview-write",
        transport: "sse",
        tool: "starlane.documents.delete",
        request: {
          capability_token: "local-mcp-write-token",
          confirmation_text: "confirm local mcp starlane.documents.delete",
          input: { document_id: "doc-auth-write" },
        },
      },
      "2026-07-13T13:32:00.000Z",
    );
    expect(acceptedWriteResponse).toMatchObject({
      id: "request-auth-preview-write",
      transport: "sse",
      executed: false,
      capability_token_acceptance_gate_preview: {
        token_checks_passed: true,
        token_accepted: true,
        acceptance_allowed: true,
        reason: "capability_token_accepted",
      },
      transport_dispatch_gate_preview: {
        dispatch_allowed: false,
        acceptance_gate: {
          acceptance_allowed: true,
          reason: "capability_token_accepted",
        },
        blocking_gates: [
          {
            gate: "transport",
            reason: "transport_not_enabled",
          },
          {
            gate: "operation_journal",
            reason: "operation_journal_not_enabled",
          },
          {
            gate: "write_execution",
            reason: "write_execution_disabled",
          },
        ],
      },
      journaled_preflight: {
        dispatch_gate: {
          dispatch_allowed: false,
          reason: "transport_not_enabled",
        },
        operation_journal_record: {
          record_written: true,
          record: {
            id: "local-mcp-journal:request-auth-preview-write",
            target: "doc-auth-write",
            raw_confirmation_text_stored: false,
            raw_capability_token_stored: false,
            executed: false,
          },
        },
        confirmed_write_closeout: {
          journal_recorded: true,
          closeout_status: "journal_recorded",
          execution_allowed: false,
          executed: false,
          record_id: "local-mcp-journal:request-auth-preview-write",
        },
      },
    });
    expect("journaled_preflight" in acceptedWriteResponse).toBe(true);
    if (!("journaled_preflight" in acceptedWriteResponse)) {
      throw new Error("expected journaled preflight on accepted write auth response");
    }
    expect(acceptedWriteResponse.journaled_preflight).toBeDefined();
    expect(acceptedWriteResponse.transport_dispatch_gate_preview).toBe(
      acceptedWriteResponse.journaled_preflight?.dispatch_gate,
    );
    expect(listLocalMcpOperationJournalRecords()).toHaveLength(1);

    await expect(
      handleLocalMcpTransportRequestEnvelopeWithAuthPreview(
        {
          id: "",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toEqual({
      id: "",
      transport: "stdio",
      runtime_mode: "local_vault",
      executed: false,
      reason: "request_invalid",
      error_code: "invalid_request",
      validation: {
        valid: false,
        reason: "request_id_required",
        error_code: "invalid_request",
      },
      request_trace: previewLocalMcpTransportRejectedRequestTrace(
        {
          id: "",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        {
          valid: false,
          reason: "request_id_required",
          error_code: "invalid_request",
        },
      ),
    });
    await expect(
      handleLocalMcpTransportRequestEnvelopeWithAuthPreview(
        {
          id: "",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.not.toHaveProperty("transport_dispatch_gate_preview");
  });

  it("previews local MCP transport request traces without persisting or journaling", () => {
    expect(localMcpTransportRequestTraceContract()).toEqual({
      schema_version: 1,
      runtime_mode: "local_vault",
      status: "preview",
      storage: "response_only",
      retention: "none",
      persisted: false,
      journaled: false,
      raw_values_stored: false,
      value_policy: "presence_only",
      stages: ["envelope_validation", "transport_preflight"],
      primary_blocking_stages: ["envelope_validation", "capability_token", "transport"],
      secondary_blocking_stages: ["tool_guard"],
      attaches_to: ["valid_preflight_response", "invalid_envelope_response"],
    });

    expect(localMcpTransportRequestTracePolicy()).toEqual({
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
    });

    const readEnvelope = {
      id: "trace-1",
      transport: "stdio" as const,
      tool: "starlane.spaces.list",
    };
    const readPreflight = preflightLocalMcpTransportRequest(
      readEnvelope.transport,
      readEnvelope.tool,
    );

    expect(previewLocalMcpTransportRequestTrace(readEnvelope, readPreflight)).toEqual({
      schema_version: 1,
      id: "trace-1",
      transport: "stdio",
      tool: "starlane.spaces.list",
      runtime_mode: "local_vault",
      stage: "transport_preflight",
      status: "preflight_only",
      blocking_stage: "capability_token",
      blocking_priority: "primary",
      executed: false,
      persisted: false,
      journaled: false,
      storage: "response_only",
      retention: "none",
      raw_values_stored: false,
      value_policy: "presence_only",
      input_present: false,
      confirmation_text_present: false,
      capability_token_present: false,
      redacted_fields: [],
      reason: "capability_token_not_enabled",
      error_code: "capability_token_required",
    });

    const writeEnvelope = {
      id: "trace-2",
      transport: "sse" as const,
      tool: "starlane.documents.delete",
      request: {
        confirmation_text: "confirm local mcp starlane.documents.delete",
      },
    };
    const writePreflight = preflightLocalMcpTransportRequest(
      writeEnvelope.transport,
      writeEnvelope.tool,
      writeEnvelope.request,
    );

    expect(previewLocalMcpTransportRequestTrace(writeEnvelope, writePreflight)).toEqual({
      schema_version: 1,
      id: "trace-2",
      transport: "sse",
      tool: "starlane.documents.delete",
      runtime_mode: "local_vault",
      stage: "transport_preflight",
      status: "preflight_only",
      blocking_stage: "capability_token",
      blocking_priority: "primary",
      secondary_blocking_stage: "tool_guard",
      executed: false,
      persisted: false,
      journaled: false,
      storage: "response_only",
      retention: "none",
      raw_values_stored: false,
      value_policy: "presence_only",
      input_present: false,
      confirmation_text_present: true,
      capability_token_present: false,
      redacted_fields: ["confirmation_text"],
      reason: "capability_token_not_enabled",
      error_code: "capability_token_required",
      tool_error_code: "write_execution_disabled",
    });

    const inputEnvelope = {
      id: "trace-3",
      transport: "stdio" as const,
      tool: "starlane.spaces.get",
      request: {
        input: { space_id: "local-space" },
        capability_token: "candidate-token",
      },
    };
    const inputPreflight = preflightLocalMcpTransportRequest(
      inputEnvelope.transport,
      inputEnvelope.tool,
      inputEnvelope.request,
    );

    expect(previewLocalMcpTransportRequestTrace(inputEnvelope, inputPreflight)).toMatchObject({
      schema_version: 1,
      id: "trace-3",
      transport: "stdio",
      tool: "starlane.spaces.get",
      stage: "transport_preflight",
      blocking_stage: "capability_token",
      blocking_priority: "primary",
      executed: false,
      persisted: false,
      journaled: false,
      storage: "response_only",
      retention: "none",
      raw_values_stored: false,
      value_policy: "presence_only",
      input_present: true,
      confirmation_text_present: false,
      capability_token_present: true,
      redacted_fields: ["input", "capability_token"],
    });
  });

  it("handles enabled local MCP transport envelopes through the canonical dispatch gate", async () => {
    const readTokenRequest = {
      request_id: "token-request-enabled-read",
      created_at: "2026-07-13T13:01:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "sse" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      readTokenRequest,
      "local-mcp-enabled-read-token",
    );

    await expect(
      handleLocalMcpEnabledTransportRequestEnvelope(
        {
          id: "enabled-read-unaccepted",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-enabled-read-token",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      id: "enabled-read-unaccepted",
      transport: "sse",
      executed: false,
      reason: "capability_token_not_enabled",
      capability_token_acceptance_gate_preview: {
        token_checks_passed: true,
        token_accepted: false,
        acceptance_allowed: false,
        reason: "capability_token_acceptance_not_enabled",
      },
      transport_dispatch_gate_preview: {
        dispatch_allowed: false,
        transport_gate_enabled: true,
        blocking_gates: [
          {
            gate: "token_acceptance",
            reason: "capability_token_acceptance_not_enabled",
          },
        ],
      },
    });

    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "enabled-read-accept",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: {
          capability_token: "local-mcp-enabled-read-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    await expect(
      handleLocalMcpEnabledTransportRequestEnvelope(
        {
          id: "enabled-read-execute",
          transport: "sse",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-enabled-read-token",
          },
        },
        "2026-07-13T13:32:00.000Z",
      ),
    ).resolves.toMatchObject({
      id: "enabled-read-execute",
      transport: "sse",
      runtime_mode: "local_vault",
      executed: true,
      reason: "dispatch_allowed",
      capability_token_acceptance_gate_preview: {
        token_checks_passed: true,
        token_accepted: true,
        acceptance_allowed: true,
        reason: "capability_token_accepted",
      },
      transport_dispatch_gate_preview: {
        dispatch_allowed: true,
        transport_gate_enabled: true,
        blocking_gates: [],
        reason: "dispatch_allowed",
      },
      request_trace: {
        stage: "transport_dispatch",
        status: "dispatch_executed",
        executed: true,
        raw_values_stored: false,
        redacted_fields: ["capability_token"],
        reason: "dispatch_allowed",
      },
      result: {
        tool: "starlane.spaces.list",
        read_only: true,
        runtime_mode: "local_vault",
        result: [expect.objectContaining({ id: "local-space", name: "Personal Space" })],
      },
    });

    const writeTokenRequest = {
      request_id: "token-request-enabled-write",
      created_at: "2026-07-13T13:02:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };

    await issueLocalMcpCapabilityToken(
      writeTokenRequest,
      "local-mcp-enabled-write-token",
    );
    await acceptLocalMcpCapabilityTokenRecord(
      {
        id: "enabled-write-accept",
        transport: "stdio",
        tool: "starlane.documents.delete",
        request: {
          capability_token: "local-mcp-enabled-write-token",
        },
      },
      "2026-07-13T13:31:00.000Z",
      { acceptance_enabled: true },
    );

    const enabledWriteResponse = await handleLocalMcpEnabledTransportRequestEnvelope(
      {
        id: "enabled-write-journal",
        transport: "stdio",
        tool: "starlane.documents.delete",
        request: {
          capability_token: "local-mcp-enabled-write-token",
          confirmation_text: "confirm local mcp starlane.documents.delete",
          input: { document_id: "doc-enabled-write" },
        },
      },
      "2026-07-13T13:32:00.000Z",
    );

    expect(enabledWriteResponse).toMatchObject({
      id: "enabled-write-journal",
      transport: "stdio",
      executed: false,
      capability_token_acceptance_gate_preview: {
        token_accepted: true,
        acceptance_allowed: true,
      },
      transport_dispatch_gate_preview: {
        dispatch_allowed: false,
        transport_gate_enabled: true,
        blocking_gates: [
          {
            gate: "operation_journal",
            reason: "operation_journal_not_enabled",
          },
          {
            gate: "write_execution",
            reason: "write_execution_disabled",
          },
        ],
      },
      journaled_preflight: {
        operation_journal_record: {
          record_written: true,
          record: {
            id: "local-mcp-journal:enabled-write-journal",
            target: "doc-enabled-write",
            confirmation_text_present: true,
            raw_confirmation_text_stored: false,
            raw_capability_token_stored: false,
            executed: false,
          },
        },
        confirmed_write_closeout: {
          journal_recorded: true,
          execution_allowed: false,
          executed: false,
        },
      },
    });
    expect(listLocalMcpOperationJournalRecords()).toHaveLength(1);
  });

  it("validates local transport request envelopes before preflight", () => {
    expect(
      validateLocalMcpTransportRequestEnvelope({
        id: "request-1",
        transport: "stdio",
        tool: "starlane.spaces.list",
      }),
    ).toEqual({ valid: true });
    expect(
      validateLocalMcpTransportRequestEnvelope({
        id: "   ",
        transport: "stdio",
        tool: "starlane.spaces.list",
      }),
    ).toEqual({ valid: false, reason: "request_id_required", error_code: "invalid_request" });
    expect(
      validateLocalMcpTransportRequestEnvelope({
        id: "request-1",
        transport: "websocket" as "stdio",
        tool: "starlane.spaces.list",
      }),
    ).toEqual({ valid: false, reason: "transport_unsupported", error_code: "invalid_request" });
    expect(
      validateLocalMcpTransportRequestEnvelope({
        id: "request-1",
        transport: "stdio",
        tool: " ",
      }),
    ).toEqual({ valid: false, reason: "tool_required", error_code: "invalid_request" });
  });

  it("rejects invalid local transport envelopes without preflight", () => {
    expect(
      handleLocalMcpTransportRequestEnvelope({
        id: " ",
        transport: "stdio",
        tool: "starlane.spaces.list",
      }),
    ).toEqual({
      id: " ",
      transport: "stdio",
      runtime_mode: "local_vault",
      executed: false,
      reason: "request_invalid",
      error_code: "invalid_request",
      validation: { valid: false, reason: "request_id_required", error_code: "invalid_request" },
      request_trace: previewLocalMcpTransportRejectedRequestTrace(
        {
          id: " ",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        { valid: false, reason: "request_id_required", error_code: "invalid_request" },
      ),
    });

    expect(
      handleLocalMcpTransportRequestEnvelope({
        id: "request-1",
        transport: "websocket" as "stdio",
        tool: "starlane.spaces.list",
        request: {
          input: { space_id: "local-space" },
          capability_token: "candidate-token",
        },
      }),
    ).toEqual({
      id: "request-1",
      transport: undefined,
      runtime_mode: "local_vault",
      executed: false,
      reason: "request_invalid",
      error_code: "invalid_request",
      validation: { valid: false, reason: "transport_unsupported", error_code: "invalid_request" },
      request_trace: {
        schema_version: 1,
        id: "request-1",
        transport: undefined,
        tool: "starlane.spaces.list",
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
        input_present: true,
        confirmation_text_present: false,
        capability_token_present: true,
        redacted_fields: ["input", "capability_token"],
        reason: "request_invalid",
        validation_reason: "transport_unsupported",
        error_code: "invalid_request",
      },
    });
  });

  it("maps local MCP preflight and guard failures to stable error codes", () => {
    expect(localMcpPreflightErrorCode(validateLocalMcpCapabilityToken(undefined))).toBe(
      "capability_token_required",
    );
    expect(localMcpPreflightErrorCode(validateLocalMcpCapabilityToken("candidate-token"))).toBe(
      "transport_disabled",
    );
    expect(
      handleLocalMcpTransportRequestEnvelope({
        id: "request-1",
        transport: "stdio",
        tool: "starlane.spaces.list",
      }),
    ).toMatchObject({
      error_code: "capability_token_required",
      preflight: {
        error_code: "capability_token_required",
      },
    });
    expect(localMcpGuardErrorCode(guardLocalMcpToolExecution("starlane.documents.delete"))).toBe(
      "write_execution_disabled",
    );
    expect(localMcpGuardErrorCode(guardLocalMcpToolExecution("starlane.import.preview"))).toBe(
      "tool_not_implemented",
    );
    expect(localMcpGuardErrorCode(guardLocalMcpToolExecution("starlane.unknown"))).toBe("tool_unknown");
    expect(localMcpGuardErrorCode(guardLocalMcpToolExecution("starlane.spaces.list"))).toBeUndefined();
    expect(preflightLocalMcpTransportRequest("stdio", "starlane.spaces.list")).not.toHaveProperty(
      "tool_error_code",
    );
    expect(preflightLocalMcpTransportRequest("stdio", "starlane.documents.delete")).toMatchObject({
      tool_error_code: "write_execution_disabled",
    });
    expect(preflightLocalMcpTransportRequest("stdio", "starlane.import.preview")).toMatchObject({
      tool_error_code: "tool_not_implemented",
    });
    expect(preflightLocalMcpTransportRequest("stdio", "starlane.unknown")).toMatchObject({
      tool_error_code: "tool_unknown",
    });
  });

  it("executes discovered local tools by string name and rejects blocked or unknown tools", async () => {
    await expect(executeLocalMcpTool("starlane.spaces.list")).resolves.toEqual({
      tool: "starlane.spaces.list",
      read_only: true,
      runtime_mode: "local_vault",
      result: [expect.objectContaining({ id: "local-space" })],
    });
    await expect(executeLocalMcpTool("starlane.documents.create")).rejects.toThrow(
      "MCP local tool requires write/confirmation support and is not enabled: starlane.documents.create",
    );
    await expect(executeLocalMcpTool("starlane.import.preview")).rejects.toThrow(
      "MCP local tool is cataloged but not implemented locally: starlane.import.preview",
    );
    await expect(executeLocalMcpTool("starlane.unknown")).rejects.toThrow(
      "MCP local tool is unknown: starlane.unknown",
    );
  });

  it("plans local tool safety before execution", () => {
    expect(planLocalMcpToolExecution("starlane.spaces.list")).toEqual({
      name: "starlane.spaces.list",
      availability: "executable",
      read_only: true,
      runtime_mode: "local_vault",
      requires_confirmation: false,
    });
    expect(planLocalMcpToolExecution("starlane.documents.delete")).toEqual({
      name: "starlane.documents.delete",
      availability: "confirmation_required",
      read_only: false,
      runtime_mode: "local_vault",
      requires_confirmation: true,
      reason: "write_not_enabled",
    });
    expect(planLocalMcpToolExecution("starlane.import.preview")).toEqual({
      name: "starlane.import.preview",
      availability: "not_implemented",
      read_only: false,
      runtime_mode: "local_vault",
      requires_confirmation: false,
      reason: "not_implemented",
    });
    expect(planLocalMcpToolExecution("starlane.unknown")).toEqual({
      name: "starlane.unknown",
      availability: "unknown",
      read_only: false,
      runtime_mode: "local_vault",
      requires_confirmation: false,
      reason: "unknown_tool",
    });
  });

  it("builds confirmation challenges only for blocked local write tools", () => {
    expect(buildLocalMcpConfirmationChallenge("starlane.documents.delete")).toEqual({
      tool: "starlane.documents.delete",
      runtime_mode: "local_vault",
      required: true,
      confirmation_text: "confirm local mcp starlane.documents.delete",
      reason: "write_not_enabled",
    });
    expect(buildLocalMcpConfirmationChallenge("starlane.spaces.list")).toBeNull();
    expect(buildLocalMcpConfirmationChallenge("starlane.import.preview")).toBeNull();
    expect(buildLocalMcpConfirmationChallenge("starlane.unknown")).toBeNull();
  });

  it("validates local confirmation text without enabling execution", () => {
    expect(
      validateLocalMcpConfirmation(
        "starlane.documents.delete",
        "confirm local mcp starlane.documents.delete",
      ),
    ).toEqual({
      tool: "starlane.documents.delete",
      valid: true,
      runtime_mode: "local_vault",
      reason: undefined,
    });
    expect(validateLocalMcpConfirmation("starlane.documents.delete", "delete it")).toEqual({
      tool: "starlane.documents.delete",
      valid: false,
      runtime_mode: "local_vault",
      reason: "confirmation_mismatch",
    });
    expect(
      validateLocalMcpConfirmation("starlane.spaces.list", "confirm local mcp starlane.spaces.list"),
    ).toEqual({
      tool: "starlane.spaces.list",
      valid: false,
      runtime_mode: "local_vault",
      reason: "confirmation_not_required",
    });
  });

  it("guards transport-facing local execution before dispatch", () => {
    expect(guardLocalMcpToolExecution("starlane.spaces.list")).toEqual({
      tool: "starlane.spaces.list",
      executable: true,
      runtime_mode: "local_vault",
      plan: {
        name: "starlane.spaces.list",
        availability: "executable",
        read_only: true,
        runtime_mode: "local_vault",
        requires_confirmation: false,
      },
    });

    expect(
      guardLocalMcpToolExecution("starlane.documents.delete", {
        confirmation_text: "confirm local mcp starlane.documents.delete",
      }),
    ).toEqual({
      tool: "starlane.documents.delete",
      executable: false,
      runtime_mode: "local_vault",
      plan: {
        name: "starlane.documents.delete",
        availability: "confirmation_required",
        read_only: false,
        runtime_mode: "local_vault",
        requires_confirmation: true,
        reason: "write_not_enabled",
      },
      confirmation_challenge: {
        tool: "starlane.documents.delete",
        runtime_mode: "local_vault",
        required: true,
        confirmation_text: "confirm local mcp starlane.documents.delete",
        reason: "write_not_enabled",
      },
      confirmation_validation: {
        tool: "starlane.documents.delete",
        valid: true,
        runtime_mode: "local_vault",
        reason: undefined,
      },
      rejection_reason: "write_execution_disabled",
    });

    expect(guardLocalMcpToolExecution("starlane.import.preview")).toMatchObject({
      tool: "starlane.import.preview",
      executable: false,
      rejection_reason: "not_implemented",
    });
    expect(guardLocalMcpToolExecution("starlane.unknown")).toMatchObject({
      tool: "starlane.unknown",
      executable: false,
      rejection_reason: "unknown_tool",
    });
  });

  it("previews local MCP transport dispatch gates without executing", async () => {
    await expect(
      previewLocalMcpTransportDispatchGate(
        {
          id: " ",
          transport: "stdio",
          tool: "starlane.spaces.list",
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toEqual({
      runtime_mode: "local_vault",
      dispatch_allowed: false,
      executed: false,
      transport: "stdio",
      tool: "starlane.spaces.list",
      envelope_validation: {
        valid: false,
        reason: "request_id_required",
        error_code: "invalid_request",
      },
      blocking_gates: [
        {
          gate: "envelope",
          status: "blocked",
          reason: "request_id_required",
          error_code: "invalid_request",
        },
      ],
      reason: "request_id_required",
      error_code: "invalid_request",
    });

    const request = {
      request_id: "token-request-1",
      created_at: "2026-07-13T13:00:00.000Z",
      runtime_mode: "local_vault" as const,
      transport: "stdio" as const,
      scope: "notes:read" as const,
      vault_fingerprint: "vault-sha256:abc123",
      confirmation_text: "confirm local mcp capability token",
    };
    await issueLocalMcpCapabilityToken(request, "local-mcp-raw-token-1");

    await expect(
      previewLocalMcpTransportDispatchGate(
        {
          id: "dispatch-gate-read",
          transport: "stdio",
          tool: "starlane.spaces.list",
          request: {
            capability_token: "local-mcp-raw-token-1",
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      runtime_mode: "local_vault",
      dispatch_allowed: false,
      executed: false,
      transport: "stdio",
      tool: "starlane.spaces.list",
      envelope_validation: {
        valid: true,
      },
      auth_gate: {
        token_checks_passed: true,
        token_accepted: false,
        transport_dispatch_allowed: false,
        reason: "capability_token_ready",
      },
      acceptance_gate: {
        token_checks_passed: true,
        token_accepted: false,
        acceptance_allowed: false,
        listener_binding_allowed: false,
        transport_dispatch_allowed: false,
        reason: "capability_token_acceptance_not_enabled",
      },
      transport_readiness: {
        transport_available: false,
      },
      operation_journal_readiness: {
        preview: {
          required: false,
          reason: "journal_not_required",
        },
      },
      guard: {
        executable: true,
      },
      blocking_gates: [
        {
          gate: "token_acceptance",
          status: "disabled",
          reason: "capability_token_acceptance_not_enabled",
          error_code: "capability_token_not_enabled",
        },
        {
          gate: "transport",
          status: "disabled",
          reason: "transport_not_enabled",
          error_code: "transport_disabled",
        },
      ],
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
    });

    await expect(
      previewLocalMcpTransportDispatchGate(
        {
          id: "dispatch-gate-write",
          transport: "sse",
          tool: "starlane.documents.delete",
          request: {
            capability_token: "local-mcp-raw-token-1",
            confirmation_text: "confirm local mcp starlane.documents.delete",
            input: {
              document_id: "doc-1",
            },
          },
        },
        "2026-07-13T13:30:00.000Z",
      ),
    ).resolves.toMatchObject({
      runtime_mode: "local_vault",
      dispatch_allowed: false,
      executed: false,
      transport: "sse",
      tool: "starlane.documents.delete",
      auth_gate: {
        token_checks_passed: true,
      },
      acceptance_gate: {
        token_checks_passed: true,
        acceptance_allowed: false,
        reason: "capability_token_acceptance_not_enabled",
      },
      operation_journal_readiness: {
        preview: {
          required: true,
          reason: "operation_journal_not_enabled",
        },
      },
      guard: {
        executable: false,
        rejection_reason: "write_execution_disabled",
        confirmation_validation: {
          valid: true,
        },
      },
      blocking_gates: [
        {
          gate: "token_acceptance",
          status: "disabled",
          reason: "capability_token_acceptance_not_enabled",
          error_code: "capability_token_not_enabled",
        },
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
        {
          gate: "write_execution",
          status: "disabled",
          reason: "write_execution_disabled",
          error_code: "write_execution_disabled",
        },
      ],
      reason: "capability_token_acceptance_not_enabled",
      error_code: "capability_token_not_enabled",
    });
  });

  it("does not execute blocked write tools even with valid confirmation text", async () => {
    await expect(
      executeLocalMcpTool("starlane.documents.delete", {
        confirmation_text: "confirm local mcp starlane.documents.delete",
      }),
    ).rejects.toThrow(
      "MCP local tool requires write/confirmation support and is not enabled: starlane.documents.delete",
    );
  });

  it("executes local spaces.list against the local vault data client", async () => {
    const response = await executeLocalMcpReadOnlyTool("starlane.spaces.list");

    expect(response).toEqual({
      tool: "starlane.spaces.list",
      read_only: true,
      runtime_mode: "local_vault",
      result: [expect.objectContaining({ id: "local-space", name: "Personal Space" })],
    });
  });

  it("executes local spaces.get against the local vault data client", async () => {
    await dataClient.updateSpace("local-space", { name: "Field Notes" });

    await expect(
      executeLocalMcpReadOnlyTool("starlane.spaces.get", {
        input: { space_id: "local-space" },
      }),
    ).resolves.toEqual({
      tool: "starlane.spaces.get",
      read_only: true,
      runtime_mode: "local_vault",
      result: expect.objectContaining({ id: "local-space", name: "Field Notes" }),
    });
  });

  it("executes local folders.list against the local vault data client", async () => {
    const folder = await dataClient.createFolder("local-space", "Projects");

    await expect(
      executeLocalMcpReadOnlyTool("starlane.folders.list", {
        input: { space_id: "local-space" },
      }),
    ).resolves.toEqual({
      tool: "starlane.folders.list",
      read_only: true,
      runtime_mode: "local_vault",
      result: [expect.objectContaining({ id: folder.id, name: "Projects" })],
    });
  });

  it("executes local documents.get with content against the local vault data client", async () => {
    const document = await dataClient.createDocument("local-space", {
      title: "Field Report",
      content: "# Report",
    });

    await expect(
      executeLocalMcpReadOnlyTool("starlane.documents.get", {
        input: { space_id: "local-space", document_id: document.id },
      }),
    ).resolves.toEqual({
      tool: "starlane.documents.get",
      read_only: true,
      runtime_mode: "local_vault",
      result: expect.objectContaining({
        id: document.id,
        title: "Field Report",
        content: "# Report",
      }),
    });
  });

  it("executes local metadata and template list tools against the local vault data client", async () => {
    const tag = await dataClient.createTag("local-space", { name: "Planning", color: "#2563eb" });
    const property = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "select",
    });
    const template = await dataClient.createTemplate("local-space", {
      name: "Project brief",
      content: "# Brief",
      tags: ["Planning"],
      properties: { status: "Draft" },
    });

    await expect(
      executeLocalMcpReadOnlyTool("starlane.tags.list", {
        input: { space_id: "local-space" },
      }),
    ).resolves.toEqual({
      tool: "starlane.tags.list",
      read_only: true,
      runtime_mode: "local_vault",
      result: [expect.objectContaining({ id: tag.id, name: "Planning" })],
    });
    await expect(
      executeLocalMcpReadOnlyTool("starlane.properties.list", {
        input: { space_id: "local-space" },
      }),
    ).resolves.toEqual({
      tool: "starlane.properties.list",
      read_only: true,
      runtime_mode: "local_vault",
      result: [expect.objectContaining({ id: property.id, name: "Status" })],
    });
    await expect(
      executeLocalMcpReadOnlyTool("starlane.templates.list", {
        input: { space_id: "local-space" },
      }),
    ).resolves.toEqual({
      tool: "starlane.templates.list",
      read_only: true,
      runtime_mode: "local_vault",
      result: [expect.objectContaining({ id: template.id, name: "Project brief" })],
    });
  });

  it("executes local search.query against the local vault data client", async () => {
    const matching = await dataClient.createDocument("local-space", {
      title: "Telemetry Notes",
      content: "Needle marker",
    });
    await dataClient.createDocument("local-space", {
      title: "Other Notes",
      content: "No match here",
    });

    await expect(
      executeLocalMcpReadOnlyTool("starlane.search.query", {
        input: { q: "needle", space_id: "local-space", limit: 5 },
      }),
    ).resolves.toEqual({
      tool: "starlane.search.query",
      read_only: true,
      runtime_mode: "local_vault",
      result: [expect.objectContaining({ document_id: matching.id, title: "Telemetry Notes" })],
    });
  });

  it("executes local graph.get against the local vault data client", async () => {
    const document = await dataClient.createDocument("local-space", {
      title: "Graph Node",
      content: "Connected later",
    });

    await expect(
      executeLocalMcpReadOnlyTool("starlane.graph.get", {
        input: { space_id: "local-space" },
      }),
    ).resolves.toEqual({
      tool: "starlane.graph.get",
      read_only: true,
      runtime_mode: "local_vault",
      result: expect.objectContaining({
        nodes: expect.arrayContaining([expect.objectContaining({ id: document.id, title: "Graph Node" })]),
        edges: [],
      }),
    });
  });

  it("validates required local tool input", async () => {
    await expect(executeLocalMcpReadOnlyTool("starlane.spaces.get")).rejects.toThrow(
      "MCP local tool input must include space_id",
    );
    await expect(
      executeLocalMcpReadOnlyTool("starlane.spaces.get", { input: { space_id: " " } }),
    ).rejects.toThrow("MCP local tool input must include space_id");
    await expect(
      executeLocalMcpReadOnlyTool("starlane.documents.get", { input: { space_id: "local-space" } }),
    ).rejects.toThrow("MCP local tool input must include document_id");
    await expect(executeLocalMcpReadOnlyTool("starlane.tags.list")).rejects.toThrow(
      "MCP local tool input must include space_id",
    );
    await expect(
      executeLocalMcpReadOnlyTool("starlane.search.query", { input: { q: "needle", limit: 101 } }),
    ).rejects.toThrow("MCP local search limit must be an integer from 1 to 100");
    await expect(executeLocalMcpReadOnlyTool("starlane.graph.get")).rejects.toThrow(
      "MCP local tool input must include space_id",
    );
  });

  it("executes one accepted, confirmed document create once and journals its request ID", async () => {
    const space = await dataClient.createSpace("MCP writes");
    const token = "local-mcp-document-create-token";
    await issueLocalMcpCapabilityToken(
      {
        request_id: "document-create-token",
        created_at: "2026-07-21T17:00:00.000Z",
        runtime_mode: "local_vault",
        transport: "stdio",
        scope: "notes:read",
        vault_fingerprint: "vault-sha256:mcp-write",
        confirmation_text: "confirm local mcp capability token",
      },
      token,
    );
    await acceptLocalMcpCapabilityTokenRecord(
      { id: "document-create-token-accept", transport: "stdio", tool: "starlane.documents.create", request: { capability_token: token } },
      "2026-07-21T17:01:00.000Z",
      { acceptance_enabled: true },
    );
    const envelope = {
      id: "document-create-once",
      transport: "stdio" as const,
      tool: "starlane.documents.create",
      request: { capability_token: token, confirmation_text: "confirm local mcp starlane.documents.create", input: { space_id: space.id, title: "Created through MCP", content: "journaled" } },
    };

    await expect(executeConfirmedLocalMcpDocumentCreate(envelope, "2026-07-21T17:02:00.000Z")).resolves.toMatchObject({ read_only: false, result: { id: expect.any(String) } });
    await expect(executeConfirmedLocalMcpDocumentCreate(envelope, "2026-07-21T17:03:00.000Z")).rejects.toThrow("already processed");
    expect(await dataClient.listDocuments(space.id)).toHaveLength(1);
    expect(listLocalMcpOperationJournalRecords()).toEqual(expect.arrayContaining([expect.objectContaining({ id: "local-mcp-journal:document-create-once", target: space.id })]));
  });
});
