import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  buildProviderRemoteEnablementPlan,
  buildProviderRemoteImportApplyPlan,
  buildProviderRemoteImportDryRun,
  buildProviderRemoteImportPreflight,
  buildProviderRemoteMutationEnvelope,
  buildProviderRemoteMutationPlan,
  buildProviderRemoteSyncManifest,
  buildProviderRemoteWriteRequestSpec,
  createProviderAdapter,
  executeProviderRemoteImportApplyPlan,
  executeProviderRemoteMutationEnvelope,
  executeProviderRemoteMutationEnvelopeWithTransport,
  getProviderRemoteImportUnsupportedMessage,
  isProviderRemoteImportOperationSupported,
} from "./providerAdapters";
import { writeProviderDefaultSyncValidationEvidence } from "./providerSession";

describe("providerAdapters", () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    (globalThis as { window?: unknown }).window = {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => {
          storage.set(key, value);
        },
        removeItem: (key: string) => {
          storage.delete(key);
        },
      },
    };
  });

  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
  });

  it("identifies provider remote import operation families supported by the local merge executor", () => {
    const supportedOperations: Array<[string, string]> = [
      ["space", "create"],
      ["space", "update"],
      ["document", "create"],
      ["document", "update"],
      ["document", "update_content"],
      ["document", "delete"],
      ["folder", "create"],
      ["folder", "update"],
      ["folder", "delete"],
      ["tag", "create"],
      ["tag", "update"],
      ["tag", "delete"],
      ["template", "create"],
      ["template", "update"],
      ["template", "delete"],
      ["workflow_template", "create"],
      ["workflow_template", "update"],
      ["workflow_template", "delete"],
      ["document_tag", "add"],
      ["document_tag", "remove"],
      ["property_schema", "create"],
      ["property_schema", "update"],
      ["property_schema", "reorder"],
      ["property_schema", "delete"],
      ["document_property_values", "set"],
      ["asset", "create"],
      ["asset", "delete"],
      ["document_workflow", "create"],
      ["document_workflow", "update"],
      ["document_workflow", "transition"],
      ["document_workflow", "delete"],
      ["document_workflow_event", "enable"],
      ["document_workflow_event", "transition"],
      ["document_workflow_event", "disable"],
      ["document_workflow_event", "delegate"],
      ["document_workflow_event", "escalate"],
    ];

    for (const [entityType, action] of supportedOperations) {
      expect(isProviderRemoteImportOperationSupported(entityType, action)).toBe(true);
    }

    expect(isProviderRemoteImportOperationSupported("comment", "create")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("space", "delete")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("document", "publish")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("document", "unpublish")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("document_reaction", "toggle")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("document_version", "restore")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("webhook", "create")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("notification", "read")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("api_key", "create")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("space_member", "add")).toBe(false);
    expect(isProviderRemoteImportOperationSupported("custom_block", "create")).toBe(false);
    expect(getProviderRemoteImportUnsupportedMessage("comment", "create")).toBe(
      "Remote replay operation comment:create is server-only and cannot be imported into a local vault",
    );
    expect(getProviderRemoteImportUnsupportedMessage("space", "delete")).toBe(
      "Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow",
    );
    expect(getProviderRemoteImportUnsupportedMessage("document", "publish")).toBe(
      "Remote replay operation document:publish is server-only and cannot be imported into a local vault",
    );
    expect(getProviderRemoteImportUnsupportedMessage("document", "unpublish")).toBe(
      "Remote replay operation document:unpublish is server-only and cannot be imported into a local vault",
    );
    expect(getProviderRemoteImportUnsupportedMessage("document_reaction", "toggle")).toBe(
      "Remote replay operation document_reaction:toggle is server-only and cannot be imported into a local vault",
    );
    expect(getProviderRemoteImportUnsupportedMessage("document_version", "restore")).toBe(
      "Remote replay operation document_version:restore is server-only and cannot be imported into a local vault",
    );
    expect(getProviderRemoteImportUnsupportedMessage("webhook", "create")).toBe(
      "Remote replay operation webhook:create is server-only and cannot be imported into a local vault",
    );
    expect(getProviderRemoteImportUnsupportedMessage("notification", "read")).toBe(
      "Remote replay operation notification:read is server-only and cannot be imported into a local vault",
    );
    expect(getProviderRemoteImportUnsupportedMessage("api_key", "create")).toBe(
      "Remote replay operation api_key:create is server-only and cannot be imported into a local vault",
    );
    expect(getProviderRemoteImportUnsupportedMessage("space_member", "add")).toBe(
      "Remote replay operation space_member:add is server-only and cannot be imported into a local vault",
    );
    expect(getProviderRemoteImportUnsupportedMessage("custom_block", "create")).toBe(
      "Remote replay operation custom_block:create is not supported by local provider import",
    );
  });

  it("returns no-op adapter for local-only provider", async () => {
    const adapter = createProviderAdapter("none");
    await expect(adapter.connect()).resolves.toBeUndefined();
    await expect(adapter.pull()).resolves.toMatchObject({
      pulled: 0,
      pushed: 0,
      conflicts: 0,
    });
    await expect(adapter.getStatus()).resolves.toMatchObject({ connected: true });
  });

  it("cloud provider requires connect before sync and then syncs", async () => {
    const gdrive = createProviderAdapter("google-drive");
    await expect(gdrive.pull()).rejects.toThrow("must be connected before pull");

    await gdrive.connect();
    await expect(gdrive.getStatus()).resolves.toMatchObject({
      connected: true,
      remote_contract: {
        status: "contract_only",
        root_strategy: "drive-appdata-or-user-selected-folder",
        write_model: "local-sqlite-first-then-provider-api-push",
      },
      remote_mutation_plans: [
        {
          direction: "pull",
          status: "dry_run_only",
          remote_mutation_enabled: false,
          dry_run: true,
        },
        {
          direction: "push",
          status: "dry_run_only",
          remote_mutation_enabled: false,
          dry_run: true,
        },
      ],
      session: {
        provider: "google-drive",
        connected: true,
        account_label: "Google account",
        sync_mode: "simulated",
      },
    });

    const result = await gdrive.push();
    expect(result.pushed).toBeGreaterThan(0);
    expect(result.remote_mutation_envelope).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      direction: "push",
      mutation_kind: "provider-sync-push",
      payload_format: "starlane-provider-sync-manifest+json",
      target_root_strategy: "drive-appdata-or-user-selected-folder",
      write_model: "local-sqlite-first-then-provider-api-push",
      dry_run: true,
      remote_mutation_enabled: false,
      blockers: [],
    });
    expect(result.remote_mutation_attempt).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      direction: "push",
      status: "dry_run",
      transport_kind: "contract-only",
      applied: false,
      blockers: [],
      request_spec: {
        request_kind: "google-drive-file-upsert",
        method: "POST",
        endpoint: "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart",
        remote_path: "Starlane Notes/provider-sync-manifest.json",
        idempotency_key: result.remote_mutation_envelope?.mutation_id,
      },
    });
    await expect(gdrive.getStatus()).resolves.toMatchObject({ connected: true });

    await gdrive.disconnect();
    await expect(gdrive.getStatus()).resolves.toMatchObject({ connected: false });
  });

  it("returns a blocked transport attempt for manual remote sync mode", async () => {
    const icloud = createProviderAdapter("icloud-drive");
    await icloud.connect();

    const manualSession = {
      provider: "icloud-drive",
      connected: true,
      auth: {
        access_token: "manual-access",
        refresh_token: "manual-refresh",
        account_label: "iCloud account",
        scopes: ["icloud.documents"],
        issued_at: new Date().toISOString(),
        expires_at: new Date(Date.now() + 60_000).toISOString(),
      },
      sync_mode: "manual",
      last_operation: "connect",
    };

    window.localStorage.setItem(
      "starlane_provider_session_icloud-drive",
      JSON.stringify(manualSession),
    );

    const result = await icloud.pull();
    expect(result.pulled).toBe(0);
    expect(result.remote_mutation_envelope).toMatchObject({
      provider: "icloud-drive",
      direction: "pull",
      dry_run: true,
      remote_mutation_enabled: false,
      blockers: ["iCloud Drive remote pull mutation is contract-only and not enabled yet"],
    });
    expect(result.remote_mutation_attempt).toMatchObject({
      provider: "icloud-drive",
      direction: "pull",
      status: "blocked",
      transport_kind: "provider-file",
      applied: false,
      blockers: ["iCloud Drive remote pull mutation is contract-only and not enabled yet"],
    });
  });

  it("describes iCloud remote contract without enabling real mutation", async () => {
    const icloud = createProviderAdapter("icloud-drive");
    const status = await icloud.getStatus();

    expect(status.remote_contract).toMatchObject({
      status: "contract_only",
      root_strategy: "native-ubiquity-documents-container",
      auth_model: "apple-account-via-native-container",
      write_model: "local-sqlite-first-then-provider-file-push",
      conflict_model: "provider-version-plus-local-replay-queue-validation",
    });
    expect(status.remote_contract.required_scopes).toContain("icloud.documents");
  });

  it("blocks real remote mutation plans until provider contracts are enabled", () => {
    const plan = buildProviderRemoteMutationPlan(
      "google-drive",
      "Google Drive",
      "push",
      {
        status: "contract_only",
        root_strategy: "drive-appdata-or-user-selected-folder",
        auth_model: "oauth2-offline-access",
        required_scopes: ["drive.file"],
        write_model: "local-sqlite-first-then-provider-api-push",
        conflict_model: "provider-revision-plus-local-replay-queue-validation",
      },
      {
        provider: "google-drive",
        connected: true,
        account_label: "Google account",
        scopes: ["drive.file"],
        sync_mode: "manual",
      },
    );

    expect(plan).toMatchObject({
      status: "blocked",
      remote_mutation_enabled: false,
      dry_run: true,
      blockers: ["Google Drive remote push mutation is contract-only and not enabled yet"],
    });
  });

  it("blocks provider remote enablement without a real manual session", () => {
    const plan = buildProviderRemoteEnablementPlan(
      "google-drive",
      "Google Drive",
      {
        status: "contract_only",
        root_strategy: "drive-appdata-or-user-selected-folder",
        auth_model: "oauth2-offline-access",
        required_scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        write_model: "local-sqlite-first-then-provider-api-push",
        conflict_model: "provider-revision-plus-local-replay-queue-validation",
      },
      {
        provider: "google-drive",
        connected: true,
        account_label: "Google account",
        scopes: ["drive.file"],
        sync_mode: "simulated",
      },
    );

    expect(plan).toMatchObject({
      provider: "google-drive",
      status: "blocked",
      transport_kind: "provider-api",
      enablement_boundary: "explicit_manual_provider_session",
      default_sync_enabled: false,
      blockers: [
        "Google Drive requires a real manual provider session before remote sync enablement",
        "Google Drive remote sync missing scopes: drive.metadata.readonly, offline_access",
        "Google Drive remote sync requires a refresh token",
        "Google Drive remote sync requires a selected Drive root folder",
      ],
    });
  });

  it("blocks provider remote enablement without a refresh-capable Google Drive session", () => {
    const plan = buildProviderRemoteEnablementPlan(
      "google-drive",
      "Google Drive",
      {
        status: "contract_only",
        root_strategy: "drive-appdata-or-user-selected-folder",
        auth_model: "oauth2-offline-access",
        required_scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        write_model: "local-sqlite-first-then-provider-api-push",
        conflict_model: "provider-revision-plus-local-replay-queue-validation",
      },
      {
        provider: "google-drive",
        connected: true,
        account_label: "Google account",
        scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        sync_mode: "manual",
        expires_at: "2027-07-09T12:00:00.000Z",
        refresh_token_ready: false,
        provider_root_id: "drive-root-folder-1",
      },
      new Date("2026-07-09T12:00:00.000Z"),
    );

    expect(plan).toMatchObject({
      provider: "google-drive",
      status: "blocked",
      blockers: ["Google Drive remote sync requires a refresh token"],
    });
  });

  it("reports provider remote enablement ready for scoped manual Google Drive sessions", async () => {
    const gdrive = createProviderAdapter("google-drive");
    window.localStorage.setItem(
      "starlane_provider_session_google-drive",
      JSON.stringify({
        provider: "google-drive",
        connected: true,
        auth: {
          access_token: "google-drive-access-real",
          refresh_token: "google-drive-refresh-real",
          account_label: "Google account",
          scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
          provider_root_id: "drive-root-folder-1",
          provider_root_name: "Starlane Notes",
          issued_at: "2026-07-08T12:00:00.000Z",
          expires_at: "2027-07-09T12:00:00.000Z",
        },
        sync_mode: "manual",
      }),
    );

    await expect(gdrive.getStatus()).resolves.toMatchObject({
      connected: true,
      remote_contract: {
        status: "contract_only",
      },
      remote_sync_enablement: {
        enabled: false,
      },
      session: {
        sync_mode: "manual",
        provider_root_id: "drive-root-folder-1",
      },
      remote_enablement: {
        status: "ready_to_enable",
        transport_kind: "provider-api",
        enablement_boundary: "explicit_manual_provider_session",
        default_sync_enabled: false,
        blockers: [],
      },
      default_sync_policy: {
        status: "manual_only",
        default_sync_enabled: false,
        automatic_enablement_allowed: false,
        policy_boundary: "explicit_manual_provider_session",
        blockers: [
          "Google Drive remote sync must be explicitly enabled before default sync can be considered",
          "Google Drive default sync requires production OAuth validation",
          "Google Drive default sync requires device-executed background wake validation",
        ],
        readiness_gates: [
          {
            gate: "manual_provider_session",
            passed: true,
          },
          {
            gate: "explicit_remote_enablement",
            passed: false,
          },
          {
            gate: "background_transport_wake",
            passed: false,
          },
          {
            gate: "production_oauth_validation",
            passed: false,
          },
          {
            gate: "device_executed_wake_validation",
            passed: false,
          },
        ],
        validation_evidence: {
          production_oauth_verified: false,
          device_background_wake_verified: false,
          notes: [
            "Google Drive production OAuth completion must be verified outside fixtures before default sync can be enabled.",
            "Google Drive OS-delivered native background wake execution must be recorded on device before default sync can be enabled.",
          ],
        },
      },
    });
  });

  it("persists explicit provider remote sync enablement and can disable it", async () => {
    const gdrive = createProviderAdapter("google-drive");
    window.localStorage.setItem(
      "starlane_provider_session_google-drive",
      JSON.stringify({
        provider: "google-drive",
        connected: true,
        auth: {
          access_token: "google-drive-access-real",
          refresh_token: "google-drive-refresh-real",
          account_label: "Google account",
          scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
          provider_root_id: "drive-root-folder-1",
          provider_root_name: "Starlane Notes",
          issued_at: "2026-07-08T12:00:00.000Z",
          expires_at: "2027-07-09T12:00:00.000Z",
        },
        sync_mode: "manual",
      }),
    );

    await gdrive.enableRemoteSync();

    await expect(gdrive.getStatus()).resolves.toMatchObject({
      remote_contract: {
        status: "enabled",
      },
      remote_sync_enablement: {
        enabled: true,
        enablement_boundary: "explicit_manual_provider_session",
      },
      default_sync_policy: {
        status: "eligible_after_explicit_enablement",
        default_sync_enabled: false,
        automatic_enablement_allowed: false,
        blockers: [
          "Google Drive default sync requires production OAuth validation",
          "Google Drive default sync requires device-executed background wake validation",
        ],
        readiness_gates: [
          {
            gate: "manual_provider_session",
            passed: true,
          },
          {
            gate: "explicit_remote_enablement",
            passed: true,
          },
          {
            gate: "background_transport_wake",
            passed: true,
          },
          {
            gate: "production_oauth_validation",
            passed: false,
          },
          {
            gate: "device_executed_wake_validation",
            passed: false,
          },
        ],
        validation_evidence: {
          production_oauth_verified: false,
          device_background_wake_verified: false,
        },
      },
      remote_mutation_plans: [
        {
          direction: "pull",
          status: "ready",
          remote_mutation_enabled: true,
          dry_run: false,
        },
        {
          direction: "push",
          status: "ready",
          remote_mutation_enabled: true,
          dry_run: false,
        },
      ],
    });

    await gdrive.disableRemoteSync("test disabled");

    await expect(gdrive.getStatus()).resolves.toMatchObject({
      remote_contract: {
        status: "contract_only",
      },
      remote_sync_enablement: {
        enabled: false,
        reason: "test disabled",
      },
      default_sync_policy: {
        status: "manual_only",
        default_sync_enabled: false,
      },
    });
  });

  it("treats stored Google Drive remote sync enablement as disabled when refresh token readiness regresses", async () => {
    const gdrive = createProviderAdapter("google-drive");
    window.localStorage.setItem(
      "starlane_provider_session_google-drive",
      JSON.stringify({
        provider: "google-drive",
        connected: true,
        auth: {
          access_token: "google-drive-access-real",
          refresh_token: "google-drive-refresh-real",
          account_label: "Google account",
          scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
          provider_root_id: "drive-root-folder-1",
          provider_root_name: "Starlane Notes",
          issued_at: "2026-07-08T12:00:00.000Z",
          expires_at: "2027-07-09T12:00:00.000Z",
        },
        sync_mode: "manual",
      }),
    );

    await gdrive.enableRemoteSync();

    window.localStorage.setItem(
      "starlane_provider_session_google-drive",
      JSON.stringify({
        provider: "google-drive",
        connected: true,
        auth: {
          access_token: "google-drive-access-real",
          refresh_token: "",
          account_label: "Google account",
          scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
          provider_root_id: "drive-root-folder-1",
          provider_root_name: "Starlane Notes",
          issued_at: "2026-07-08T12:00:00.000Z",
          expires_at: "2027-07-09T12:00:00.000Z",
        },
        sync_mode: "manual",
      }),
    );

    await expect(gdrive.getStatus()).resolves.toMatchObject({
      remote_contract: {
        status: "contract_only",
      },
      session: {
        refresh_token_ready: false,
      },
      remote_enablement: {
        status: "blocked",
        blockers: ["Google Drive remote sync requires a refresh token"],
      },
      remote_sync_enablement: {
        enabled: false,
        reason: "stored enablement is blocked by current provider readiness",
      },
      default_sync_policy: {
        status: "manual_only",
        readiness_gates: [
          {
            gate: "manual_provider_session",
            passed: false,
            detail: "Google Drive manual session is not ready for provider transport",
          },
          {
            gate: "explicit_remote_enablement",
            passed: false,
            detail: "Google Drive remote sync has not been explicitly enabled",
          },
          {
            gate: "background_transport_wake",
            passed: false,
            detail: "Google Drive background provider attempts wait for explicit remote enablement",
          },
          {
            gate: "production_oauth_validation",
            passed: false,
            detail: "No production OAuth validation has been recorded",
          },
          {
            gate: "device_executed_wake_validation",
            passed: false,
            detail: "No OS-delivered device background wake validation has been recorded",
          },
        ],
      },
      remote_mutation_plans: [
        {
          direction: "pull",
          status: "blocked",
          remote_mutation_enabled: false,
          dry_run: true,
        },
        {
          direction: "push",
          status: "blocked",
          remote_mutation_enabled: false,
          dry_run: true,
        },
      ],
    });
  });

  it("uses stored production OAuth evidence while keeping device wake validation blocked", async () => {
    const gdrive = createProviderAdapter("google-drive");
    window.localStorage.setItem(
      "starlane_provider_session_google-drive",
      JSON.stringify({
        provider: "google-drive",
        connected: true,
        auth: {
          access_token: "google-drive-access-real",
          refresh_token: "google-drive-refresh-real",
          account_label: "Google account",
          scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
          provider_root_id: "drive-root-folder-1",
          provider_root_name: "Starlane Notes",
          issued_at: "2026-07-08T12:00:00.000Z",
          expires_at: "2027-07-09T12:00:00.000Z",
        },
        sync_mode: "manual",
      }),
    );
    writeProviderDefaultSyncValidationEvidence("google-drive", {
      provider: "google-drive",
      production_oauth_verified: true,
      device_background_wake_verified: false,
      production_oauth_verified_at: "2026-07-09T12:00:00.000Z",
      notes: ["Live OAuth completed"],
    });

    await gdrive.enableRemoteSync();

    await expect(gdrive.getStatus()).resolves.toMatchObject({
      default_sync_policy: {
        automatic_enablement_allowed: false,
        blockers: [
          "Google Drive default sync requires device-executed background wake validation",
        ],
        readiness_gates: [
          {
            gate: "manual_provider_session",
            passed: true,
          },
          {
            gate: "explicit_remote_enablement",
            passed: true,
          },
          {
            gate: "background_transport_wake",
            passed: true,
          },
          {
            gate: "production_oauth_validation",
            passed: true,
          },
          {
            gate: "device_executed_wake_validation",
            passed: false,
          },
        ],
        validation_evidence: {
          production_oauth_verified: true,
          device_background_wake_verified: false,
          production_oauth_verified_at: "2026-07-09T12:00:00.000Z",
          notes: [
            "Live OAuth completed",
            "Google Drive OS-delivered native background wake execution must be recorded on device before default sync can be enabled.",
          ],
        },
      },
    });
  });

  it("blocks explicit remote sync enablement when readiness is incomplete", async () => {
    const gdrive = createProviderAdapter("google-drive");
    await gdrive.connect();

    await expect(gdrive.enableRemoteSync()).rejects.toThrow(
      "Provider remote sync is not ready to enable",
    );
    await expect(gdrive.getStatus()).resolves.toMatchObject({
      remote_sync_enablement: {
        enabled: false,
      },
    });
  });

  it("builds deterministic provider remote mutation envelopes", () => {
    const plan = buildProviderRemoteMutationPlan(
      "icloud-drive",
      "iCloud Drive",
      "pull",
      {
        status: "enabled",
        root_strategy: "native-ubiquity-documents-container",
        auth_model: "apple-account-via-native-container",
        required_scopes: ["icloud.documents"],
        write_model: "local-sqlite-first-then-provider-file-push",
        conflict_model: "provider-version-plus-local-replay-queue-validation",
      },
      {
        provider: "icloud-drive",
        connected: true,
        account_label: "iCloud account",
        scopes: ["icloud.documents"],
        sync_mode: "manual",
      },
    );

    expect(
      buildProviderRemoteMutationEnvelope(
        plan,
        "2026-07-08T12:00:00.000Z",
        "mutation-1",
      ),
    ).toEqual({
      schema_version: 1,
      provider: "icloud-drive",
      direction: "pull",
      mutation_id: "mutation-1",
      mutation_kind: "provider-sync-pull",
      payload_format: "starlane-provider-sync-manifest+json",
      target_root_strategy: "native-ubiquity-documents-container",
      write_model: "local-sqlite-first-then-provider-file-push",
      conflict_model: "provider-version-plus-local-replay-queue-validation",
      required_scopes: ["icloud.documents"],
      remote_mutation_enabled: true,
      dry_run: false,
      blockers: [],
      created_at: "2026-07-08T12:00:00.000Z",
    });
  });

  it("blocks provider import preflight when a provider wrapper has no replay payload", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-no-replay-payload",
    );

    expect(buildProviderRemoteImportPreflight(buildProviderRemoteSyncManifest(envelope))).toEqual({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: false,
      operation_count: 0,
      ready_operation_count: undefined,
      missing_payload_count: undefined,
      blockers: ["Provider-sync manifest does not include a queued replay manifest payload"],
    });
  });

  it("marks replay-bearing provider import preflight ready for review", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-review-ready",
    );

    expect(
      buildProviderRemoteImportPreflight(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 1,
          operation_summary: {
            ready_operation_count: 1,
            missing_payload_count: 0,
          },
          operations: [
            {
              id: "remote-ready",
              entity_type: "document",
              entity_id: "doc-1",
              action: "update_content",
              payload_status: "available",
            },
          ],
        }),
      ),
    ).toEqual({
      schema_version: 1,
      provider: "google-drive",
      status: "ready_to_review",
      import_ready: true,
      replay_manifest_present: true,
      operation_count: 1,
      ready_operation_count: 1,
      missing_payload_count: 0,
      blockers: [],
    });
  });

  it("blocks provider import preflight when replay operation counts do not match", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-review-count-mismatch",
    );

    expect(
      buildProviderRemoteImportPreflight(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 2,
          operations: [
            {
              id: "remote-ready",
              entity_type: "document",
              entity_id: "doc-1",
              action: "update_content",
              payload_status: "available",
            },
          ],
        }),
      ),
    ).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: true,
      operation_count: 2,
      blockers: [
        "Provider-sync replay operation count mismatch: declared 2, found 1",
      ],
    });
  });

  it("blocks provider import preflight when replay operation counts are malformed", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-review-malformed-counts",
    );

    expect(
      buildProviderRemoteImportPreflight(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 1.5,
          operation_summary: {
            ready_operation_count: 1.25,
            missing_payload_count: "0",
          },
          operations: [
            {
              id: "remote-ready",
              entity_type: "document",
              entity_id: "doc-1",
              action: "update_content",
              payload_status: "available",
            },
          ],
        }),
      ),
    ).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: true,
      operation_count: 0,
      ready_operation_count: undefined,
      missing_payload_count: undefined,
      blockers: expect.arrayContaining([
        "Provider-sync replay operation count is invalid",
        "Provider-sync replay ready operation count is invalid",
        "Provider-sync replay missing payload count is invalid",
      ]),
    });
  });

  it("blocks provider import preflight when replay operations list is malformed", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-review-malformed-operations",
    );

    expect(
      buildProviderRemoteImportPreflight(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 0,
          operation_summary: {
            ready_operation_count: 0,
            missing_payload_count: 0,
          },
          operations: { id: "remote-ready" },
        }),
      ),
    ).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: true,
      operation_count: 0,
      ready_operation_count: 0,
      missing_payload_count: 0,
      blockers: expect.arrayContaining(["Provider-sync replay operations list is invalid"]),
    });
  });

  it("blocks provider import preflight when replay operation summary is malformed", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-review-malformed-summary",
    );

    expect(
      buildProviderRemoteImportPreflight(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 0,
          operation_summary: [],
          operations: [],
        }),
      ),
    ).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: true,
      operation_count: 0,
      ready_operation_count: undefined,
      missing_payload_count: undefined,
      blockers: expect.arrayContaining(["Provider-sync replay operation summary is invalid"]),
    });
  });

  it("does not select remote replay imports when preflight is blocked", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-dry-run-count-mismatch",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-ready",
            entity_type: "document",
            entity_id: "doc-1",
            action: "update_content",
            payload_status: "available",
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      status: "blocked",
      import_ready: false,
      operation_count: 1,
      would_apply_count: 0,
      failed_count: 1,
      operations: [
        {
          operation_id: "remote-ready",
          status: "failed",
          message:
            "Provider import preflight is blocked: Provider-sync replay operation count mismatch: declared 2, found 1",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 1,
      operations: [
        {
          operation_id: "remote-ready",
          apply_status: "skipped_failed",
        },
      ],
    });

    const recoveryDryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-space-delete",
            entity_type: "space",
            entity_id: "space-1",
            action: "delete",
            payload_status: "available",
            payload: { id: "space-1" },
          },
        ],
      }),
      [],
      [],
      { allowDestructiveSpaceRecovery: true },
    );
    expect(recoveryDryRun).toMatchObject({
      would_apply_count: 1,
      unsupported_count: 0,
      operations: [{ operation_id: "remote-space-delete", status: "would_apply" }],
    });
  });

  it("blocks provider import preflight when replay operation summaries do not match", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-review-summary-mismatch",
    );

    expect(
      buildProviderRemoteImportPreflight(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 2,
          operation_summary: {
            ready_operation_count: 1,
            missing_payload_count: 0,
          },
          operations: [
            {
              id: "remote-ready",
              entity_type: "document",
              entity_id: "doc-1",
              action: "update_content",
              payload_status: "available",
            },
            {
              id: "remote-missing",
              entity_type: "asset",
              entity_id: "asset-1",
              action: "create",
              payload_status: "missing",
            },
          ],
        }),
      ),
    ).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: true,
      operation_count: 2,
      ready_operation_count: 1,
      missing_payload_count: 0,
      blockers: expect.arrayContaining([
        "Provider-sync replay summary mismatch: ready 1 plus missing 0 does not equal 2",
        "Provider-sync replay missing payload count mismatch: summary 0, found 1",
      ]),
    });
  });

  it("blocks provider import preflight when replay payload status counts do not match", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-review-status-mismatch",
    );

    expect(
      buildProviderRemoteImportPreflight(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 2,
          operation_summary: {
            ready_operation_count: 2,
            missing_payload_count: 0,
          },
          operations: [
            {
              id: "remote-ready",
              entity_type: "document",
              entity_id: "doc-1",
              action: "update_content",
              payload_status: "available",
            },
            {
              id: "remote-missing",
              entity_type: "asset",
              entity_id: "asset-1",
              action: "create",
              payload_status: "missing",
            },
          ],
        }),
      ),
    ).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: true,
      operation_count: 2,
      ready_operation_count: 2,
      missing_payload_count: 0,
      blockers: expect.arrayContaining([
        "Provider-sync replay ready count mismatch: summary 2, found 1",
        "Provider-sync replay missing payload count mismatch: summary 0, found 1",
      ]),
    });
  });

  it("blocks provider import preflight when replay payload statuses are malformed", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-review-malformed-status",
    );

    expect(
      buildProviderRemoteImportPreflight(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 2,
          operation_summary: {
            ready_operation_count: 1,
            missing_payload_count: 1,
          },
          operations: [
            {
              id: "remote-padded-status",
              entity_type: "document",
              entity_id: "doc-1",
              action: "update_content",
              payload_status: "available ",
            },
            {
              id: "remote-unknown-status",
              entity_type: "document",
              entity_id: "doc-2",
              action: "update_content",
              payload_status: "queued",
            },
          ],
        }),
      ),
    ).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: true,
      operation_count: 2,
      ready_operation_count: 1,
      missing_payload_count: 1,
      blockers: expect.arrayContaining([
        "Provider-sync replay payload status is invalid",
        "Provider-sync replay ready count mismatch: summary 1, found 0",
        "Provider-sync replay missing payload count mismatch: summary 1, found 0",
      ]),
    });
  });

  it("blocks provider import preflight when replay operation identities are padded", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-review-padded-identity",
    );

    expect(
      buildProviderRemoteImportPreflight(
        buildProviderRemoteSyncManifest(envelope, {
          schema_version: 1,
          operation_count: 1,
          operation_summary: {
            ready_operation_count: 1,
            missing_payload_count: 0,
          },
          operations: [
            {
              id: " remote-padded ",
              entity_type: "document",
              entity_id: "doc-1",
              action: "update_content",
              payload_status: "available",
            },
          ],
        }),
      ),
    ).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      replay_manifest_present: true,
      operation_count: 1,
      ready_operation_count: 1,
      missing_payload_count: 0,
      blockers: expect.arrayContaining([
        "Provider-sync replay operation identity fields are padded",
      ]),
    });
  });

  it("dry-runs remote provider replay imports without mutating local data", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-dry-run",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 6,
      operations: [
        {
          id: "remote-ready",
          entity_type: "document",
          entity_id: "doc-1",
          action: "update_content",
          payload_status: "available",
          payload: {
            id: "doc-1",
            space_id: "space-1",
            content: "remote draft",
          },
        },
        {
          id: "local-conflict",
          entity_type: "tag",
          entity_id: "tag-1",
          action: "create",
          payload_status: "available",
        },
        {
          id: "remote-same-entity",
          entity_type: "document",
          entity_id: "doc-local-pending",
          action: "update_content",
          payload_status: "available",
        },
        {
          id: "remote-server-only-same-document",
          entity_type: "document",
          entity_id: "doc-local-pending",
          action: "publish",
          payload_status: "available",
          payload: {
            id: "doc-local-pending",
            space_id: "space-1",
            public_slug: "remote-public-slug",
          },
        },
        {
          id: "remote-missing",
          entity_type: "asset",
          entity_id: "asset-1",
          action: "create",
          payload_status: "missing",
        },
        {
          id: "remote-invalid",
          entity_type: "folder",
        },
      ],
    });

    expect(
      buildProviderRemoteImportDryRun(manifest, ["local-conflict"], [
        {
          id: "local-doc-pending",
          entity_type: "document",
          entity_id: "doc-local-pending",
          action: "update_content",
        },
      ]),
    ).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      import_ready: false,
      operation_count: 6,
      would_apply_count: 1,
      conflict_count: 2,
      failed_count: 1,
      unsupported_count: 2,
      operations: [
        {
          operation_id: "remote-ready",
          status: "would_apply",
        },
        {
          operation_id: "local-conflict",
          status: "conflict",
        },
        {
          operation_id: "remote-same-entity",
          status: "conflict",
        },
        {
          operation_id: "remote-server-only-same-document",
          status: "unsupported",
          message:
            "Remote replay operation document:publish is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-missing",
          status: "failed",
        },
        {
          operation_id: "remote-invalid",
          status: "unsupported",
        },
      ],
      blockers: expect.arrayContaining([
        "Remote provider replay import dry-run does not mutate the local vault",
      ]),
    });
  });

  it("blocks duplicate remote operation IDs before import apply planning", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-duplicate-operation",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 2,
      operation_summary: {
        ready_operation_count: 2,
        missing_payload_count: 0,
      },
      operations: [
        {
          id: "remote-duplicate",
          entity_type: "document",
          entity_id: "doc-1",
          action: "update_content",
          payload_status: "available",
          payload: {
            id: "doc-1",
            space_id: "space-1",
            content: "remote draft",
          },
        },
        {
          id: "remote-duplicate",
          entity_type: "document",
          entity_id: "doc-2",
          action: "update_content",
          payload_status: "available",
          payload: {
            id: "doc-2",
            space_id: "space-1",
            content: "remote follow-up",
          },
        },
      ],
    });

    expect(buildProviderRemoteImportPreflight(manifest)).toMatchObject({
      status: "blocked",
      import_ready: false,
      blockers: expect.arrayContaining(["Provider-sync replay operation IDs are duplicated"]),
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest);

    expect(dryRun).toMatchObject({
      operation_count: 2,
      would_apply_count: 0,
      failed_count: 2,
      blockers: expect.arrayContaining(["Provider-sync replay operation IDs are duplicated"]),
      operations: [
        {
          operation_id: "remote-duplicate",
          entity_id: "doc-1",
          status: "failed",
          message:
            "Provider import preflight is blocked: Provider-sync replay operation IDs are duplicated",
        },
        {
          operation_id: "remote-duplicate",
          entity_id: "doc-2",
          status: "failed",
          message:
            "Provider import preflight is blocked: Provider-sync replay operation IDs are duplicated",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 2,
      operations: [
        { operation_id: "remote-duplicate", apply_status: "skipped_failed" },
        { operation_id: "remote-duplicate", apply_status: "skipped_failed" },
      ],
    });
  });

  it("blocks remote whole-space deletes with explicit recovery-flow copy", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-space-delete",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-space-delete",
            entity_type: "space",
            entity_id: "space-1",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "space-1",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      unsupported_count: 1,
      operations: [
        {
          operation_id: "remote-space-delete",
          entity_type: "space",
          entity_id: "space-1",
          action: "delete",
          status: "unsupported",
          message:
            "Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 1,
      operations: [
        {
          operation_id: "remote-space-delete",
          apply_status: "skipped_unsupported",
        },
      ],
    });
  });

  it("blocks remote document-scoped imports when a local queued operation targets the same document", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-related-document-conflicts",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 5,
      operations: [
        {
          id: "remote-property-set",
          entity_type: "document_property_values",
          entity_id: "doc-local-pending",
          action: "set",
          payload_status: "available",
          payload: {
            document_id: "doc-local-pending",
            space_id: "space-1",
            values: [{ property_id: "property-1", value: "Ready" }],
          },
        },
        {
          id: "remote-document-tag",
          entity_type: "document_tag",
          entity_id: "doc-local-pending:tag-1",
          action: "add",
          payload_status: "available",
          payload: {
            document_id: "doc-local-pending",
            tag_id: "tag-1",
            space_id: "space-1",
          },
        },
        {
          id: "remote-workflow-event",
          entity_type: "document_workflow_event",
          entity_id: "event-1",
          action: "transition",
          payload_status: "available",
          payload: {
            id: "event-1",
            document_workflow_id: "workflow-1",
            document_id: "doc-local-pending",
            from_stage_id: "draft",
            to_stage_id: "review",
            action: "transition",
            created_at: "2026-07-08T12:00:00.000Z",
          },
        },
        {
          id: "remote-asset-create",
          entity_type: "asset",
          entity_id: "asset-1",
          action: "create",
          payload_status: "available",
          payload: {
            id: "asset-1",
            document_id: "doc-local-pending",
            space_id: "space-1",
            name: "diagram.png",
            size: 12,
            data_url: "data:image/png;base64,AAAA",
          },
        },
        {
          id: "remote-asset-delete",
          entity_type: "asset",
          entity_id: "asset-2",
          action: "delete",
          payload_status: "available",
          payload: {
            id: "asset-2",
            document_id: "doc-local-pending",
            space_id: "space-1",
          },
        },
      ],
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest, [], [
      {
        id: "local-doc-pending",
        entity_type: "document",
        entity_id: "doc-local-pending",
        action: "update_content",
      },
    ]);

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      conflict_count: 5,
      failed_count: 0,
      unsupported_count: 0,
      operations: [
        { operation_id: "remote-property-set", status: "conflict" },
        { operation_id: "remote-document-tag", status: "conflict" },
        { operation_id: "remote-workflow-event", status: "conflict" },
        { operation_id: "remote-asset-create", status: "conflict" },
        { operation_id: "remote-asset-delete", status: "conflict" },
      ],
    });
    expect(dryRun.operations.map((operation) => operation.message)).toEqual([
      "A local queued operation targets the same document",
      "A local queued operation targets the same document",
      "A local queued operation targets the same document",
      "A local queued operation targets the same document",
      "A local queued operation targets the same document",
    ]);
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 5,
      operations: [
        { operation_id: "remote-property-set", apply_status: "skipped_conflict" },
        { operation_id: "remote-document-tag", apply_status: "skipped_conflict" },
        { operation_id: "remote-workflow-event", apply_status: "skipped_conflict" },
        { operation_id: "remote-asset-create", apply_status: "skipped_conflict" },
        { operation_id: "remote-asset-delete", apply_status: "skipped_conflict" },
      ],
    });
  });

  it("blocks remote relationship imports when local queued operations target related entities", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-related-entity-conflicts",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 4,
      operations: [
        {
          id: "remote-document-tag-related",
          entity_type: "document_tag",
          entity_id: "doc-1:tag-local-pending",
          action: "add",
          payload_status: "available",
          payload: {
            document_id: "doc-1",
            tag_id: "tag-local-pending",
            space_id: "space-1",
          },
        },
        {
          id: "remote-property-values-related",
          entity_type: "document_property_values",
          entity_id: "doc-2",
          action: "set",
          payload_status: "available",
          payload: {
            document_id: "doc-2",
            space_id: "space-1",
            values: [{ property_id: "property-local-pending", value: "Ready" }],
          },
        },
        {
          id: "remote-document-workflow-related",
          entity_type: "document_workflow",
          entity_id: "workflow-1",
          action: "create",
          payload_status: "available",
          payload: {
            id: "workflow-1",
            document_id: "doc-3",
            workflow_template_id: "workflow-template-local-pending",
            current_stage_id: "draft",
            status: "active",
          },
        },
        {
          id: "remote-workflow-event-related",
          entity_type: "document_workflow_event",
          entity_id: "workflow-event-1",
          action: "transition",
          payload_status: "available",
          payload: {
            id: "workflow-event-1",
            document_workflow_id: "workflow-local-pending",
            document_id: "doc-4",
            from_stage_id: "draft",
            to_stage_id: "review",
            action: "transition",
            created_at: "2026-07-08T12:00:00.000Z",
          },
        },
      ],
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest, [], [
      {
        id: "local-tag-pending",
        entity_type: "tag",
        entity_id: "tag-local-pending",
        action: "update",
      },
      {
        id: "local-property-pending",
        entity_type: "property_schema",
        entity_id: "property-local-pending",
        action: "update",
      },
      {
        id: "local-workflow-template-pending",
        entity_type: "workflow_template",
        entity_id: "workflow-template-local-pending",
        action: "update",
      },
      {
        id: "local-workflow-pending",
        entity_type: "document_workflow",
        entity_id: "workflow-local-pending",
        action: "transition",
      },
    ]);

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      conflict_count: 4,
      failed_count: 0,
      unsupported_count: 0,
      operations: [
        { operation_id: "remote-document-tag-related", status: "conflict" },
        { operation_id: "remote-property-values-related", status: "conflict" },
        { operation_id: "remote-document-workflow-related", status: "conflict" },
        { operation_id: "remote-workflow-event-related", status: "conflict" },
      ],
    });
    expect(dryRun.operations.map((operation) => operation.message)).toEqual([
      "A local queued operation targets a related entity",
      "A local queued operation targets a related entity",
      "A local queued operation targets a related entity",
      "A local queued operation targets a related entity",
    ]);
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 4,
      operations: [
        { operation_id: "remote-document-tag-related", apply_status: "skipped_conflict" },
        { operation_id: "remote-property-values-related", apply_status: "skipped_conflict" },
        { operation_id: "remote-document-workflow-related", apply_status: "skipped_conflict" },
        { operation_id: "remote-workflow-event-related", apply_status: "skipped_conflict" },
      ],
    });
  });

  it("blocks remote document imports when local queued operations target the payload folder", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-document-folder-conflicts",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 2,
      operations: [
        {
          id: "remote-document-create-folder-related",
          entity_type: "document",
          entity_id: "doc-new",
          action: "create",
          payload_status: "available",
          payload: {
            id: "doc-new",
            space_id: "space-1",
            title: "Remote Folder Doc",
            folder_id: "folder-local-pending",
          },
        },
        {
          id: "remote-document-update-folder-related",
          entity_type: "document",
          entity_id: "doc-existing",
          action: "update",
          payload_status: "available",
          payload: {
            id: "doc-existing",
            space_id: "space-1",
            title: "Remote Moved Doc",
            folder_id: "folder-local-pending",
          },
        },
      ],
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest, [], [
      {
        id: "local-folder-pending",
        entity_type: "folder",
        entity_id: "folder-local-pending",
        action: "delete",
      },
    ]);

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      conflict_count: 2,
      failed_count: 0,
      unsupported_count: 0,
      operations: [
        { operation_id: "remote-document-create-folder-related", status: "conflict" },
        { operation_id: "remote-document-update-folder-related", status: "conflict" },
      ],
    });
    expect(dryRun.operations.map((operation) => operation.message)).toEqual([
      "A local queued operation targets a related entity",
      "A local queued operation targets a related entity",
    ]);
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 2,
      operations: [
        { operation_id: "remote-document-create-folder-related", apply_status: "skipped_conflict" },
        { operation_id: "remote-document-update-folder-related", apply_status: "skipped_conflict" },
      ],
    });
  });

  it("rejects remote replay operations with empty identity fields during dry-run", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-empty-identity",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "",
            entity_type: "document",
            entity_id: "   ",
            action: "update_content",
            payload_status: "available",
            payload: {
              id: "",
              space_id: "space-1",
              content: "remote draft",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      unsupported_count: 1,
      operations: [
        {
          operation_id: "invalid-1",
          entity_type: "document",
          entity_id: "unknown",
          action: "update_content",
          status: "unsupported",
          message: "Remote replay operation is missing required identity fields",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 1,
      operations: [
        {
          operation_id: "invalid-1",
          apply_status: "skipped_unsupported",
        },
      ],
    });
  });

  it("rejects remote replay operations with padded identity fields during dry-run", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-padded-identity",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: " local-conflict ",
            entity_type: "document",
            entity_id: "doc-1",
            action: "update_content",
            payload_status: "available",
            payload: {
              id: "doc-1",
              space_id: "space-1",
              content: "remote draft",
            },
          },
        ],
      }),
      ["local-conflict"],
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 1,
      blockers: expect.arrayContaining([
        "Provider-sync replay operation identity fields are padded",
      ]),
      operations: [
        {
          operation_id: " local-conflict ",
          status: "failed",
          message: "Remote replay operation identity fields must not include leading or trailing whitespace",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 1,
      operations: [
        {
          operation_id: " local-conflict ",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("rejects remote replay operations with malformed payload status during dry-run", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-malformed-payload-status",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-padded-status",
            entity_type: "document",
            entity_id: "doc-1",
            action: "update_content",
            payload_status: "available ",
            payload: {
              id: "doc-1",
              space_id: "space-1",
              content: "remote draft",
            },
          },
          {
            id: "remote-unknown-status",
            entity_type: "document",
            entity_id: "doc-2",
            action: "update_content",
            payload_status: "queued",
            payload: {
              id: "doc-2",
              space_id: "space-1",
              content: "remote follow-up",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 2,
      blockers: expect.arrayContaining(["Provider-sync replay payload status is invalid"]),
      operations: [
        {
          operation_id: "remote-padded-status",
          status: "failed",
          message:
            "Provider import preflight is blocked: Provider-sync replay payload status is invalid",
        },
        {
          operation_id: "remote-unknown-status",
          status: "failed",
          message:
            "Provider import preflight is blocked: Provider-sync replay payload status is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 2,
      operations: [
        { operation_id: "remote-padded-status", apply_status: "skipped_failed" },
        { operation_id: "remote-unknown-status", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for supported remote replay operations without object payloads", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-missing-payload",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 1,
      operations: [
        {
          id: "remote-missing-object-payload",
          entity_type: "document",
          entity_id: "doc-1",
          action: "update_content",
          payload_status: "available",
        },
      ],
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest);
    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 1,
      operations: [
        {
          operation_id: "remote-missing-object-payload",
          status: "failed",
          message:
            "Remote replay operation document:update_content payload is not available for local import",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 1,
      operations: [
        {
          operation_id: "remote-missing-object-payload",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for malformed delete context while preserving payload-less deletes", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-delete-context",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 9,
      operations: [
        {
          id: "remote-folder-delete-invalid-space",
          entity_type: "folder",
          entity_id: "folder-1",
          action: "delete",
          payload_status: "available",
          payload: { id: "folder-1", space_id: 42 },
        },
        {
          id: "remote-asset-delete-invalid-document",
          entity_type: "asset",
          entity_id: "asset-1",
          action: "delete",
          payload_status: "available",
          payload: { id: "asset-1", document_id: false },
        },
        {
          id: "remote-workflow-delete-invalid-document",
          entity_type: "document_workflow",
          entity_id: "workflow-1",
          action: "delete",
          payload_status: "available",
          payload: { id: "workflow-1", document_id: ["doc-1"] },
        },
        {
          id: "remote-template-delete-blank-space",
          entity_type: "template",
          entity_id: "template-1",
          action: "delete",
          payload_status: "available",
          payload: { id: "template-1", space_id: " " },
        },
        {
          id: "remote-asset-delete-blank-document",
          entity_type: "asset",
          entity_id: "asset-blank-document",
          action: "delete",
          payload_status: "available",
          payload: { id: "asset-blank-document", document_id: "\t" },
        },
        {
          id: "remote-workflow-delete-blank-document",
          entity_type: "document_workflow",
          entity_id: "workflow-blank-document",
          action: "delete",
          payload_status: "available",
          payload: { id: "workflow-blank-document", document_id: "\n" },
        },
        {
          id: "remote-document-delete-invalid-deleter",
          entity_type: "document",
          entity_id: "doc-delete-invalid-deleter",
          action: "delete",
          payload_status: "available",
          payload: { id: "doc-delete-invalid-deleter", deleted_by: ["user-1"] },
        },
        {
          id: "remote-tag-delete-invalid-deleted-at",
          entity_type: "tag",
          entity_id: "tag-delete-invalid-deleted-at",
          action: "delete",
          payload_status: "available",
          payload: { id: "tag-delete-invalid-deleted-at", deleted_at: 42 },
        },
        {
          id: "remote-tag-delete-payloadless",
          entity_type: "tag",
          entity_id: "tag-1",
          action: "delete",
          payload_status: "available",
        },
      ],
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest);
    expect(dryRun).toMatchObject({
      would_apply_count: 1,
      failed_count: 8,
      operations: [
        {
          operation_id: "remote-folder-delete-invalid-space",
          status: "failed",
          message: "Remote replay operation folder:delete payload space is invalid",
        },
        {
          operation_id: "remote-asset-delete-invalid-document",
          status: "failed",
          message: "Remote replay operation asset:delete payload document is invalid",
        },
        {
          operation_id: "remote-workflow-delete-invalid-document",
          status: "failed",
          message:
            "Remote replay operation document_workflow:delete payload document is invalid",
        },
        {
          operation_id: "remote-template-delete-blank-space",
          status: "failed",
          message: "Remote replay operation template:delete payload space is invalid",
        },
        {
          operation_id: "remote-asset-delete-blank-document",
          status: "failed",
          message: "Remote replay operation asset:delete payload document is invalid",
        },
        {
          operation_id: "remote-workflow-delete-blank-document",
          status: "failed",
          message:
            "Remote replay operation document_workflow:delete payload document is invalid",
        },
        {
          operation_id: "remote-document-delete-invalid-deleter",
          status: "failed",
          message: "Remote replay operation document:delete payload deleter is invalid",
        },
        {
          operation_id: "remote-tag-delete-invalid-deleted-at",
          status: "failed",
          message: "Remote replay operation tag:delete payload deleted timestamp is invalid",
        },
        {
          operation_id: "remote-tag-delete-payloadless",
          status: "would_apply",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 1,
      skipped_operation_count: 8,
      operations: [
        { operation_id: "remote-folder-delete-invalid-space", apply_status: "skipped_failed" },
        { operation_id: "remote-asset-delete-invalid-document", apply_status: "skipped_failed" },
        {
          operation_id: "remote-workflow-delete-invalid-document",
          apply_status: "skipped_failed",
        },
        { operation_id: "remote-template-delete-blank-space", apply_status: "skipped_failed" },
        { operation_id: "remote-asset-delete-blank-document", apply_status: "skipped_failed" },
        {
          operation_id: "remote-workflow-delete-blank-document",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-delete-invalid-deleter",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-tag-delete-invalid-deleted-at",
          apply_status: "skipped_failed",
        },
        { operation_id: "remote-tag-delete-payloadless", apply_status: "selected" },
      ],
    });
  });

  it("fails dry-run for supported remote replay operations with array payloads", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-array-payload",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 3,
      operations: [
        {
          id: "remote-array-payload",
          entity_type: "document",
          entity_id: "doc-1",
          action: "update_content",
          payload_status: "available",
          payload: [],
        },
        {
          id: "remote-delete-array-payload",
          entity_type: "folder",
          entity_id: "folder-1",
          action: "delete",
          payload_status: "available",
          payload: [],
        },
        {
          id: "remote-delete-string-payload",
          entity_type: "document",
          entity_id: "doc-delete-1",
          action: "delete",
          payload_status: "available",
          payload: "not-a-tombstone",
        },
      ],
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest);
    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 3,
      operations: [
        {
          operation_id: "remote-array-payload",
          status: "failed",
          message:
            "Remote replay operation document:update_content payload is not available for local import",
        },
        {
          operation_id: "remote-delete-array-payload",
          status: "failed",
          message:
            "Remote replay operation folder:delete payload is not available for local import",
        },
        {
          operation_id: "remote-delete-string-payload",
          status: "failed",
          message:
            "Remote replay operation document:delete payload is not available for local import",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 3,
      operations: [
        {
          operation_id: "remote-array-payload",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-delete-array-payload",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-delete-string-payload",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for supported remote replay operations with mismatched payload IDs", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-payload-id-mismatch",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 4,
      operations: [
        {
          id: "remote-mismatched-payload",
          entity_type: "document",
          entity_id: "doc-1",
          action: "update_content",
          payload_status: "available",
          payload: {
            id: "doc-2",
            space_id: "local-space",
            content: "remote draft",
          },
        },
        {
          id: "remote-mismatched-document-tag",
          entity_type: "document_tag",
          entity_id: "doc-1:tag-1",
          action: "add",
          payload_status: "available",
          payload: {
            document_id: "doc-2",
            tag_id: "tag-1",
            space_id: "local-space",
          },
        },
        {
          id: "remote-mismatched-property-values",
          entity_type: "document_property_values",
          entity_id: "doc-1",
          action: "set",
          payload_status: "available",
          payload: {
            document_id: "doc-2",
            space_id: "local-space",
            values: [{ property_id: "property-1", value: "Remote" }],
          },
        },
        {
          id: "remote-malformed-payload-id",
          entity_type: "folder",
          entity_id: "folder-1",
          action: "delete",
          payload_status: "available",
          payload: {
            id: 42,
            space_id: "local-space",
          },
        },
      ],
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest);
    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 4,
      operations: [
        {
          operation_id: "remote-mismatched-payload",
          status: "failed",
          message:
            "Remote replay operation document:update_content payload ID does not match operation entity ID",
        },
        {
          operation_id: "remote-mismatched-document-tag",
          status: "failed",
          message:
            "Remote replay operation document_tag:add payload relationship does not match operation entity ID",
        },
        {
          operation_id: "remote-mismatched-property-values",
          status: "failed",
          message:
            "Remote replay operation document_property_values:set payload document does not match operation entity ID",
        },
        {
          operation_id: "remote-malformed-payload-id",
          status: "failed",
          message: "Remote replay operation folder:delete payload ID is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 4,
      operations: [
        {
          operation_id: "remote-mismatched-payload",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-mismatched-document-tag",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-mismatched-property-values",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-malformed-payload-id",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for document replay payloads missing required semantic fields", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-document-semantic-payload",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-document-create-missing-title",
            entity_type: "document",
            entity_id: "doc-create",
            action: "create",
            payload_status: "available",
            payload: {
              id: "doc-create",
              space_id: "space-1",
            },
          },
          {
            id: "remote-document-update-missing-space",
            entity_type: "document",
            entity_id: "doc-update",
            action: "update",
            payload_status: "available",
            payload: {
              id: "doc-update",
              title: "Missing Space",
            },
          },
          {
            id: "remote-document-update-invalid-folder",
            entity_type: "document",
            entity_id: "doc-update-invalid-folder",
            action: "update",
            payload_status: "available",
            payload: {
              id: "doc-update-invalid-folder",
              space_id: "space-1",
              title: "Invalid Folder",
              folder_id: ["folder-1"],
            },
          },
          {
            id: "remote-document-content-missing-content",
            entity_type: "document",
            entity_id: "doc-content",
            action: "update_content",
            payload_status: "available",
            payload: {
              id: "doc-content",
              space_id: "space-1",
            },
          },
          {
            id: "remote-document-content-invalid-folder",
            entity_type: "document",
            entity_id: "doc-content-invalid-folder",
            action: "update_content",
            payload_status: "available",
            payload: {
              id: "doc-content-invalid-folder",
              content: "Remote content",
              folder_id: { id: "folder-1" },
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 5,
      operations: [
        {
          operation_id: "remote-document-create-missing-title",
          status: "failed",
          message: "Remote replay operation document:create payload is missing space or title",
        },
        {
          operation_id: "remote-document-update-missing-space",
          status: "failed",
          message: "Remote replay operation document:update payload is missing space or title",
        },
        {
          operation_id: "remote-document-update-invalid-folder",
          status: "failed",
          message: "Remote replay operation document:update payload folder is invalid",
        },
        {
          operation_id: "remote-document-content-missing-content",
          status: "failed",
          message: "Remote replay operation document:update_content payload is missing content",
        },
        {
          operation_id: "remote-document-content-invalid-folder",
          status: "failed",
          message: "Remote replay operation document:update_content payload folder is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 5,
      operations: [
        {
          operation_id: "remote-document-create-missing-title",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-update-missing-space",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-update-invalid-folder",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-content-missing-content",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-content-invalid-folder",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for document payloads that violate local normalization limits", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-document-normalization-payload",
    );
    const oversizedContent = "x".repeat(5 * 1024 * 1024 + 1);
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-document-create-blank-title",
            entity_type: "document",
            entity_id: "doc-create-blank-title",
            action: "create",
            payload_status: "available",
            payload: {
              id: "doc-create-blank-title",
              space_id: "space-1",
              title: " ",
            },
          },
          {
            id: "remote-document-update-blank-title",
            entity_type: "document",
            entity_id: "doc-update-blank-title",
            action: "update",
            payload_status: "available",
            payload: {
              id: "doc-update-blank-title",
              space_id: "space-1",
              title: "\t",
            },
          },
          {
            id: "remote-document-create-oversized-content",
            entity_type: "document",
            entity_id: "doc-create-oversized-content",
            action: "create",
            payload_status: "available",
            payload: {
              id: "doc-create-oversized-content",
              space_id: "space-1",
              title: "Oversized Content",
              content: oversizedContent,
            },
          },
          {
            id: "remote-document-content-oversized-content",
            entity_type: "document",
            entity_id: "doc-content-oversized-content",
            action: "update_content",
            payload_status: "available",
            payload: {
              id: "doc-content-oversized-content",
              content: oversizedContent,
            },
          },
          {
            id: "remote-document-content-blank-title",
            entity_type: "document",
            entity_id: "doc-content-blank-title",
            action: "update_content",
            payload_status: "available",
            payload: {
              id: "doc-content-blank-title",
              content: "Valid content",
              title: " ",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 5,
      operations: [
        {
          operation_id: "remote-document-create-blank-title",
          status: "failed",
          message: "Remote replay operation document:create payload is missing title",
        },
        {
          operation_id: "remote-document-update-blank-title",
          status: "failed",
          message: "Remote replay operation document:update payload is missing title",
        },
        {
          operation_id: "remote-document-create-oversized-content",
          status: "failed",
          message: "Remote replay operation document:create payload content exceeds 5 MB limit",
        },
        {
          operation_id: "remote-document-content-oversized-content",
          status: "failed",
          message:
            "Remote replay operation document:update_content payload content exceeds 5 MB limit",
        },
        {
          operation_id: "remote-document-content-blank-title",
          status: "failed",
          message: "Remote replay operation document:update_content payload is missing title",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 5,
      operations: [
        {
          operation_id: "remote-document-create-blank-title",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-update-blank-title",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-create-oversized-content",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-content-oversized-content",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-content-blank-title",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for malformed optional document metadata before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-document-metadata",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-document-create-invalid-icon",
            entity_type: "document",
            entity_id: "doc-create-invalid-icon",
            action: "create",
            payload_status: "available",
            payload: {
              id: "doc-create-invalid-icon",
              space_id: "space-1",
              title: "Invalid Icon",
              icon: ["sparkles"],
            },
          },
          {
            id: "remote-document-update-invalid-timestamp",
            entity_type: "document",
            entity_id: "doc-update-invalid-timestamp",
            action: "update",
            payload_status: "available",
            payload: {
              id: "doc-update-invalid-timestamp",
              space_id: "space-1",
              title: "Invalid Timestamp",
              updated_at: 123,
            },
          },
          {
            id: "remote-document-content-invalid-title",
            entity_type: "document",
            entity_id: "doc-content-invalid-title",
            action: "update_content",
            payload_status: "available",
            payload: {
              id: "doc-content-invalid-title",
              content: "Remote content",
              title: { value: "Invalid Title" },
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 3,
      operations: [
        {
          operation_id: "remote-document-create-invalid-icon",
          status: "failed",
          message: "Remote replay operation document:create payload icon is invalid",
        },
        {
          operation_id: "remote-document-update-invalid-timestamp",
          status: "failed",
          message: "Remote replay operation document:update payload updated timestamp is invalid",
        },
        {
          operation_id: "remote-document-content-invalid-title",
          status: "failed",
          message: "Remote replay operation document:update_content payload title is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 3,
      operations: [
        { operation_id: "remote-document-create-invalid-icon", apply_status: "skipped_failed" },
        {
          operation_id: "remote-document-update-invalid-timestamp",
          apply_status: "skipped_failed",
        },
        { operation_id: "remote-document-content-invalid-title", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for malformed optional space metadata before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-space-metadata",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-space-create-invalid-slug",
            entity_type: "space",
            entity_id: "space-create-invalid-slug",
            action: "create",
            payload_status: "available",
            payload: {
              id: "space-create-invalid-slug",
              name: "Invalid Slug",
              slug: ["invalid-slug"],
            },
          },
          {
            id: "remote-space-update-invalid-owner",
            entity_type: "space",
            entity_id: "space-update-invalid-owner",
            action: "update",
            payload_status: "available",
            payload: {
              id: "space-update-invalid-owner",
              name: "Invalid Owner",
              owner_username: { username: "admin" },
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 2,
      operations: [
        {
          operation_id: "remote-space-create-invalid-slug",
          status: "failed",
          message: "Remote replay operation space:create payload slug is invalid",
        },
        {
          operation_id: "remote-space-update-invalid-owner",
          status: "failed",
          message: "Remote replay operation space:update payload owner username is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 2,
      operations: [
        { operation_id: "remote-space-create-invalid-slug", apply_status: "skipped_failed" },
        { operation_id: "remote-space-update-invalid-owner", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for metadata replay payloads missing required semantic fields", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-metadata-semantic-payload",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 6,
        operations: [
          {
            id: "remote-space-create-missing-name",
            entity_type: "space",
            entity_id: "space-create",
            action: "create",
            payload_status: "available",
            payload: {
              id: "space-create",
            },
          },
          {
            id: "remote-folder-update-missing-space",
            entity_type: "folder",
            entity_id: "folder-update",
            action: "update",
            payload_status: "available",
            payload: {
              id: "folder-update",
              name: "Missing Space",
            },
          },
          {
            id: "remote-folder-update-invalid-parent",
            entity_type: "folder",
            entity_id: "folder-update-invalid-parent",
            action: "update",
            payload_status: "available",
            payload: {
              id: "folder-update-invalid-parent",
              space_id: "space-1",
              name: "Invalid Parent",
              parent_id: { id: "folder-1" },
            },
          },
          {
            id: "remote-tag-create-missing-name",
            entity_type: "tag",
            entity_id: "tag-create",
            action: "create",
            payload_status: "available",
            payload: {
              id: "tag-create",
              space_id: "space-1",
            },
          },
          {
            id: "remote-template-update-missing-content",
            entity_type: "template",
            entity_id: "template-update",
            action: "update",
            payload_status: "available",
            payload: {
              id: "template-update",
              space_id: "space-1",
              name: "Missing Content",
            },
          },
          {
            id: "remote-workflow-template-create-missing-stages",
            entity_type: "workflow_template",
            entity_id: "workflow-template-create",
            action: "create",
            payload_status: "available",
            payload: {
              id: "workflow-template-create",
              space_id: "space-1",
              name: "Missing Stages",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 6,
      operations: [
        {
          operation_id: "remote-space-create-missing-name",
          status: "failed",
          message: "Remote replay operation space:create payload is missing name",
        },
        {
          operation_id: "remote-folder-update-missing-space",
          status: "failed",
          message: "Remote replay operation folder:update payload is missing space or name",
        },
        {
          operation_id: "remote-folder-update-invalid-parent",
          status: "failed",
          message: "Remote replay operation folder:update payload parent is invalid",
        },
        {
          operation_id: "remote-tag-create-missing-name",
          status: "failed",
          message: "Remote replay operation tag:create payload is missing space or name",
        },
        {
          operation_id: "remote-template-update-missing-content",
          status: "failed",
          message:
            "Remote replay operation template:update payload is missing space, name, or content",
        },
        {
          operation_id: "remote-workflow-template-create-missing-stages",
          status: "failed",
          message:
            "Remote replay operation workflow_template:create payload is missing space, name, or stages",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 6,
      operations: [
        { operation_id: "remote-space-create-missing-name", apply_status: "skipped_failed" },
        { operation_id: "remote-folder-update-missing-space", apply_status: "skipped_failed" },
        { operation_id: "remote-folder-update-invalid-parent", apply_status: "skipped_failed" },
        { operation_id: "remote-tag-create-missing-name", apply_status: "skipped_failed" },
        { operation_id: "remote-template-update-missing-content", apply_status: "skipped_failed" },
        {
          operation_id: "remote-workflow-template-create-missing-stages",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for blank import names before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-blank-name-payload",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-space-create-blank-name",
            entity_type: "space",
            entity_id: "space-create",
            action: "create",
            payload_status: "available",
            payload: {
              id: "space-create",
              name: "   ",
            },
          },
          {
            id: "remote-folder-create-blank-name",
            entity_type: "folder",
            entity_id: "folder-create",
            action: "create",
            payload_status: "available",
            payload: {
              id: "folder-create",
              space_id: "space-1",
              name: "\t",
            },
          },
          {
            id: "remote-tag-update-blank-name",
            entity_type: "tag",
            entity_id: "tag-update",
            action: "update",
            payload_status: "available",
            payload: {
              id: "tag-update",
              space_id: "space-1",
              name: "\n",
            },
          },
          {
            id: "remote-template-create-blank-name",
            entity_type: "template",
            entity_id: "template-create",
            action: "create",
            payload_status: "available",
            payload: {
              id: "template-create",
              space_id: "space-1",
              name: " ",
              content: "Template body",
            },
          },
          {
            id: "remote-asset-create-blank-name",
            entity_type: "asset",
            entity_id: "asset-create",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-create",
              space_id: "space-1",
              name: " ",
              size: 5,
              data_url: "data:text/plain;base64,SGVsbG8=",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 5,
      operations: [
        {
          operation_id: "remote-space-create-blank-name",
          status: "failed",
          message: "Remote replay operation space:create payload is missing space name",
        },
        {
          operation_id: "remote-folder-create-blank-name",
          status: "failed",
          message: "Remote replay operation folder:create payload is missing folder name",
        },
        {
          operation_id: "remote-tag-update-blank-name",
          status: "failed",
          message: "Remote replay operation tag:update payload is missing tag name",
        },
        {
          operation_id: "remote-template-create-blank-name",
          status: "failed",
          message:
            "Remote replay operation template:create payload is missing template name",
        },
        {
          operation_id: "remote-asset-create-blank-name",
          status: "failed",
          message: "Remote replay operation asset:create payload is missing asset name",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 5,
      operations: [
        { operation_id: "remote-space-create-blank-name", apply_status: "skipped_failed" },
        { operation_id: "remote-folder-create-blank-name", apply_status: "skipped_failed" },
        { operation_id: "remote-tag-update-blank-name", apply_status: "skipped_failed" },
        { operation_id: "remote-template-create-blank-name", apply_status: "skipped_failed" },
        { operation_id: "remote-asset-create-blank-name", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for blank required context IDs before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-blank-context-payload",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 8,
        operations: [
          {
            id: "remote-document-create-blank-space",
            entity_type: "document",
            entity_id: "doc-blank-space",
            action: "create",
            payload_status: "available",
            payload: {
              id: "doc-blank-space",
              space_id: " ",
              title: "Blank Space",
            },
          },
          {
            id: "remote-folder-create-blank-space",
            entity_type: "folder",
            entity_id: "folder-blank-space",
            action: "create",
            payload_status: "available",
            payload: {
              id: "folder-blank-space",
              space_id: "\t",
              name: "Blank Space",
            },
          },
          {
            id: "remote-tag-update-blank-space",
            entity_type: "tag",
            entity_id: "tag-blank-space",
            action: "update",
            payload_status: "available",
            payload: {
              id: "tag-blank-space",
              space_id: "\n",
              name: "Blank Space",
            },
          },
          {
            id: "remote-template-create-blank-space",
            entity_type: "template",
            entity_id: "template-blank-space",
            action: "create",
            payload_status: "available",
            payload: {
              id: "template-blank-space",
              space_id: " ",
              name: "Blank Space",
              content: "Template body",
            },
          },
          {
            id: "remote-workflow-template-create-blank-space",
            entity_type: "workflow_template",
            entity_id: "workflow-template-blank-space",
            action: "create",
            payload_status: "available",
            payload: {
              id: "workflow-template-blank-space",
              space_id: " ",
              name: "Blank Space",
              stages: [{ id: "draft", name: "Draft" }, { id: "review", name: "Review" }],
            },
          },
          {
            id: "remote-property-reorder-blank-id",
            entity_type: "property_schema",
            entity_id: "space-1",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "space-1",
              property_ids: ["property-1", " "],
            },
          },
          {
            id: "remote-property-values-blank-space",
            entity_type: "document_property_values",
            entity_id: "doc-1",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "doc-1",
              space_id: " ",
              values: [{ property_id: "property-1", value: "Ready" }],
            },
          },
          {
            id: "remote-asset-create-blank-space",
            entity_type: "asset",
            entity_id: "asset-blank-space",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-blank-space",
              space_id: " ",
              name: "asset.txt",
              size: 5,
              data_url: "data:text/plain;base64,SGVsbG8=",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 8,
      operations: [
        {
          operation_id: "remote-document-create-blank-space",
          status: "failed",
          message: "Remote replay operation document:create payload space is invalid",
        },
        {
          operation_id: "remote-folder-create-blank-space",
          status: "failed",
          message: "Remote replay operation folder:create payload space is invalid",
        },
        {
          operation_id: "remote-tag-update-blank-space",
          status: "failed",
          message: "Remote replay operation tag:update payload space is invalid",
        },
        {
          operation_id: "remote-template-create-blank-space",
          status: "failed",
          message: "Remote replay operation template:create payload space is invalid",
        },
        {
          operation_id: "remote-workflow-template-create-blank-space",
          status: "failed",
          message:
            "Remote replay operation workflow_template:create payload space is invalid",
        },
        {
          operation_id: "remote-property-reorder-blank-id",
          status: "failed",
          message:
            "Remote replay operation property_schema:reorder payload includes invalid property IDs",
        },
        {
          operation_id: "remote-property-values-blank-space",
          status: "failed",
          message:
            "Remote replay operation document_property_values:set payload space is invalid",
        },
        {
          operation_id: "remote-asset-create-blank-space",
          status: "failed",
          message: "Remote replay operation asset:create payload space is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 8,
      operations: [
        { operation_id: "remote-document-create-blank-space", apply_status: "skipped_failed" },
        { operation_id: "remote-folder-create-blank-space", apply_status: "skipped_failed" },
        { operation_id: "remote-tag-update-blank-space", apply_status: "skipped_failed" },
        { operation_id: "remote-template-create-blank-space", apply_status: "skipped_failed" },
        {
          operation_id: "remote-workflow-template-create-blank-space",
          apply_status: "skipped_failed",
        },
        { operation_id: "remote-property-reorder-blank-id", apply_status: "skipped_failed" },
        { operation_id: "remote-property-values-blank-space", apply_status: "skipped_failed" },
        { operation_id: "remote-asset-create-blank-space", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for template payloads that violate local normalization limits", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-template-normalization-payload",
    );
    const oversizedContent = "x".repeat(5 * 1024 * 1024 + 1);
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 4,
        operations: [
          {
            id: "remote-template-create-long-name",
            entity_type: "template",
            entity_id: "template-long-name",
            action: "create",
            payload_status: "available",
            payload: {
              id: "template-long-name",
              space_id: "space-1",
              name: "x".repeat(101),
              content: "Template body",
            },
          },
          {
            id: "remote-template-create-invalid-description",
            entity_type: "template",
            entity_id: "template-invalid-description",
            action: "create",
            payload_status: "available",
            payload: {
              id: "template-invalid-description",
              space_id: "space-1",
              name: "Invalid Description",
              description: 42,
              content: "Template body",
            },
          },
          {
            id: "remote-template-update-long-description",
            entity_type: "template",
            entity_id: "template-long-description",
            action: "update",
            payload_status: "available",
            payload: {
              id: "template-long-description",
              space_id: "space-1",
              name: "Long Description",
              description: "x".repeat(1001),
              content: "Template body",
            },
          },
          {
            id: "remote-template-create-oversized-content",
            entity_type: "template",
            entity_id: "template-oversized-content",
            action: "create",
            payload_status: "available",
            payload: {
              id: "template-oversized-content",
              space_id: "space-1",
              name: "Oversized Content",
              content: oversizedContent,
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 4,
      operations: [
        {
          operation_id: "remote-template-create-long-name",
          status: "failed",
          message: "Remote replay operation template:create payload name is too long",
        },
        {
          operation_id: "remote-template-create-invalid-description",
          status: "failed",
          message: "Remote replay operation template:create payload description is invalid",
        },
        {
          operation_id: "remote-template-update-long-description",
          status: "failed",
          message: "Remote replay operation template:update payload description is too long",
        },
        {
          operation_id: "remote-template-create-oversized-content",
          status: "failed",
          message: "Remote replay operation template:create payload content exceeds 5 MB limit",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 4,
      operations: [
        { operation_id: "remote-template-create-long-name", apply_status: "skipped_failed" },
        {
          operation_id: "remote-template-create-invalid-description",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-template-update-long-description",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-template-create-oversized-content",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for malformed optional template metadata before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-template-metadata",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-template-create-invalid-tags",
            entity_type: "template",
            entity_id: "template-invalid-tags",
            action: "create",
            payload_status: "available",
            payload: {
              id: "template-invalid-tags",
              space_id: "space-1",
              name: "Invalid Tags",
              content: "Template body",
              tags: ["valid", 42],
            },
          },
          {
            id: "remote-template-update-invalid-creator",
            entity_type: "template",
            entity_id: "template-invalid-creator",
            action: "update",
            payload_status: "available",
            payload: {
              id: "template-invalid-creator",
              space_id: "space-1",
              name: "Invalid Creator",
              content: "Template body",
              created_by: { id: "admin" },
            },
          },
          {
            id: "remote-template-update-invalid-timestamp",
            entity_type: "template",
            entity_id: "template-invalid-timestamp",
            action: "update",
            payload_status: "available",
            payload: {
              id: "template-invalid-timestamp",
              space_id: "space-1",
              name: "Invalid Timestamp",
              content: "Template body",
              updated_at: 123,
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 3,
      operations: [
        {
          operation_id: "remote-template-create-invalid-tags",
          status: "failed",
          message: "Remote replay operation template:create payload tags are invalid",
        },
        {
          operation_id: "remote-template-update-invalid-creator",
          status: "failed",
          message: "Remote replay operation template:update payload creator is invalid",
        },
        {
          operation_id: "remote-template-update-invalid-timestamp",
          status: "failed",
          message: "Remote replay operation template:update payload updated timestamp is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 3,
      operations: [
        { operation_id: "remote-template-create-invalid-tags", apply_status: "skipped_failed" },
        { operation_id: "remote-template-update-invalid-creator", apply_status: "skipped_failed" },
        {
          operation_id: "remote-template-update-invalid-timestamp",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for malformed workflow template and property normalization payloads", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-workflow-normalization-payload",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-workflow-template-create-blank-name",
            entity_type: "workflow_template",
            entity_id: "workflow-template-blank-name",
            action: "create",
            payload_status: "available",
            payload: {
              id: "workflow-template-blank-name",
              space_id: "space-1",
              name: " ",
              stages: [
                { id: "draft", name: "Draft" },
                { id: "review", name: "Review" },
              ],
            },
          },
          {
            id: "remote-workflow-template-create-invalid-stage",
            entity_type: "workflow_template",
            entity_id: "workflow-template-invalid-stage",
            action: "create",
            payload_status: "available",
            payload: {
              id: "workflow-template-invalid-stage",
              space_id: "space-1",
              name: "Invalid Stage",
              stages: [{ id: "draft", name: 5 }],
            },
          },
          {
            id: "remote-workflow-template-create-too-few-stages",
            entity_type: "workflow_template",
            entity_id: "workflow-template-too-few-stages",
            action: "create",
            payload_status: "available",
            payload: {
              id: "workflow-template-too-few-stages",
              space_id: "space-1",
              name: "Too Few Stages",
              stages: [
                { id: "draft", name: "Draft" },
                { id: "blank", name: " " },
              ],
            },
          },
          {
            id: "remote-workflow-template-create-missing-approver-stage",
            entity_type: "workflow_template",
            entity_id: "workflow-template-missing-approver-stage",
            action: "create",
            payload_status: "available",
            payload: {
              id: "workflow-template-missing-approver-stage",
              space_id: "space-1",
              name: "Missing Approver Stage",
              stages: [
                { id: "draft", name: "Draft" },
                { id: "review", name: "Review" },
              ],
              approvers: [
                {
                  approver_type: "user",
                  approver_id: "user-1",
                  stage_id: "missing-stage",
                },
              ],
            },
          },
          {
            id: "remote-property-create-blank-name",
            entity_type: "property_schema",
            entity_id: "property-blank-name",
            action: "create",
            payload_status: "available",
            payload: {
              id: "property-blank-name",
              space_id: "space-1",
              name: " ",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 5,
      operations: [
        {
          operation_id: "remote-workflow-template-create-blank-name",
          status: "failed",
          message:
            "Remote replay operation workflow_template:create payload is missing workflow template name",
        },
        {
          operation_id: "remote-workflow-template-create-invalid-stage",
          status: "failed",
          message: "Remote replay operation workflow_template:create payload includes invalid stages",
        },
        {
          operation_id: "remote-workflow-template-create-too-few-stages",
          status: "failed",
          message:
            "Remote replay operation workflow_template:create payload requires at least two stages",
        },
        {
          operation_id: "remote-workflow-template-create-missing-approver-stage",
          status: "failed",
          message:
            "Remote replay operation workflow_template:create payload approver stage does not exist",
        },
        {
          operation_id: "remote-property-create-blank-name",
          status: "failed",
          message:
            "Remote replay operation property_schema:create payload is missing property name",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 5,
      operations: [
        {
          operation_id: "remote-workflow-template-create-blank-name",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-workflow-template-create-invalid-stage",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-workflow-template-create-too-few-stages",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-workflow-template-create-missing-approver-stage",
          apply_status: "skipped_failed",
        },
        { operation_id: "remote-property-create-blank-name", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for malformed optional workflow template metadata before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-workflow-template-metadata",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 4,
        operations: [
          {
            id: "remote-workflow-template-invalid-creator",
            entity_type: "workflow_template",
            entity_id: "workflow-template-invalid-creator",
            action: "create",
            payload_status: "available",
            payload: {
              id: "workflow-template-invalid-creator",
              space_id: "space-1",
              name: "Invalid Creator",
              created_by: { id: "admin" },
              stages: [
                { id: "draft", name: "Draft" },
                { id: "review", name: "Review" },
              ],
            },
          },
          {
            id: "remote-workflow-template-invalid-stage-id",
            entity_type: "workflow_template",
            entity_id: "workflow-template-invalid-stage-id",
            action: "update",
            payload_status: "available",
            payload: {
              id: "workflow-template-invalid-stage-id",
              space_id: "space-1",
              name: "Invalid Stage",
              stages: [
                { id: ["draft"], name: "Draft" },
                { id: "review", name: "Review" },
              ],
            },
          },
          {
            id: "remote-workflow-template-invalid-approvers",
            entity_type: "workflow_template",
            entity_id: "workflow-template-invalid-approvers",
            action: "update",
            payload_status: "available",
            payload: {
              id: "workflow-template-invalid-approvers",
              space_id: "space-1",
              name: "Invalid Approvers",
              stages: [
                { id: "draft", name: "Draft" },
                { id: "review", name: "Review" },
              ],
              approvers: { approver_id: "admin" },
            },
          },
          {
            id: "remote-workflow-template-invalid-approver-type",
            entity_type: "workflow_template",
            entity_id: "workflow-template-invalid-approver-type",
            action: "update",
            payload_status: "available",
            payload: {
              id: "workflow-template-invalid-approver-type",
              space_id: "space-1",
              name: "Invalid Approver Type",
              stages: [
                { id: "draft", name: "Draft" },
                { id: "review", name: "Review" },
              ],
              approvers: [{ approver_type: null, approver_id: "admin", stage_id: "review" }],
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 4,
      operations: [
        {
          operation_id: "remote-workflow-template-invalid-creator",
          status: "failed",
          message: "Remote replay operation workflow_template:create payload creator is invalid",
        },
        {
          operation_id: "remote-workflow-template-invalid-stage-id",
          status: "failed",
          message:
            "Remote replay operation workflow_template:update payload includes invalid stage metadata",
        },
        {
          operation_id: "remote-workflow-template-invalid-approvers",
          status: "failed",
          message:
            "Remote replay operation workflow_template:update payload includes invalid approvers",
        },
        {
          operation_id: "remote-workflow-template-invalid-approver-type",
          status: "failed",
          message:
            "Remote replay operation workflow_template:update payload includes invalid approvers",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 4,
      operations: [
        {
          operation_id: "remote-workflow-template-invalid-creator",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-workflow-template-invalid-stage-id",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-workflow-template-invalid-approvers",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-workflow-template-invalid-approver-type",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for malformed optional property schema metadata before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-property-schema-metadata",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-property-schema-invalid-type",
            entity_type: "property_schema",
            entity_id: "property-invalid-type",
            action: "create",
            payload_status: "available",
            payload: {
              id: "property-invalid-type",
              space_id: "space-1",
              name: "Invalid Type",
              property_type: ["select"],
            },
          },
          {
            id: "remote-property-schema-invalid-options",
            entity_type: "property_schema",
            entity_id: "property-invalid-options",
            action: "update",
            payload_status: "available",
            payload: {
              id: "property-invalid-options",
              space_id: "space-1",
              name: "Invalid Options",
              options: ["Ready", 42],
            },
          },
          {
            id: "remote-property-schema-invalid-order",
            entity_type: "property_schema",
            entity_id: "property-invalid-order",
            action: "update",
            payload_status: "available",
            payload: {
              id: "property-invalid-order",
              space_id: "space-1",
              name: "Invalid Order",
              sort_order: Number.POSITIVE_INFINITY,
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 3,
      operations: [
        {
          operation_id: "remote-property-schema-invalid-type",
          status: "failed",
          message: "Remote replay operation property_schema:create payload type is invalid",
        },
        {
          operation_id: "remote-property-schema-invalid-options",
          status: "failed",
          message: "Remote replay operation property_schema:update payload options are invalid",
        },
        {
          operation_id: "remote-property-schema-invalid-order",
          status: "failed",
          message: "Remote replay operation property_schema:update payload sort order is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 3,
      operations: [
        { operation_id: "remote-property-schema-invalid-type", apply_status: "skipped_failed" },
        { operation_id: "remote-property-schema-invalid-options", apply_status: "skipped_failed" },
        { operation_id: "remote-property-schema-invalid-order", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for malformed optional document property value metadata before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-property-value-metadata",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-property-value-invalid-id",
            entity_type: "document_property_values",
            entity_id: "doc-1",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "doc-1",
              space_id: "space-1",
              values: [{ id: 42, property_id: "property-1", value: "Ready" }],
            },
          },
          {
            id: "remote-property-value-invalid-created-at",
            entity_type: "document_property_values",
            entity_id: "doc-2",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "doc-2",
              space_id: "space-1",
              values: [{ property_id: "property-1", value: "Ready", created_at: [] }],
            },
          },
          {
            id: "remote-property-value-invalid-updated-at",
            entity_type: "document_property_values",
            entity_id: "doc-3",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "doc-3",
              space_id: "space-1",
              values: [{ property_id: "property-1", value: "Ready", updated_at: 123 }],
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 3,
      operations: [
        {
          operation_id: "remote-property-value-invalid-id",
          status: "failed",
          message:
            "Remote replay operation document_property_values:set payload includes invalid value metadata",
        },
        {
          operation_id: "remote-property-value-invalid-created-at",
          status: "failed",
          message:
            "Remote replay operation document_property_values:set payload includes invalid value metadata",
        },
        {
          operation_id: "remote-property-value-invalid-updated-at",
          status: "failed",
          message:
            "Remote replay operation document_property_values:set payload includes invalid value metadata",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 3,
      operations: [
        { operation_id: "remote-property-value-invalid-id", apply_status: "skipped_failed" },
        {
          operation_id: "remote-property-value-invalid-created-at",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-property-value-invalid-updated-at",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for malformed optional asset metadata before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-asset-metadata",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-asset-invalid-mime",
            entity_type: "asset",
            entity_id: "asset-invalid-mime",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-invalid-mime",
              space_id: "space-1",
              name: "invalid-mime.txt",
              size: 5,
              mime_type: ["text/plain"],
              data_url: "data:text/plain;base64,SGVsbG8=",
            },
          },
          {
            id: "remote-asset-invalid-created-at",
            entity_type: "asset",
            entity_id: "asset-invalid-created-at",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-invalid-created-at",
              space_id: "space-1",
              name: "invalid-created-at.txt",
              size: 5,
              created_at: 123,
              data_url: "data:text/plain;base64,SGVsbG8=",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 2,
      operations: [
        {
          operation_id: "remote-asset-invalid-mime",
          status: "failed",
          message: "Remote replay operation asset:create payload MIME type is invalid",
        },
        {
          operation_id: "remote-asset-invalid-created-at",
          status: "failed",
          message: "Remote replay operation asset:create payload created timestamp is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 2,
      operations: [
        { operation_id: "remote-asset-invalid-mime", apply_status: "skipped_failed" },
        { operation_id: "remote-asset-invalid-created-at", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for malformed optional document workflow metadata before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-document-workflow-metadata",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-document-workflow-invalid-creator",
            entity_type: "document_workflow",
            entity_id: "workflow-invalid-creator",
            action: "create",
            payload_status: "available",
            payload: {
              id: "workflow-invalid-creator",
              document_id: "doc-1",
              workflow_template_id: "template-1",
              current_stage_id: "review",
              status: "active",
              created_by: { id: "admin" },
            },
          },
          {
            id: "remote-document-workflow-invalid-created-at",
            entity_type: "document_workflow",
            entity_id: "workflow-invalid-created-at",
            action: "update",
            payload_status: "available",
            payload: {
              id: "workflow-invalid-created-at",
              document_id: "doc-1",
              workflow_template_id: "template-1",
              current_stage_id: "review",
              status: "active",
              created_at: 123,
            },
          },
          {
            id: "remote-document-workflow-invalid-updated-at",
            entity_type: "document_workflow",
            entity_id: "workflow-invalid-updated-at",
            action: "transition",
            payload_status: "available",
            payload: {
              id: "workflow-invalid-updated-at",
              document_id: "doc-1",
              workflow_template_id: "template-1",
              current_stage_id: "review",
              status: "active",
              updated_at: [],
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 3,
      operations: [
        {
          operation_id: "remote-document-workflow-invalid-creator",
          status: "failed",
          message: "Remote replay operation document_workflow:create payload creator is invalid",
        },
        {
          operation_id: "remote-document-workflow-invalid-created-at",
          status: "failed",
          message:
            "Remote replay operation document_workflow:update payload created timestamp is invalid",
        },
        {
          operation_id: "remote-document-workflow-invalid-updated-at",
          status: "failed",
          message:
            "Remote replay operation document_workflow:transition payload updated timestamp is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 3,
      operations: [
        {
          operation_id: "remote-document-workflow-invalid-creator",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-workflow-invalid-created-at",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-workflow-invalid-updated-at",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for property, asset, and workflow payloads missing required semantic fields", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-state-semantic-payload",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 7,
        operations: [
          {
            id: "remote-property-reorder-missing-order",
            entity_type: "property_schema",
            entity_id: "space-1",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "space-1",
            },
          },
          {
            id: "remote-property-create-missing-name",
            entity_type: "property_schema",
            entity_id: "property-create",
            action: "create",
            payload_status: "available",
            payload: {
              id: "property-create",
              space_id: "space-1",
            },
          },
          {
            id: "remote-property-reorder-wrong-space",
            entity_type: "property_schema",
            entity_id: "space-1",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "space-2",
              property_ids: ["property-1"],
            },
          },
          {
            id: "remote-document-property-values-missing-values",
            entity_type: "document_property_values",
            entity_id: "doc-1",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "doc-1",
              space_id: "space-1",
            },
          },
          {
            id: "remote-asset-create-missing-size",
            entity_type: "asset",
            entity_id: "asset-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-1",
              space_id: "space-1",
              name: "missing-size.png",
            },
          },
          {
            id: "remote-document-workflow-missing-status",
            entity_type: "document_workflow",
            entity_id: "workflow-1",
            action: "transition",
            payload_status: "available",
            payload: {
              id: "workflow-1",
              document_id: "doc-1",
              workflow_template_id: "template-1",
              current_stage_id: "review",
            },
          },
          {
            id: "remote-document-workflow-event-missing-created",
            entity_type: "document_workflow_event",
            entity_id: "event-1",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "event-1",
              document_workflow_id: "workflow-1",
              document_id: "doc-1",
              to_stage_id: "review",
              action: "delegate",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 7,
      operations: [
        {
          operation_id: "remote-property-reorder-missing-order",
          status: "failed",
          message:
            "Remote replay operation property_schema:reorder payload is missing space or property order",
        },
        {
          operation_id: "remote-property-create-missing-name",
          status: "failed",
          message:
            "Remote replay operation property_schema:create payload is missing space or name",
        },
        {
          operation_id: "remote-property-reorder-wrong-space",
          status: "failed",
          message:
            "Remote replay operation property_schema:reorder payload space does not match operation entity ID",
        },
        {
          operation_id: "remote-document-property-values-missing-values",
          status: "failed",
          message:
            "Remote replay operation document_property_values:set payload is missing document, space, or values",
        },
        {
          operation_id: "remote-asset-create-missing-size",
          status: "failed",
          message: "Remote replay operation asset:create payload is missing space, name, or size",
        },
        {
          operation_id: "remote-document-workflow-missing-status",
          status: "failed",
          message:
            "Remote replay operation document_workflow:transition payload is missing workflow state",
        },
        {
          operation_id: "remote-document-workflow-event-missing-created",
          status: "failed",
          message:
            "Remote replay operation document_workflow_event:delegate payload is missing event state",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 7,
      operations: [
        { operation_id: "remote-property-reorder-missing-order", apply_status: "skipped_failed" },
        { operation_id: "remote-property-create-missing-name", apply_status: "skipped_failed" },
        { operation_id: "remote-property-reorder-wrong-space", apply_status: "skipped_failed" },
        {
          operation_id: "remote-document-property-values-missing-values",
          apply_status: "skipped_failed",
        },
        { operation_id: "remote-asset-create-missing-size", apply_status: "skipped_failed" },
        { operation_id: "remote-document-workflow-missing-status", apply_status: "skipped_failed" },
        {
          operation_id: "remote-document-workflow-event-missing-created",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for empty required workflow payload fields before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-empty-workflow-fields",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-document-workflow-empty-document",
            entity_type: "document_workflow",
            entity_id: "workflow-empty-document",
            action: "update",
            payload_status: "available",
            payload: {
              id: "workflow-empty-document",
              document_id: " ",
              workflow_template_id: "template-1",
              current_stage_id: "review",
              status: "active",
            },
          },
          {
            id: "remote-document-workflow-event-empty-stage",
            entity_type: "document_workflow_event",
            entity_id: "event-empty-stage",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "event-empty-stage",
              document_workflow_id: "workflow-1",
              document_id: "doc-1",
              to_stage_id: "",
              action: "delegate",
              created_at: "2026-07-08T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 2,
      operations: [
        {
          operation_id: "remote-document-workflow-empty-document",
          status: "failed",
          message:
            "Remote replay operation document_workflow:update payload includes empty workflow state fields",
        },
        {
          operation_id: "remote-document-workflow-event-empty-stage",
          status: "failed",
          message:
            "Remote replay operation document_workflow_event:delegate payload includes empty event state fields",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 2,
      operations: [
        {
          operation_id: "remote-document-workflow-empty-document",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-workflow-event-empty-stage",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for asset and workflow payload values that cannot merge locally", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-asset-workflow-values",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-asset-create-external-url",
            entity_type: "asset",
            entity_id: "asset-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-1",
              space_id: "space-1",
              name: "remote.png",
              size: 128,
              url: "https://example.com/remote.png",
            },
          },
          {
            id: "remote-document-workflow-invalid-status",
            entity_type: "document_workflow",
            entity_id: "workflow-1",
            action: "update",
            payload_status: "available",
            payload: {
              id: "workflow-1",
              document_id: "doc-1",
              workflow_template_id: "template-1",
              current_stage_id: "review",
              status: "paused",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 2,
      operations: [
        {
          operation_id: "remote-asset-create-external-url",
          status: "failed",
          message:
            "Remote replay operation asset:create payload requires embedded data URL bytes",
        },
        {
          operation_id: "remote-document-workflow-invalid-status",
          status: "failed",
          message: "Remote replay operation document_workflow:update payload has invalid status",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 2,
      operations: [
        { operation_id: "remote-asset-create-external-url", apply_status: "skipped_failed" },
        { operation_id: "remote-document-workflow-invalid-status", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for invalid asset data URL bytes before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-asset-byte-values",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-asset-create-invalid-bytes",
            entity_type: "asset",
            entity_id: "asset-invalid",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-invalid",
              space_id: "space-1",
              name: "invalid.txt",
              size: 5,
              data_url: "data:text/plain;base64,%%%%",
            },
          },
          {
            id: "remote-asset-create-invalid-size",
            entity_type: "asset",
            entity_id: "asset-invalid-size",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-invalid-size",
              space_id: "space-1",
              name: "invalid-size.txt",
              size: -1,
              data_url: "data:text/plain;base64,SGVsbG8=",
            },
          },
          {
            id: "remote-asset-create-size-mismatch",
            entity_type: "asset",
            entity_id: "asset-mismatch",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-mismatch",
              space_id: "space-1",
              name: "mismatch.txt",
              size: 99,
              data_url: "data:text/plain;base64,SGVsbG8=",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 3,
      operations: [
        {
          operation_id: "remote-asset-create-invalid-bytes",
          status: "failed",
          message:
            "Remote replay operation asset:create payload includes invalid data URL bytes",
        },
        {
          operation_id: "remote-asset-create-invalid-size",
          status: "failed",
          message: "Remote replay operation asset:create payload size is invalid",
        },
        {
          operation_id: "remote-asset-create-size-mismatch",
          status: "failed",
          message:
            "Remote replay operation asset:create payload byte size does not match declared size",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 3,
      operations: [
        { operation_id: "remote-asset-create-invalid-bytes", apply_status: "skipped_failed" },
        { operation_id: "remote-asset-create-invalid-size", apply_status: "skipped_failed" },
        { operation_id: "remote-asset-create-size-mismatch", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for malformed optional asset and workflow context before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-optional-context",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-asset-create-invalid-document",
            entity_type: "asset",
            entity_id: "asset-invalid-document",
            action: "create",
            payload_status: "available",
            payload: {
              id: "asset-invalid-document",
              space_id: "space-1",
              name: "invalid-document.txt",
              size: 5,
              document_id: 123,
              data_url: "data:text/plain;base64,SGVsbG8=",
            },
          },
          {
            id: "remote-document-workflow-invalid-space",
            entity_type: "document_workflow",
            entity_id: "workflow-invalid-space",
            action: "update",
            payload_status: "available",
            payload: {
              id: "workflow-invalid-space",
              document_id: "doc-1",
              workflow_template_id: "template-1",
              current_stage_id: "review",
              status: "active",
              space_id: ["space-1"],
            },
          },
          {
            id: "remote-workflow-event-invalid-source",
            entity_type: "document_workflow_event",
            entity_id: "event-invalid-source",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "event-invalid-source",
              document_workflow_id: "workflow-1",
              document_id: "doc-1",
              from_stage_id: false,
              to_stage_id: "review",
              action: "delegate",
              created_at: "2026-07-08T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 3,
      operations: [
        {
          operation_id: "remote-asset-create-invalid-document",
          status: "failed",
          message: "Remote replay operation asset:create payload document is invalid",
        },
        {
          operation_id: "remote-document-workflow-invalid-space",
          status: "failed",
          message:
            "Remote replay operation document_workflow:update payload space is invalid",
        },
        {
          operation_id: "remote-workflow-event-invalid-source",
          status: "failed",
          message:
            "Remote replay operation document_workflow_event:delegate payload source stage is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 3,
      operations: [
        { operation_id: "remote-asset-create-invalid-document", apply_status: "skipped_failed" },
        {
          operation_id: "remote-document-workflow-invalid-space",
          apply_status: "skipped_failed",
        },
        { operation_id: "remote-workflow-event-invalid-source", apply_status: "skipped_failed" },
      ],
    });
  });

  it("fails dry-run for empty optional workflow context before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-empty-workflow-context",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-document-workflow-empty-space",
            entity_type: "document_workflow",
            entity_id: "workflow-empty-space",
            action: "update",
            payload_status: "available",
            payload: {
              id: "workflow-empty-space",
              document_id: "doc-1",
              workflow_template_id: "template-1",
              current_stage_id: "review",
              status: "active",
              space_id: " ",
            },
          },
          {
            id: "remote-workflow-event-empty-space",
            entity_type: "document_workflow_event",
            entity_id: "event-empty-space",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "event-empty-space",
              document_workflow_id: "workflow-1",
              document_id: "doc-1",
              to_stage_id: "review",
              action: "delegate",
              created_at: "2026-07-08T12:00:00.000Z",
              space_id: "",
            },
          },
          {
            id: "remote-workflow-event-empty-source",
            entity_type: "document_workflow_event",
            entity_id: "event-empty-source",
            action: "transition",
            payload_status: "available",
            payload: {
              id: "event-empty-source",
              document_workflow_id: "workflow-1",
              document_id: "doc-1",
              from_stage_id: " ",
              to_stage_id: "approved",
              action: "transition",
              created_at: "2026-07-08T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 3,
      operations: [
        {
          operation_id: "remote-document-workflow-empty-space",
          status: "failed",
          message:
            "Remote replay operation document_workflow:update payload space is invalid",
        },
        {
          operation_id: "remote-workflow-event-empty-space",
          status: "failed",
          message:
            "Remote replay operation document_workflow_event:delegate payload space is invalid",
        },
        {
          operation_id: "remote-workflow-event-empty-source",
          status: "failed",
          message:
            "Remote replay operation document_workflow_event:transition payload source stage is invalid",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 3,
      operations: [
        {
          operation_id: "remote-document-workflow-empty-space",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-workflow-event-empty-space",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-workflow-event-empty-source",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for malformed property arrays before apply selection", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-property-array-payload",
    );
    const dryRun = buildProviderRemoteImportDryRun(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-property-reorder-invalid-id",
            entity_type: "property_schema",
            entity_id: "space-1",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "space-1",
              property_ids: ["property-1", 42],
            },
          },
          {
            id: "remote-property-reorder-duplicate-id",
            entity_type: "property_schema",
            entity_id: "space-1",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "space-1",
              property_ids: ["property-1", "property-1"],
            },
          },
          {
            id: "remote-document-property-values-invalid-entry",
            entity_type: "document_property_values",
            entity_id: "doc-1",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "doc-1",
              space_id: "space-1",
              values: [null],
            },
          },
          {
            id: "remote-document-property-values-missing-property",
            entity_type: "document_property_values",
            entity_id: "doc-1",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "doc-1",
              space_id: "space-1",
              values: [{ value: "Ready" }],
            },
          },
          {
            id: "remote-document-property-values-duplicate",
            entity_type: "document_property_values",
            entity_id: "doc-1",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "doc-1",
              space_id: "space-1",
              values: [
                { property_id: "property-1", value: "Ready" },
                { property_id: "property-1", value: "Done" },
              ],
            },
          },
        ],
      }),
    );

    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 5,
      operations: [
        {
          operation_id: "remote-property-reorder-invalid-id",
          status: "failed",
          message:
            "Remote replay operation property_schema:reorder payload includes invalid property IDs",
        },
        {
          operation_id: "remote-property-reorder-duplicate-id",
          status: "failed",
          message:
            "Remote replay operation property_schema:reorder payload includes duplicate property IDs",
        },
        {
          operation_id: "remote-document-property-values-invalid-entry",
          status: "failed",
          message:
            "Remote replay operation document_property_values:set payload includes an invalid value entry",
        },
        {
          operation_id: "remote-document-property-values-missing-property",
          status: "failed",
          message:
            "Remote replay operation document_property_values:set payload includes a value without property or value",
        },
        {
          operation_id: "remote-document-property-values-duplicate",
          status: "failed",
          message:
            "Remote replay operation document_property_values:set payload includes duplicate property values",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 5,
      operations: [
        { operation_id: "remote-property-reorder-invalid-id", apply_status: "skipped_failed" },
        { operation_id: "remote-property-reorder-duplicate-id", apply_status: "skipped_failed" },
        {
          operation_id: "remote-document-property-values-invalid-entry",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-property-values-missing-property",
          apply_status: "skipped_failed",
        },
        {
          operation_id: "remote-document-property-values-duplicate",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("fails dry-run for workflow event replays with mismatched payload actions", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-payload-action-mismatch",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 1,
      operations: [
        {
          id: "remote-mismatched-action",
          entity_type: "document_workflow_event",
          entity_id: "event-1",
          action: "delegate",
          payload_status: "available",
          payload: {
            id: "event-1",
            document_workflow_id: "workflow-1",
            document_id: "doc-1",
            to_stage_id: "review",
            action: "escalate",
            created_at: "2026-07-08T12:00:00.000Z",
          },
        },
      ],
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest);
    expect(dryRun).toMatchObject({
      would_apply_count: 0,
      failed_count: 1,
      operations: [
        {
          operation_id: "remote-mismatched-action",
          status: "failed",
          message:
            "Remote replay operation document_workflow_event:delegate payload action does not match operation action",
        },
      ],
    });
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 1,
      operations: [
        {
          operation_id: "remote-mismatched-action",
          apply_status: "skipped_failed",
        },
      ],
    });
  });

  it("plans explicit remote replay import apply without mutating local data", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-apply-plan",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 3,
      operations: [
        {
          id: "remote-ready",
          entity_type: "document",
          entity_id: "doc-1",
          action: "update_content",
          payload_status: "available",
          payload: {
            id: "doc-1",
            space_id: "space-1",
            content: "remote draft",
          },
        },
        {
          id: "local-conflict",
          entity_type: "tag",
          entity_id: "tag-1",
          action: "create",
          payload_status: "available",
        },
        {
          id: "remote-missing",
          entity_type: "asset",
          entity_id: "asset-1",
          action: "create",
          payload_status: "missing",
        },
      ],
    });

    const dryRun = buildProviderRemoteImportDryRun(manifest, ["local-conflict"]);
    expect(buildProviderRemoteImportApplyPlan(dryRun)).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      apply_ready: false,
      selected_operation_count: 1,
      skipped_operation_count: 2,
      operations: [
        {
          operation_id: "remote-ready",
          source_status: "would_apply",
          apply_status: "selected",
        },
        {
          operation_id: "local-conflict",
          source_status: "conflict",
          apply_status: "skipped_conflict",
        },
        {
          operation_id: "remote-missing",
          source_status: "failed",
          apply_status: "skipped_failed",
        },
      ],
      blockers: expect.arrayContaining([
        "Remote provider replay import apply requires an explicit local merge executor",
      ]),
    });
  });

  it("blocks remote replay import execution unless local mutation is explicitly enabled", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-execute-blocked",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 1,
      operations: [
        {
          id: "remote-ready",
          entity_type: "document",
          entity_id: "doc-1",
          action: "update_content",
          payload_status: "available",
          payload: {
            id: "doc-1",
            space_id: "space-1",
            content: "remote draft",
          },
        },
      ],
    });
    const plan = buildProviderRemoteImportApplyPlan(buildProviderRemoteImportDryRun(manifest));

    await expect(
      executeProviderRemoteImportApplyPlan(plan, {
        executedAt: "2026-07-08T12:10:00.000Z",
      }),
    ).resolves.toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blocked_operation_count: 1,
      blockers: expect.arrayContaining([
        "Remote provider replay import apply execution requires explicit local mutation permission",
      ]),
      executed_at: "2026-07-08T12:10:00.000Z",
      results: [
        {
          operation_id: "remote-ready",
          status: "blocked",
        },
      ],
    });
  });

  it("preserves failed operation counts when disabled import execution has no selected operations", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-execute-disabled-failed-counts",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 1,
      operations: [
        {
          id: "remote-missing",
          entity_type: "asset",
          entity_id: "asset-1",
          action: "create",
          payload_status: "missing",
        },
      ],
    });
    const plan = buildProviderRemoteImportApplyPlan(buildProviderRemoteImportDryRun(manifest));

    await expect(
      executeProviderRemoteImportApplyPlan(plan, {
        executedAt: "2026-07-08T12:12:00.000Z",
      }),
    ).resolves.toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 1,
      blocked_operation_count: 0,
      blockers: expect.arrayContaining([
        "Remote provider replay import apply execution requires explicit local mutation permission",
      ]),
      executed_at: "2026-07-08T12:12:00.000Z",
      results: [
        {
          operation_id: "remote-missing",
          status: "failed",
          message: "Remote replay operation payload is not available",
        },
      ],
    });
  });

  it("executes only selected remote replay import operations through an injected merge executor", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-execute-selected",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 2,
      operations: [
        {
          id: "remote-ready",
          entity_type: "document",
          entity_id: "doc-1",
          action: "update_content",
          payload_status: "available",
          payload: {
            id: "doc-1",
            space_id: "space-1",
            content: "remote draft",
          },
        },
        {
          id: "remote-missing",
          entity_type: "asset",
          entity_id: "asset-1",
          action: "create",
          payload_status: "missing",
        },
      ],
    });
    const plan = buildProviderRemoteImportApplyPlan(buildProviderRemoteImportDryRun(manifest));
    const appliedOperationIds: string[] = [];

    await expect(
      executeProviderRemoteImportApplyPlan(plan, {
        allowLocalMutation: true,
        executedAt: "2026-07-08T12:15:00.000Z",
        mergeExecutor: {
          async applyOperation(operation) {
            appliedOperationIds.push(operation.operation_id);
            return {
              status: "applied",
              message: "Applied by injected local merge executor",
            };
          },
        },
      }),
    ).resolves.toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: [],
      executed_at: "2026-07-08T12:15:00.000Z",
      results: [
        {
          operation_id: "remote-ready",
          status: "applied",
        },
        {
          operation_id: "remote-missing",
          status: "skipped",
        },
      ],
    });
    expect(appliedOperationIds).toEqual(["remote-ready"]);
  });

  it("executes selected remote replay import operations sequentially in plan order", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-execute-sequential",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 2,
      operations: [
        {
          id: "remote-folder-create",
          entity_type: "folder",
          entity_id: "folder-1",
          action: "create",
          payload_status: "available",
          payload: {
            id: "folder-1",
            space_id: "space-1",
            name: "Remote Folder",
          },
        },
        {
          id: "remote-document-create",
          entity_type: "document",
          entity_id: "doc-1",
          action: "create",
          payload_status: "available",
          payload: {
            id: "doc-1",
            space_id: "space-1",
            folder_id: "folder-1",
            title: "Remote Document",
            content: "remote draft",
          },
        },
      ],
    });
    const plan = buildProviderRemoteImportApplyPlan(buildProviderRemoteImportDryRun(manifest));
    const events: string[] = [];

    await expect(
      executeProviderRemoteImportApplyPlan(plan, {
        allowLocalMutation: true,
        executedAt: "2026-07-08T12:20:00.000Z",
        mergeExecutor: {
          async applyOperation(operation) {
            events.push(`start:${operation.operation_id}`);
            await Promise.resolve();
            events.push(`end:${operation.operation_id}`);
            return {
              status: "applied",
              message: "Applied by injected local merge executor",
            };
          },
        },
      }),
    ).resolves.toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "completed",
      applied: true,
      attempted_operation_count: 2,
      applied_operation_count: 2,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-create",
          status: "applied",
        },
        {
          operation_id: "remote-document-create",
          status: "applied",
        },
      ],
    });
    expect(events).toEqual([
      "start:remote-folder-create",
      "end:remote-folder-create",
      "start:remote-document-create",
      "end:remote-document-create",
    ]);
  });

  it("blocks later selected remote replay import operations after an earlier selected apply failure", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-import-execute-halt-after-failure",
    );
    const manifest = buildProviderRemoteSyncManifest(envelope, {
      schema_version: 1,
      operation_count: 2,
      operations: [
        {
          id: "remote-folder-create",
          entity_type: "folder",
          entity_id: "folder-1",
          action: "create",
          payload_status: "available",
          payload: {
            id: "folder-1",
            space_id: "space-1",
            name: "Remote Folder",
          },
        },
        {
          id: "remote-document-create",
          entity_type: "document",
          entity_id: "doc-1",
          action: "create",
          payload_status: "available",
          payload: {
            id: "doc-1",
            space_id: "space-1",
            folder_id: "folder-1",
            title: "Remote Document",
            content: "remote draft",
          },
        },
      ],
    });
    const plan = buildProviderRemoteImportApplyPlan(buildProviderRemoteImportDryRun(manifest));
    const appliedOperationIds: string[] = [];

    await expect(
      executeProviderRemoteImportApplyPlan(plan, {
        allowLocalMutation: true,
        executedAt: "2026-07-08T12:25:00.000Z",
        mergeExecutor: {
          async applyOperation(operation) {
            appliedOperationIds.push(operation.operation_id);
            return {
              status: "failed",
              message: "Folder prerequisite could not be imported",
            };
          },
        },
      }),
    ).resolves.toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-folder-create",
          status: "failed",
          message: "Folder prerequisite could not be imported",
        },
        {
          operation_id: "remote-document-create",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        },
      ],
    });
    expect(appliedOperationIds).toEqual(["remote-folder-create"]);
  });

  it("blocks enabled provider mutation envelopes until transport is configured", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "mutation-ready",
    );

    expect(
      executeProviderRemoteMutationEnvelope(
        envelope,
        "provider-api",
        "2026-07-08T12:01:00.000Z",
      ),
    ).toEqual({
      schema_version: 1,
      provider: "google-drive",
      direction: "push",
      mutation_id: "mutation-ready",
      status: "blocked",
      transport_kind: "provider-api",
      applied: false,
      blockers: ["google-drive remote mutation transport is not configured yet"],
      attempted_at: "2026-07-08T12:01:00.000Z",
      request_spec: {
        schema_version: 1,
        provider: "google-drive",
        direction: "push",
        mutation_id: "mutation-ready",
        request_kind: "google-drive-file-upsert",
        target_root_strategy: "drive-appdata-or-user-selected-folder",
        payload_format: "starlane-provider-sync-manifest+json",
        method: "POST",
        endpoint: "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart",
        remote_path: "Starlane Notes/provider-sync-manifest.json",
        required_scopes: ["drive.file"],
        idempotency_key: "mutation-ready",
      },
    });
  });

  it("applies enabled Google Drive mutation envelopes through an explicit API transport", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-mutation-ready",
    );
    const calls: Array<{ input: string; init: RequestInit }> = [];

    const result = await executeProviderRemoteMutationEnvelopeWithTransport(
      envelope,
      "2026-07-08T12:01:00.000Z",
      {
        accessToken: "google-access-token",
        providerSyncPayload: {
          schema_version: 1,
          operation_count: 2,
          operations: [{ id: "queued-op-1" }],
        },
        apiTransport: {
          fetch: async (input, init) => {
            calls.push({ input, init });
            return {
              ok: true,
              status: 200,
              statusText: "OK",
              json: async () => ({ id: "drive-file-1", name: "provider-sync-manifest.json" }),
            };
          },
        },
      },
    );

    expect(calls).toHaveLength(1);
    expect(calls[0]?.input).toBe(
      "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart",
    );
    expect(calls[0]?.init.method).toBe("POST");
    expect(calls[0]?.init.headers).toMatchObject({
      Authorization: "Bearer google-access-token",
      "X-Starlane-Idempotency-Key": "gdrive-mutation-ready",
    });
    expect(String((calls[0]?.init.headers as Record<string, string>)["Content-Type"])).toContain(
      "multipart/related",
    );
    expect(String(calls[0]?.init.body)).toContain("gdrive-mutation-ready");
    expect(String(calls[0]?.init.body)).toContain("provider-sync-manifest.json");
    expect(String(calls[0]?.init.body)).toContain("starlane-provider-sync-manifest");
    expect(String(calls[0]?.init.body)).toContain('"operation_count": 2');
    expect(String(calls[0]?.init.body)).toContain('"queued-op-1"');
    expect(result).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      direction: "push",
      mutation_id: "gdrive-mutation-ready",
      status: "applied",
      transport_kind: "provider-api",
      applied: true,
      blockers: [],
      receipt: {
        id: "drive-file-1",
        endpoint: "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart",
        remote_path: "Starlane Notes/provider-sync-manifest.json",
      },
    });
  });

  it("applies Google Drive mutation envelopes with a ready manual provider session and root folder", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-manual-session-ready",
    );
    const calls: Array<{ input: string; init: RequestInit }> = [];

    const result = await executeProviderRemoteMutationEnvelopeWithTransport(
      envelope,
      "2026-07-08T12:01:00.000Z",
      {
        session: {
          provider: "google-drive",
          connected: true,
          auth: {
            access_token: "google-drive-access-real",
            refresh_token: "google-drive-refresh-real",
            account_label: "Google account",
            scopes: ["drive.file"],
            provider_root_id: "drive-root-folder-1",
            provider_root_name: "Starlane Notes",
            issued_at: "2026-07-08T12:00:00.000Z",
            expires_at: "2099-07-10T12:00:00.000Z",
          },
          sync_mode: "manual",
        },
        apiTransport: {
          fetch: async (input, init) => {
            calls.push({ input, init });
            return {
              ok: true,
              status: 200,
              json: async () => ({ id: "drive-file-2" }),
            };
          },
        },
      },
    );

    expect(calls).toHaveLength(1);
    expect(calls[0]?.init.headers).toMatchObject({
      Authorization: "Bearer google-drive-access-real",
    });
    expect(String(calls[0]?.init.body)).toContain('"parents":["drive-root-folder-1"]');
    expect(result).toMatchObject({
      status: "applied",
      transport_kind: "provider-api",
      applied: true,
      blockers: [],
      receipt: {
        id: "drive-file-2",
        remote_path: "Starlane Notes/provider-sync-manifest.json",
      },
    });
  });

  it("applies Google Drive pull envelopes through an explicit API transport", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "pull",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file", "drive.metadata.readonly"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file", "drive.metadata.readonly"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-pull-ready",
    );
    const calls: Array<{ input: string; init: RequestInit }> = [];

    const result = await executeProviderRemoteMutationEnvelopeWithTransport(
      envelope,
      "2026-07-08T12:01:00.000Z",
      {
        session: {
          provider: "google-drive",
          connected: true,
          auth: {
            access_token: "google-drive-access-real",
            refresh_token: "google-drive-refresh-real",
            account_label: "Google account",
            scopes: ["drive.file", "drive.metadata.readonly"],
            provider_root_id: "drive-root-folder-1",
            provider_root_name: "Starlane Notes",
            issued_at: "2026-07-08T12:00:00.000Z",
            expires_at: "2099-07-10T12:00:00.000Z",
          },
          sync_mode: "manual",
        },
        apiTransport: {
          fetch: async (input, init) => {
            calls.push({ input, init });
            if (String(input).includes("alt=media")) {
              return {
                ok: true,
                status: 200,
                json: async () => ({
                  schema_version: 1,
                  manifest_kind: "starlane-provider-sync-manifest",
                  envelope: {
                    schema_version: 1,
                    provider: "google-drive",
                    direction: "push",
                    mutation_id: "remote-provider-push-1",
                    mutation_kind: "provider-sync-push",
                    payload_format: "starlane-provider-sync-manifest+json",
                    target_root_strategy: "drive-appdata-or-user-selected-folder",
                    write_model: "local-sqlite-first-then-provider-api-push",
                    conflict_model: "provider-revision-plus-local-replay-queue-validation",
                    required_scopes: ["drive.file"],
                    remote_mutation_enabled: true,
                    dry_run: false,
                    blockers: [],
                    created_at: "2026-07-08T12:00:00.000Z",
                  },
                  replay_manifest: {
                    schema_version: 1,
                    operation_count: 1,
                    operation_summary: {
                      ready_operation_count: 1,
                      missing_payload_count: 0,
                    },
                    operations: [
                      {
                        id: "remote-ready",
                        entity_type: "document",
                        entity_id: "doc-remote",
                        action: "update_content",
                        payload_status: "available",
                        payload: {
                          id: "doc-remote",
                          space_id: "space-remote",
                          content: "remote draft",
                        },
                      },
                    ],
                  },
                  import_ready: true,
                  import_blockers: [],
                  created_at: "2026-07-08T12:00:00.000Z",
                }),
              };
            }
            return {
              ok: true,
              status: 200,
              json: async () => ({
                files: [
                  {
                    id: "drive-file-3",
                    name: "provider-sync-manifest.json",
                    modifiedTime: "2026-07-08T12:01:00.000Z",
                  },
                ],
              }),
            };
          },
        },
      },
    );

    expect(calls).toHaveLength(2);
    const requestedUrl = new URL(calls[0]?.input ?? "");
    expect(`${requestedUrl.origin}${requestedUrl.pathname}`).toBe(
      "https://www.googleapis.com/drive/v3/files",
    );
    expect(requestedUrl.searchParams.get("q")).toContain("provider-sync-manifest.json");
    expect(requestedUrl.searchParams.get("q")).toContain("'drive-root-folder-1' in parents");
    expect(calls[0]?.init.method).toBe("GET");
    expect(calls[0]?.init.headers).toMatchObject({
      Authorization: "Bearer google-drive-access-real",
      "X-Starlane-Idempotency-Key": "gdrive-pull-ready",
    });
    expect(calls[0]?.init.body).toBeUndefined();
    expect(calls[1]?.input).toBe(
      "https://www.googleapis.com/drive/v3/files/drive-file-3?alt=media",
    );
    expect(calls[1]?.init.method).toBe("GET");
    expect(calls[1]?.init.headers).toMatchObject({
      Authorization: "Bearer google-drive-access-real",
      "X-Starlane-Idempotency-Key": "gdrive-pull-ready",
    });
    expect(result).toMatchObject({
      schema_version: 1,
      provider: "google-drive",
      direction: "pull",
      mutation_id: "gdrive-pull-ready",
      status: "applied",
      transport_kind: "provider-api",
      applied: true,
      blockers: [],
      request_spec: {
        request_kind: "google-drive-file-download",
        method: "GET",
        endpoint: "https://www.googleapis.com/drive/v3/files",
      },
      receipt: {
        files: [
          {
            id: "drive-file-3",
            name: "provider-sync-manifest.json",
          },
        ],
        endpoint: "https://www.googleapis.com/drive/v3/files",
        remote_path: "Starlane Notes/provider-sync-manifest.json",
        manifest_endpoint: "https://www.googleapis.com/drive/v3/files/drive-file-3?alt=media",
      },
      pull_discovery: {
        schema_version: 1,
        provider: "google-drive",
        status: "found",
        remote_path: "Starlane Notes/provider-sync-manifest.json",
        import_ready: true,
        blockers: [],
        file: {
          id: "drive-file-3",
          name: "provider-sync-manifest.json",
          modified_time: "2026-07-08T12:01:00.000Z",
        },
      },
      remote_manifest: {
        schema_version: 1,
        provider: "google-drive",
        direction: "push",
        mutation_id: "remote-provider-push-1",
        payload_format: "starlane-provider-sync-manifest+json",
      },
      provider_sync_manifest: {
        schema_version: 1,
        manifest_kind: "starlane-provider-sync-manifest",
        import_ready: true,
        import_blockers: [],
        replay_manifest: {
          operation_count: 1,
        },
      },
      import_preflight: {
        schema_version: 1,
        provider: "google-drive",
        status: "ready_to_review",
        import_ready: true,
        replay_manifest_present: true,
        operation_count: 1,
        ready_operation_count: 1,
        missing_payload_count: 0,
        blockers: [],
      },
      import_dry_run: {
        schema_version: 1,
        provider: "google-drive",
        status: "blocked",
        import_ready: false,
        operation_count: 1,
        would_apply_count: 1,
        conflict_count: 0,
        failed_count: 0,
        unsupported_count: 0,
        operations: [
          {
            operation_id: "remote-ready",
            status: "would_apply",
          },
        ],
      },
      import_apply_plan: {
        schema_version: 1,
        provider: "google-drive",
        status: "blocked",
        apply_ready: false,
        selected_operation_count: 1,
        skipped_operation_count: 0,
        operations: [
          {
            operation_id: "remote-ready",
            source_status: "would_apply",
            apply_status: "selected",
          },
        ],
      },
    });
  });

  it("reports Google Drive pull discovery blockers when the provider manifest is missing", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "pull",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file", "drive.metadata.readonly"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file", "drive.metadata.readonly"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-pull-missing",
    );

    await expect(
      executeProviderRemoteMutationEnvelopeWithTransport(
        envelope,
        "2026-07-08T12:01:00.000Z",
        {
          accessToken: "google-drive-access-real",
          apiTransport: {
            fetch: async () => ({
              ok: true,
              status: 200,
              json: async () => ({ files: [] }),
            }),
          },
        },
      ),
    ).resolves.toMatchObject({
      status: "applied",
      applied: true,
      pull_discovery: {
        status: "not_found",
        import_ready: false,
        blockers: ["Google Drive provider-sync manifest was not found in the selected root"],
      },
    });
  });

  it("blocks Google Drive pull when downloaded provider manifest content is invalid", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "pull",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file", "drive.metadata.readonly"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file", "drive.metadata.readonly"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-pull-invalid",
    );

    await expect(
      executeProviderRemoteMutationEnvelopeWithTransport(
        envelope,
        "2026-07-08T12:01:00.000Z",
        {
          accessToken: "google-drive-access-real",
          apiTransport: {
            fetch: async (input) =>
              String(input).includes("alt=media")
                ? {
                    ok: true,
                    status: 200,
                    json: async () => ({ invalid: true }),
                  }
                : {
                    ok: true,
                    status: 200,
                    json: async () => ({
                      files: [{ id: "drive-file-4", name: "provider-sync-manifest.json" }],
                    }),
                  },
          },
        },
      ),
    ).resolves.toMatchObject({
      status: "blocked",
      applied: false,
      blockers: [
        "Google Drive provider manifest content is not a valid Starlane provider sync manifest",
      ],
      pull_discovery: {
        status: "found",
        import_ready: true,
      },
    });
  });

  it("blocks Google Drive transport when an access token is not supplied", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-mutation-ready",
    );

    await expect(
      executeProviderRemoteMutationEnvelopeWithTransport(
        envelope,
        "2026-07-08T12:01:00.000Z",
        {
          apiTransport: {
            fetch: async () => {
              throw new Error("should not fetch without a token");
            },
          },
        },
      ),
    ).resolves.toMatchObject({
      status: "blocked",
      transport_kind: "provider-api",
      applied: false,
      blockers: ["Google Drive provider API transport requires an access token"],
    });
  });

  it("blocks Google Drive transport when a manual provider session lacks required scopes", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file", "drive.metadata.readonly"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-missing-scopes",
    );

    await expect(
      executeProviderRemoteMutationEnvelopeWithTransport(
        envelope,
        "2026-07-08T12:01:00.000Z",
        {
          session: {
            provider: "google-drive",
            connected: true,
            auth: {
              access_token: "google-drive-access-real",
              refresh_token: "google-drive-refresh-real",
              account_label: "Google account",
              scopes: ["drive.file"],
              issued_at: "2026-07-08T12:00:00.000Z",
              expires_at: "2099-07-10T12:00:00.000Z",
            },
            sync_mode: "manual",
          },
          apiTransport: {
            fetch: async () => {
              throw new Error("should not fetch with missing scopes");
            },
          },
        },
      ),
    ).resolves.toMatchObject({
      status: "blocked",
      transport_kind: "provider-api",
      applied: false,
      blockers: ["Google Drive provider API transport missing scopes: drive.metadata.readonly"],
    });
  });

  it("blocks Google Drive transport when a manual provider token is expired", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-expired-token",
    );

    await expect(
      executeProviderRemoteMutationEnvelopeWithTransport(
        envelope,
        "2026-07-08T12:01:00.000Z",
        {
          session: {
            provider: "google-drive",
            connected: true,
            auth: {
              access_token: "google-drive-access-expired",
              refresh_token: "google-drive-refresh-real",
              account_label: "Google account",
              scopes: ["drive.file"],
              issued_at: "2026-07-07T12:00:00.000Z",
              expires_at: "2026-07-08T12:00:30.000Z",
            },
            sync_mode: "manual",
          },
          apiTransport: {
            fetch: async () => {
              throw new Error("should not fetch with expired token");
            },
          },
        },
      ),
    ).resolves.toMatchObject({
      status: "blocked",
      transport_kind: "provider-api",
      applied: false,
      blockers: ["Google Drive provider API transport requires an unexpired access token"],
    });
  });

  it("blocks Google Drive transport for simulated provider sessions", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "gdrive-mutation-ready",
    );

    await expect(
      executeProviderRemoteMutationEnvelopeWithTransport(
        envelope,
        "2026-07-08T12:01:00.000Z",
        {
          session: {
            provider: "google-drive",
            connected: true,
            auth: {
              access_token: "google-drive-access-simulated",
              refresh_token: "google-drive-refresh-simulated",
              account_label: "Google account",
              scopes: ["drive.file"],
              issued_at: "2026-07-08T12:00:00.000Z",
              expires_at: "2099-07-10T12:00:00.000Z",
            },
            sync_mode: "simulated",
          },
          apiTransport: {
            fetch: async () => {
              throw new Error("should not fetch with a simulated session");
            },
          },
        },
      ),
    ).resolves.toMatchObject({
      status: "blocked",
      transport_kind: "provider-api",
      applied: false,
      blockers: ["Google Drive provider API transport requires a real provider auth session"],
    });
  });

  it("builds native iCloud write request specs from provider mutation envelopes", () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "icloud-drive",
        "iCloud Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "native-ubiquity-documents-container",
          auth_model: "apple-account-via-native-container",
          required_scopes: ["icloud.documents"],
          write_model: "local-sqlite-first-then-provider-file-push",
          conflict_model: "provider-version-plus-local-replay-queue-validation",
        },
        {
          provider: "icloud-drive",
          connected: true,
          account_label: "iCloud account",
          scopes: ["icloud.documents"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "icloud-mutation-ready",
    );

    expect(buildProviderRemoteWriteRequestSpec(envelope)).toEqual({
      schema_version: 1,
      provider: "icloud-drive",
      direction: "push",
      mutation_id: "icloud-mutation-ready",
      request_kind: "icloud-drive-file-upsert",
      target_root_strategy: "native-ubiquity-documents-container",
      payload_format: "starlane-provider-sync-manifest+json",
      native_command: "write_icloud_provider_sync_manifest",
      remote_path: "Starlane Notes/provider-sync-manifest.json",
      required_scopes: ["icloud.documents"],
      idempotency_key: "icloud-mutation-ready",
    });
  });

  it("applies enabled iCloud mutation envelopes through the native transport command", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "icloud-drive",
        "iCloud Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "native-ubiquity-documents-container",
          auth_model: "apple-account-via-native-container",
          required_scopes: ["icloud.documents"],
          write_model: "local-sqlite-first-then-provider-file-push",
          conflict_model: "provider-version-plus-local-replay-queue-validation",
        },
        {
          provider: "icloud-drive",
          connected: true,
          account_label: "iCloud account",
          scopes: ["icloud.documents"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "icloud-mutation-ready",
    );
    const calls: Array<{ cmd: string; args?: Record<string, unknown> }> = [];

    const result = await executeProviderRemoteMutationEnvelopeWithTransport(
      envelope,
      "2026-07-08T12:01:00.000Z",
      {
        invoke: async (cmd, args) => {
          calls.push({ cmd, args });
          return {
            path: "/iCloud/Starlane Notes/.starlane/provider-sync/icloud-mutation-ready.json",
            relative_path: ".starlane/provider-sync/icloud-mutation-ready.json",
            bytes_written: 512,
          } as never;
        },
      },
    );

    expect(calls).toEqual([
      {
        cmd: "write_icloud_provider_sync_manifest",
        args: {
          mutationId: "icloud-mutation-ready",
          manifest: envelope,
        },
      },
    ]);
    expect(result).toMatchObject({
      schema_version: 1,
      provider: "icloud-drive",
      direction: "push",
      mutation_id: "icloud-mutation-ready",
      status: "applied",
      transport_kind: "provider-file",
      applied: true,
      blockers: [],
      receipt: {
        relative_path: ".starlane/provider-sync/icloud-mutation-ready.json",
        bytes_written: 512,
      },
    });
  });

  it("blocks enabled iCloud mutation envelopes outside the native runtime", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "icloud-drive",
        "iCloud Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "native-ubiquity-documents-container",
          auth_model: "apple-account-via-native-container",
          required_scopes: ["icloud.documents"],
          write_model: "local-sqlite-first-then-provider-file-push",
          conflict_model: "provider-version-plus-local-replay-queue-validation",
        },
        {
          provider: "icloud-drive",
          connected: true,
          account_label: "iCloud account",
          scopes: ["icloud.documents"],
          sync_mode: "manual",
        },
      ),
      "2026-07-08T12:00:00.000Z",
      "icloud-mutation-ready",
    );

    await expect(
      executeProviderRemoteMutationEnvelopeWithTransport(
        envelope,
        "2026-07-08T12:01:00.000Z",
      ),
    ).resolves.toMatchObject({
      status: "blocked",
      transport_kind: "provider-file",
      applied: false,
      blockers: ["iCloud provider transport requires the Tauri native command runtime"],
    });
  });
});
