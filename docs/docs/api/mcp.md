# MCP Server Plan

Starlane Notes ships an early MCP contract surface. This page defines the target contract for the first supported server so the implementation can be reviewed against a stable safety model.

v1.0.12 adds the first docs-to-code scaffold in `server/src/mcp.rs`: a typed tool catalog, documented scope tokens, confirmation-boundary helpers, and a serializable manifest with focused tests. The manifest now includes a catalog `summary`, explicit per-tool `runtime_modes`, plus `input_schema` and `output_schema` fields for the current catalog, including representative read, write, delete, workflow, import/export, and admin audit tools. It is exposed at `GET /api/v1/mcp/manifest` for authenticated users. A conservative authenticated HTTP execution endpoint exists at `POST /api/v1/mcp/tools/{tool_name}` for the current no-confirmation read tools, and the generated OpenAPI route documents the `{tool_name}` path parameter: `notes:read` space/folder/document/search/graph/tag/property/template tools plus the admin-scoped `starlane.admin.users.list` read tool. Current server catalog entries report only `server` runtime support; a first local-vault runtime helper exists in the web/desktop client for spaces, folder list, document get, search, graph, tags, property schemas, and templates, but local-vault transport exposure, write, delete, import/export, workflow-write, admin audit export, stdio, and SSE transports remain disabled.

## Transport and Authentication

The MCP server should support local stdio for desktop/local vault automation and HTTP/SSE for self-hosted server mode.

Current server behavior is intentionally narrower than the long-term MCP server plan: `GET /api/v1/mcp/manifest` returns the reviewed tool catalog, scope tokens, catalog summary counts, confirmation policies, runtime modes, and input/output schemas through the normal Starlane bearer/API-key authentication path. Its manifest reports `transport_status: "read_only_http_tools"` because only the authenticated HTTP read-only tool subset can execute. Each current server tool reports `runtime_modes: ["server"]`; local-vault mode should appear in the server manifest only after the corresponding local runtime path is transport-addressable. The tool endpoint accepts `{ "input": { ... } }` and rejects every tool outside that read-only subset. Admin reads require an admin user or an admin-scoped service account API key.

Desktop/local-vault behavior is beginning in the client runtime, not the HTTP server. `apps/web/src/lib/mcpLocalRuntime.ts` currently executes `starlane.spaces.list`, `starlane.spaces.get`, `starlane.folders.list`, `starlane.documents.get`, `starlane.search.query`, `starlane.graph.get`, `starlane.tags.list`, `starlane.properties.list`, and `starlane.templates.list` through the local-vault `dataClient` and returns `runtime_mode: "local_vault"`. Document reads compose local metadata and content so the helper matches the server MCP document-read shape. Local search supports `q`, optional `space_id`, and a 1–100 `limit`. The helper exposes `localMcpRuntimeManifest()` with `transport_status: "helper_only"` so callers can discover local read support without implying stdio/SSE transport availability. The manifest also lists blocked catalog tools with reasons such as `write_not_enabled` and `not_implemented`, plus planned `stdio` and `sse` local transports marked unavailable with `transport_not_enabled`. It also exposes a disabled local capability-token policy, planned non-issuing token lifecycle policy, a non-issuing capability-token request challenge, `localMcpCapabilityTokenAuthReadiness()` for transport-facing auth readiness, `capability_token_auth_contract` for compact auth readiness, planned operation-journal policy, `operation_journal_contract` for compact journal readiness, `operation_journal_record_contract` for the planned disabled write-audit record shape, `transport_execution_contract` tying disabled transports to auth, journal, trace, read-tool, and blocked-write gates, `transport_envelope_contract` for disabled stdio/SSE request and response envelope fields, validation reasons, response reasons, stable error codes, and trace attachment, `transport_dispatch_contract` for helper-only read dispatch versus disabled stdio/SSE dispatch gates, `transport_error_contract` for mapping stable error codes to envelope/preflight stages, blocker priority, validation reasons, and the `tool_error_code` field, `transport_result_contract` clarifying helper read results while disabled transport responses never return tool results, `transport_retry_contract` for non-automatic retry semantics across invalid envelopes, disabled transports, token gates, blocked writes, not-implemented tools, and unknown tools, `transport_idempotency_contract` for request-ID, replay-cache, and write-idempotency boundaries while transport and journaling remain disabled, `write_block_contract` for disabled write execution, `localMcpTransportCapabilitySummary()` for disabled execution gates, `trace_contract` for the compact request-trace contract, and `request_trace_policy` with `schema_version: 1` describing preview-only, response-only, non-retained, non-persistent, non-journaled request traces for valid and invalid envelopes with input, confirmation text, and capability-token redaction, `value_policy: "presence_only"`, `raw_values_stored: false`, supported `stages`, `blocking_stages`, `primary_blocking_stages`, and `secondary_blocking_stages`. Transport-facing callers can use `executeLocalMcpTool(name, request)` to execute only discovered read tools by string name; unknown tools, unimplemented tools, and write tools are rejected before dispatch. `planLocalMcpToolExecution(name)` provides a side-effect-free safety plan for executable, confirmation-required, not-implemented, and unknown states before any transport executes a tool. `buildLocalMcpConfirmationChallenge(name)` defines the typed confirmation text required for blocked write tools, and `validateLocalMcpConfirmation(name, confirmationText)` validates exact challenge text without enabling write execution. `buildLocalMcpCapabilityTokenRequestChallenge()` defines the planned local token approval text and metadata fields, while `validateLocalMcpCapabilityTokenRequest(request)` validates request ID, timestamp, runtime mode, transport, scope, vault fingerprint, and confirmation text before returning `capability_token_not_enabled`; neither function issues or accepts a token. `localMcpCapabilityTokenLifecyclePolicy()` defines the planned local-vault metadata fields for future opaque bearer tokens, including token ID, issue/expiry times, runtime mode, transport, scope, vault fingerprint, SHA-256 token fingerprint, and revocation time; it still reports `token_issued: false`. `previewLocalMcpCapabilityTokenIssuance(request)` derives the would-be token record for a structurally valid request, including one-hour expiry metadata, without returning a raw token. `persistLocalMcpCapabilityTokenRecord(request)` persists that planned metadata with `accepted: false` and `raw_token_stored: false`; `issueLocalMcpCapabilityToken(request)` returns a one-time opaque token while persisting only its SHA-256 fingerprint; `listLocalMcpCapabilityTokenRecords()` reads normalized records, `findLocalMcpCapabilityTokenRecord(tokenId)` looks up persisted records, and `clearLocalMcpCapabilityTokenRecords()` clears them without making any token valid. `previewLocalMcpCapabilityTokenRecordExpiry(tokenId, checkedAt)` and `previewLocalMcpCapabilityTokenRecordRevocation(request)` derive disabled lifecycle previews from persisted token records while still accepting and revoking no tokens. `localMcpCapabilityTokenAuthReadiness()` summarizes disabled transport readiness, token issuance/acceptance/revocation gates, request challenge metadata, lifecycle policy, and available preview helpers. `validateLocalMcpCapabilityTokenExpiry(request)` and `previewLocalMcpCapabilityTokenExpiry(request)` classify planned token records as active or expired without accepting them. `validateLocalMcpCapabilityTokenRevocationRequest(request)` and `previewLocalMcpCapabilityTokenRevocation(request)` validate the planned revocation request shape and derive the would-be revocation record while still reporting `token_revoked: false`. `guardLocalMcpToolExecution(name, request)` combines the safety plan, optional confirmation challenge/validation, and final executable/rejection status so future transports can preflight a request before dispatch; blocked write tools still report `write_execution_disabled` even when confirmation text is valid. `localMcpWriteExecutionReadiness(tool)` and `localMcpTransportReadiness(transport, tool)` group disabled write-execution, auth, journal, and transport gates for the requested tool without enabling transport dispatch. `preflightLocalMcpTransportRequest(transport, tool, request)` runs that guard for planned transports while reporting the transport and capability token as disabled, attaching auth readiness, operation-journal readiness, and compact transport-readiness summaries, and reporting `tool_error_code` for write-disabled, not-implemented, or unknown tools. `validateLocalMcpTransportRequestEnvelope(envelope)` rejects blank request IDs, unsupported transports, and blank tool names before preflight. `previewLocalMcpTransportRequestTrace(envelope, preflight)` derives a schema-versioned non-persistent request trace for valid disabled preflight responses, including request ID, tool, transport, `stage: "transport_preflight"`, primary `blocking_stage`, `blocking_priority: "primary"`, optional `secondary_blocking_stage: "tool_guard"`, reason, error codes, explicit `storage: "response_only"`, `retention: "none"`, `persisted: false` / `journaled: false` / `raw_values_stored: false` status, request-shape booleans for input/confirmation/capability-token presence, `value_policy: "presence_only"`, and a `redacted_fields` list without storing those values. `previewLocalMcpTransportRejectedRequestTrace(envelope, validation)` derives the same redacted, schema-versioned response-only trace shape for invalid transport envelopes without running preflight, with `stage: "envelope_validation"`, `blocking_stage: "envelope_validation"`, and `blocking_priority: "primary"`. `handleLocalMcpTransportRequestEnvelope(envelope)` wraps valid preflight plus request trace into a non-executing request/response shape for future stdio/SSE handlers and returns a non-executing `request_invalid` response with redacted trace metadata for invalid envelopes. Local envelope and preflight responses now include stable error codes such as `invalid_request`, `capability_token_required`, `capability_token_not_enabled`, `transport_disabled`, `write_execution_disabled`, `tool_not_implemented`, and `tool_unknown`. `previewLocalMcpOperationJournal(name)` exposes the planned journal fields and whether a write tool would require journaling, while `localMcpOperationJournalRecordContract()` defines the future journal record ID, timestamp source, local metadata storage, token fingerprinting, and raw-value handling; neither records any entry. This keeps local-vault MCP reads aligned with the same standalone data model while stdio/SSE exposure and broader read-tool parity are built.

The local manifest also exposes `transport_side_effect_contract`, which explicitly marks envelope validation, disabled transport preflight, helper read dispatch, blocked write execution, operation-journal handling, and replay-cache handling as non-mutating while local stdio/SSE transport remains disabled.

It also exposes `transport_adapter_contract`, which marks the planned stdio and SSE adapters as unavailable, non-bindable, non-request-accepting, and non-response-emitting until capability-token and operation-journal gates are implemented.

The local manifest also exposes `transport_lifecycle_contract`, which marks configure, bind, authenticate, preflight, dispatch, respond, journal, and shutdown stages disabled until stdio/SSE transport is intentionally enabled.

Local-vault MCP now includes durable operation-journal helpers for blocked write attempts: `appendLocalMcpOperationJournalRecord(request)` persists redacted write-attempt metadata under the `local-mcp-journal:<request_id>` idempotency key, `previewLocalMcpOperationJournalReplay(requestId)` reports whether that key already has a record, `listLocalMcpOperationJournalRecords()` reads normalized records, and `clearLocalMcpOperationJournalRecords()` clears them. These records store request ID, timestamp, transport, tool, target, confirmation-text presence, optional SHA-256 capability-token fingerprint, and disabled execution status without storing raw confirmation text or raw capability tokens. Repeated request IDs are treated as journal replays and return the existing record without overwriting it. Journaled preflight returns a compact confirmed-write closeout summary that classifies recorded, replayed, skipped, and blocked journal boundaries while keeping write execution disabled.

Auth-preview transport envelope handling now carries that journaled-preflight evidence for confirmed write requests, so accepted-token write envelopes expose token auth, dispatch blockers, idempotent journal records, and confirmed-write closeout together without enabling stdio/SSE dispatch or tool execution. Confirmed-write auth responses reuse the journaled preflight dispatch gate as their transport dispatch preview. Auth-preview preflight builds token acceptance from the already-computed auth gate to avoid duplicate token validation. Dispatch gates derive `dispatch_allowed` from blocker presence; default transport blockers still keep dispatch disabled, while an explicit transport-enable option lets accepted-token read requests become dispatch-allowed and execute read-only local-vault helpers through the same handler with dispatch-executed trace metadata.

`handleLocalMcpEnabledTransportRequestEnvelope(envelope, checkedAt)` is the minimum enabled local stdio/SSE request path. It does not start listeners or bind streams by itself; it enables the already-reviewed transport envelope path for a single request by reusing `handleLocalMcpTransportRequestEnvelopeWithAuthPreview(...)` with explicit transport enablement. Accepted-token read requests execute through the canonical dispatch gate and return the existing local-vault read helper result. Missing/unaccepted tokens remain blocked by token gates, and accepted confirmed writes only record the redacted journal/idempotency boundary while write execution remains disabled.

`apps/web/src/lib/mcpLocalTransportListener.ts` supplies the concrete listener adapters around that gate. `startLocalMcpStdioListener(...)` consumes one JSON envelope per host-provided stdin line and emits one redacted JSON response per stdout line. `startLocalMcpSseListener(...)` registers a host-provided request callback and emits complete `event: message` SSE frames. Both listeners support drain/stop, reject malformed or wrong-transport input with a safe `invalid_request` response, and never persist or return raw capability tokens. The embedding desktop/CLI or HTTP host remains responsible for creating the actual process streams or endpoint; accepted read tools execute, while writes remain journal-only and blocked from execution.

Local-vault MCP also includes durable capability-token metadata helpers: `persistLocalMcpCapabilityTokenRecord(request)` stores the same would-be token record produced by the issuance preview, keyed by `local-mcp-token:<request_id>`, while `listLocalMcpCapabilityTokenRecords()`, `findLocalMcpCapabilityTokenRecord(tokenId)`, and `clearLocalMcpCapabilityTokenRecords()` manage the normalized records. `issueLocalMcpCapabilityToken(request)` returns a one-time opaque token to the caller and persists only its SHA-256 fingerprint with `accepted: false` and `raw_token_stored: false`. `previewLocalMcpCapabilityTokenValidation(token, checkedAt)` hashes a supplied token, matches it against persisted fingerprints, derives expiry/revocation state, and reports persisted accepted-token state without enabling dispatch; revoked records report `capability_token_revoked`. `previewLocalMcpCapabilityTokenAuthGate(token, checkedAt)` builds on that validation preview to report whether token checks pass and whether the matching record has been accepted while still returning `transport_dispatch_allowed: false`. `previewLocalMcpCapabilityTokenAcceptanceGate(token, checkedAt)` treats explicitly accepted records as satisfying token acceptance, leaving listener binding and transport dispatch blocked by the existing stdio/SSE transport gate. `preflightLocalMcpTransportRequestWithAuthPreview(transport, tool, request, checkedAt)` returns that acceptance-gate preview alongside token validation and auth-gate metadata, and `handleLocalMcpTransportRequestEnvelopeWithAuthPreview(envelope, checkedAt)` exposes it on valid non-executing envelope responses while preserving invalid-envelope rejection behavior. `previewLocalMcpTransportDispatchGate(envelope, checkedAt)` combines envelope validation, the token auth gate, token acceptance gate, transport readiness, operation-journal readiness, and the tool guard into a single non-executing dispatch decision with explicit blockers before any future stdio/SSE handler can execute. `localMcpTransportListenerConfigContract()` and `previewLocalMcpTransportListenerConfig(transport, tool)` define the disabled listener configuration shape for stdio stream fields and SSE endpoint/header fields before any adapter can bind. `validateLocalMcpTransportListenerConfig(request)` rejects unsupported transports, missing stdio streams, missing SSE endpoints, and non-GET SSE methods before returning `listener_configuration_not_enabled` for structurally valid disabled configs; `previewLocalMcpTransportListenerConfigValidation(request, checkedAt, options)` now attaches the dispatch-backed token-aware bind preflight plus a non-starting listener bind descriptor, disabled startup handoff, and request-loop registration evidence for those structurally valid configs without starting listeners or registering loops. `previewLocalMcpTransportAdapterBindPreflight(transport, tool)` reports disabled stdio/SSE bind readiness without attempting a bind, starting listeners, starting request loops, accepting requests, or emitting responses, while attaching the current transport, capability-token, and operation-journal readiness blockers. `previewLocalMcpTransportAdapterBindPreflightWithAcceptance(transport, tool, request, checkedAt, options)` now derives its first bind blocker from the canonical dispatch gate, so missing tokens block on capability-token evidence, issued-but-unaccepted tokens block on token acceptance, and explicitly accepted read tokens with `{ transport_enabled: true }` surface dispatch-allowed evidence without binding listeners. Adapter request, response, lifecycle, and runtime-state previews now also accept the same explicit transport-enable options, so accepted-read adapter evidence can surface dispatch-allowed state while response emission and runtime activation remain disabled. Runtime-state previews reuse token-aware bind/startup evidence under explicit transport enablement while keeping listener binding, active runtime state, shutdown, and shell responses closed. `previewLocalMcpCapabilityTokenRecordExpiry(tokenId, checkedAt)` derives active/expired preview metadata from a stored record, and `previewLocalMcpCapabilityTokenRecordRevocation(request)` verifies the stored record exists before deriving the disabled revocation preview. `revokeLocalMcpCapabilityTokenRecord(request)` persists `revoked_at` only after token ID and fingerprint checks, clears `accepted`, and stores no raw token. These records do not change the synchronous `validateLocalMcpCapabilityToken()` preflight yet; stdio/SSE dispatch remains disabled until transport and journal gates are wired together.

`previewLocalMcpTransportRequestLoopRegistration(transport, tool, request, checkedAt, options)` composes token-aware startup, bind evidence, and the canonical dispatch gate into a disabled stdio/SSE request-loop registration boundary, reporting no registered loop, no started loop, no accepted requests, no emitted responses, and the first blocker before any future handler can accept messages. It passes explicit transport-enable options through startup and bind preflight so accepted-read registration evidence stays aligned with request-loop dispatch.

`previewLocalMcpTransportRequestLoopDispatch(envelope, checkedAt, options)` composes the disabled request-loop registration boundary with the canonical dispatch gate. By default it reports no dispatch attempt, no accepted request, no handler invocation, no response emission, no execution, and no journal entry. With explicit `{ transport_enabled: true }`, accepted-token read requests can invoke `handleLocalMcpTransportRequestEnvelopeWithAuthPreview(...)` through the same dispatch gate and return the handler response. Accepted confirmed writes can invoke the same handler to record the redacted journal boundary, while listener registration, listener startup, response emission, and write execution remain disabled.

`previewLocalMcpTransportHandlerInvocation(envelope, checkedAt)` composes request-loop dispatch evidence into a disabled handler boundary, reporting no handler availability, no request acceptance, no handler invocation, no tool execution, no response emission, and no journal entry before future stdio/SSE handlers are wired.

`previewLocalMcpTransportHandlerResponseFrame(envelope, checkedAt, options)` composes handler-invocation evidence into a response-frame boundary. By default it reports no handler result, no response frame, no response envelope creation, no emitted response, no execution, and no journal entry. With explicit transport enablement, accepted-token reads reuse the existing handler response to create a response frame/envelope while still emitting no stdout/SSE output.

`previewLocalMcpTransportResponseEmission(envelope, checkedAt, options)` composes handler response-frame evidence into the transport-output boundary. With explicit transport enablement, accepted-token reads can surface an available response frame/envelope and mark the selected stdio/SSE output path as emitted. Missing-token, unaccepted-token, write, and default-disabled paths still report no emitted response.

`previewLocalMcpTransportResponseCompletion(envelope, checkedAt, options)` composes response-emission evidence into the completion boundary. By default it reports no completed response, request, lifecycle, cleanup scheduling, execution, or journal entry. With explicit transport enablement, accepted-token reads that emitted stdio/SSE output now report completed response/request evidence while lifecycle completion and cleanup scheduling remain disabled.

`previewLocalMcpTransportErrorEnvelopeFrame(envelope, checkedAt)` composes response-completion evidence into a disabled error-envelope framing boundary, reporting detected blocked/error state without creating or emitting an error envelope, completing the request, executing, or journaling.

`previewLocalMcpTransportErrorEmission(envelope, checkedAt)` composes error-envelope framing evidence into a disabled transport error-output boundary, reporting no stderr write, no SSE error event, no emitted error envelope, no request completion, no execution, and no journal entry.

`previewLocalMcpTransportLifecycleAudit(envelope, checkedAt, options)` composes response-completion and error-emission evidence into the lifecycle-audit boundary. With explicit transport enablement, accepted-token reads can report a completed response path and completed request evidence, while error paths, lifecycle completion, cleanup scheduling, audit records, and writes remain disabled.

`previewLocalMcpTransportRequestCloseout(envelope, checkedAt, options)` composes lifecycle-audit evidence into the request closeout and cleanup-queue handoff boundary. With explicit transport enablement, accepted-token reads with completed request evidence now report closeout attempted, while cleanup queue handoff/enqueue, lifecycle completion, cleanup scheduling, confirmed writes, and write execution remain disabled.

`previewLocalMcpTransportTerminalCleanup(envelope, checkedAt, options)` composes request-closeout evidence into the terminal cleanup and adapter closeout handoff boundary. With explicit transport enablement, accepted-token reads with closeout evidence now report terminal cleanup attempted, while adapter closeout handoff, cleanup queues, lifecycle completion, confirmed writes, and write execution remain disabled.

`previewLocalMcpTransportHandlerTerminalState(envelope, checkedAt, options)` composes terminal-cleanup evidence into the handler terminal-state summary. By default it reports no terminal-state record, handled request, accepted request, response emission, request completion, execution, or journal entry. With explicit transport enablement, accepted-token reads can report handled/request-accepted/response-emitted/request-completed/executed evidence from the same terminal-cleanup path while terminal-state persistence, lifecycle completion, cleanup queues, journaling, confirmed writes, and write execution remain closed. `previewLocalMcpTransportHandlerSkeleton(envelope, checkedAt, options)` passes the same explicit transport-enable option through its composed bind, dispatch, response, cleanup, terminal-state, readiness, and runtime evidence so skeleton previews no longer pin accepted-read paths to stale default-disabled substate.

`previewLocalMcpTransportEnablementReadiness(envelope, checkedAt, options)` composes the canonical dispatch gate with terminal-cleanup evidence. By default it reports disabled readiness. With explicit transport enablement, accepted-token reads can report stdio/SSE request-loop, response, cleanup, and execution readiness from the same dispatch-allowed path, while confirmed writes, journaling, lifecycle completion, and cleanup queues remain closed.

`previewLocalMcpCapabilityTokenAcceptanceReadiness(envelope, checkedAt)` is the single local-vault token-acceptance readiness surface. It reports token checks, matching persisted record state, accepted persistence readiness, and the current listener/dispatch blockers without mutating records. `acceptLocalMcpCapabilityTokenRecord(envelope, checkedAt, options)` is the only accepted-token persistence path; without `acceptance_enabled: true` it leaves records unchanged, and with explicit enablement it stores `accepted: true`, ISO `accepted_at`, `raw_token_stored: false`, and `reason: "capability_token_accepted"`. Accepted records now satisfy the token acceptance/auth gate, but stdio/SSE dispatch still stops at the disabled transport gate.

`previewLocalMcpConfirmedWriteJournalGate(envelope)` reports whether a transport request is a write, whether its exact confirmation text is valid, whether a journal record would be required, and which planned operation-journal gate still blocks write execution without writing a record. `preflightLocalMcpTransportRequestWithJournal(envelope, timestamp)` wraps the existing disabled local transport preflight, attaches the canonical dispatch gate plus that journal gate preview, and persists a redacted operation-journal record only for confirmed write attempts with valid confirmation text. Read preflights, missing-confirmation writes, and confirmation-mismatch writes remain non-journaled, and the original synchronous `preflightLocalMcpTransportRequest()` remains available for compatibility.

| Mode | Authentication | Data boundary |
|---|---|---|
| Local vault | Desktop-issued local capability token | Current local vault only |
| Server | Starlane API token or OAuth-issued bearer token | Spaces and admin surfaces allowed by the token |

Tokens should be scoped. A full site-admin API token must not be required for normal document tools.

## Scopes

| Scope | Allows |
|---|---|
| `notes:read` | Read spaces, folders, documents, metadata, search, graph, templates, and workflow state |
| `notes:write` | Create/update documents, folders, tags, properties, templates, and workflow events |
| `notes:delete` | Delete documents, folders, tags, properties, templates, and webhook records |
| `notes:admin` | Manage users, groups, admin config, audit export, search reindex, webhooks, and notification SMTP settings |
| `notes:export` | Export local/server snapshots and downloadable content |
| `notes:import` | Preview and execute imports into a vault or server space |

## Tool Families

Tool names should be stable, namespaced, and explicit:

| Tool family | Example tools |
|---|---|
| Spaces | `starlane.spaces.list`, `starlane.spaces.get`, `starlane.spaces.create` |
| Folders | `starlane.folders.list`, `starlane.folders.create`, `starlane.folders.move` |
| Documents | `starlane.documents.get`, `starlane.documents.create`, `starlane.documents.update`, `starlane.documents.move`, `starlane.documents.delete` |
| Metadata | `starlane.tags.list`, `starlane.tags.apply`, `starlane.properties.set` |
| Templates | `starlane.templates.list`, `starlane.templates.create`, `starlane.templates.apply` |
| Search | `starlane.search.query`, `starlane.search.saved_views.list` |
| Graph | `starlane.graph.get` |
| Workflows | `starlane.workflows.queue`, `starlane.workflows.transition`, `starlane.workflows.record_event` |
| Imports/exports | `starlane.import.preview`, `starlane.import.execute`, `starlane.export.snapshot` |
| Admin | `starlane.admin.users.list`, `starlane.admin.groups.list`, `starlane.admin.webhooks.list`, `starlane.admin.audit.export` |

The initial server-side catalog intentionally starts smaller than the full target surface and covers representative read, write, delete, import/export, workflow, and admin tools. `mcp_server_manifest()` exposes the current scaffold as a JSON-serializable manifest with `transport_status: "read_only_http_tools"` and a summary of tool counts by family, scope, confirmation policy, and runtime mode so future transports can reuse the same reviewed tool, scope, confirmation, runtime, and input/output schema contract. Runtime work should continue by expanding read-only coverage, adding local-vault parity, and only then adding confirmed write tools.

Currently executable HTTP tools:

- `starlane.spaces.list`
- `starlane.spaces.get`
- `starlane.folders.list`
- `starlane.documents.get`
- `starlane.search.query`
- `starlane.graph.get`
- `starlane.tags.list`
- `starlane.properties.list`
- `starlane.templates.list`
- `starlane.admin.users.list`

Currently executable local-vault helper tools, including accepted-token enabled transport-envelope reads through `handleLocalMcpEnabledTransportRequestEnvelope(...)`:

- `starlane.spaces.list`
- `starlane.spaces.get`
- `starlane.folders.list`
- `starlane.documents.get`
- `starlane.search.query`
- `starlane.graph.get`
- `starlane.tags.list`
- `starlane.properties.list`
- `starlane.templates.list`

## Confirmation Boundaries

MCP tools must require explicit host confirmation before destructive or broad operations:

| Operation | Confirmation |
|---|---|
| Delete document, folder, template, tag, property, webhook, or group grant | Required |
| Bulk update more than 10 records | Required |
| Import execution after preview | Required |
| Snapshot export containing document bodies | Required |
| Admin config changes, role changes, user deactivate/reactivate | Required |
| Search reindex and webhook redelivery | Required |

Read-only tools, single-document drafts, and import previews should not require extra confirmation beyond normal tool approval.

## Audit and Logging

Server-mode MCP writes should reuse existing audit logging and include:

- `source: "mcp"`
- tool name
- actor user ID
- target type and target ID
- confirmation ID when a tool required confirmation

Local-vault mode should write a local operation journal beside the vault snapshot so offline changes can be inspected before server import. The current client helper only exposes `localMcpOperationJournalPolicy()`, `localMcpOperationJournalContract()`, and `previewLocalMcpOperationJournal(name)` for non-writing contract previews. The planned journal fields are ID, timestamp, runtime mode, transport, tool, operation, target, confirmation text, and capability-token fingerprint.

## Error Contract

Server-mode HTTP MCP endpoints use the shared REST JSON error envelope:

```json
{ "error": "forbidden", "message": "Admin access required" }
```

The current server HTTP path returns `401 unauthorized` when authentication is missing or invalid, `403 forbidden` when an authenticated principal lacks the required admin role, API-key scope, or read-only HTTP execution boundary, `400 bad_request` for invalid tool names/input, and `404 not_found` for missing readable resources.

Local-vault and future native MCP transports should map API and runtime failures into machine-readable MCP-local codes:

| Code | Meaning |
|---|---|
| `auth_required` | Token missing or expired |
| `scope_denied` | Token lacks required scope |
| `not_found` | Target does not exist or is outside scope |
| `conflict` | Locked, stale, or conflicting resource |
| `confirmation_required` | Operation needs host confirmation |
| `validation_failed` | Input failed schema validation |
| `server_unavailable` | Server or local vault is unavailable |

## Implementation Order

1. Read-only server tools for spaces, folders, documents, metadata, search, and graph.
2. Local-vault read-only parity over the standalone snapshot.
3. Safe write tools for documents, tags, properties, and workflow events.
4. Import/export tools with preview-before-execute.
5. Admin tools after audit coverage and confirmation boundaries are verified.
