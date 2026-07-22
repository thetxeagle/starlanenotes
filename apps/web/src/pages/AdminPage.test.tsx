import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

const mocks = vi.hoisted(() => ({
  auth: {
    user: null as null | {
      id: string;
      role: string;
      display_name: string;
      email: string;
    },
  },
  launch: {
    mode: "server" as "server" | "local",
  },
  provider: {
    kind: "none" as "none" | "icloud-drive" | "google-drive",
  },
  runtime: {
    target: "web" as "web" | "tauri-desktop" | "tauri-mobile",
  },
}));

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    user: mocks.auth.user,
    config: null,
    loading: false,
    login: vi.fn(),
    register: vi.fn(),
    updateProfile: vi.fn(),
    logout: vi.fn(),
  }),
}));

vi.mock("@/lib/launchMode", () => ({
  getCurrentLaunchMode: () => mocks.launch.mode,
}));

vi.mock("@/lib/storageProviders", () => ({
  clearStorageSelection: vi.fn(),
  getSyncProvider: () => mocks.provider.kind,
  setStorageMode: vi.fn(),
  setSyncProvider: vi.fn(),
}));

vi.mock("@/lib/providerSession", async () => {
  const actual = await vi.importActual<typeof import("@/lib/providerSession")>(
    "@/lib/providerSession",
  );
  return {
    ...actual,
    readProviderSession: () => ({
      provider: "google-drive",
      connected: true,
      auth: {
        access_token: "google-drive-access-simulated",
        refresh_token: "google-drive-refresh-simulated",
        account_label: "Google account",
        scopes: ["drive.file"],
        issued_at: "2026-07-08T12:00:00.000Z",
        expires_at: "2026-07-08T13:00:00.000Z",
      },
      sync_mode: "simulated",
    }),
  };
});

vi.mock("@/hooks/useRuntimePlatform", () => ({
  useRuntimePlatform: () => ({
    runtime: mocks.runtime.target,
    loading: false,
    isMobileRuntime: mocks.runtime.target === "tauri-mobile",
    isDesktopRuntime: mocks.runtime.target === "tauri-desktop",
  }),
}));

vi.mock("@/lib/dataClient", () => ({
  dataClient: {
    getProviderStatus: vi.fn(async () => ({
      connected: true,
      remote_contract: {
        status: "not_applicable",
        root_strategy: "local-vault",
        auth_model: "none",
        required_scopes: [],
        write_model: "local-only",
        conflict_model: "none",
      },
      remote_enablement: {
        provider: "none",
        status: "not_applicable",
        transport_kind: "none",
        enablement_boundary: "none",
        default_sync_enabled: false,
        required_scopes: [],
        blockers: [],
        steps: [],
      },
      remote_sync_enablement: {
        provider: "none",
        enabled: false,
        enablement_boundary: "explicit_manual_provider_session",
      },
      default_sync_policy: {
        provider: "none",
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
      },
    })),
    syncWithProvider: vi.fn(),
    enableProviderRemoteSync: vi.fn(),
    disableProviderRemoteSync: vi.fn(),
    simulateSyncConflict: vi.fn(),
    clearSyncIssue: vi.fn(),
    getProviderSyncReadiness: vi.fn(async () => ({
      provider: "none",
      connected: true,
      ready: true,
      sync_state: "idle",
      queued_operation_count: 0,
      ready_replay_operation_count: 0,
      missing_payload_count: 0,
      pending_conflict_count: 0,
      blockers: [],
      remote_mutation_plans: [],
    })),
    connectProvider: vi.fn(),
    disconnectProvider: vi.fn(),
    resolveSyncConflict: vi.fn(),
    getLocalVaultStats: vi.fn(async () => ({
      spaces: 1,
      folders: 2,
      documents: 3,
      assets: 4,
      asset_bytes: 2048,
    })),
    exportLocalVaultSnapshot: vi.fn(),
    exportQueuedSyncReplayManifest: vi.fn(async () => ({
      schema_version: 3,
      exported_at: "2026-07-06T00:00:00.000Z",
      provider: "none",
      replay_contract: {},
      replay_readiness: {},
      operation_count: 0,
      operation_summary: {
        total_operations: 0,
        missing_payload_count: 0,
        ready_operation_count: 0,
        by_entity_type: {},
        by_action: {},
        unavailable_payloads: [],
      },
      recovery_checkpoint_count: 0,
      recovery_checkpoints: [],
      operations: [],
    })),
    preflightQueuedSyncReplayManifest: vi.fn(async () => ({
      status: "ready",
      provider: "none",
      ready: true,
      operation_count: 0,
      ready_operation_count: 0,
      blocked_operation_count: 0,
      recovery_checkpoint_count: 0,
      target_backends: ["sqlite", "postgresql"],
      blockers: [],
    })),
    runQueuedSyncReplay: vi.fn(async () => ({
      status: "completed",
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      unsupported_operation_count: 0,
      results: [],
    })),
    listQueuedSyncReplayHistory: vi.fn(async () => ({ manifests: [] })),
    applyProviderRemoteImportManifest: vi.fn(async () => ({
      schema_version: 1,
      provider: "google-drive",
      status: "completed",
      applied: true,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blocked_operation_count: 0,
      blockers: [],
      executed_at: "2026-07-09T12:00:00.000Z",
      results: [],
    })),
    importLocalVaultSnapshot: vi.fn(),
    exportLocalVaultArchive: vi.fn(),
    importLocalVaultArchive: vi.fn(),
    listQueuedSyncOperations: vi.fn(async () => []),
    clearQueuedSyncOperations: vi.fn(),
    listSpaces: vi.fn(async () => []),
    exportServerSpaceSnapshot: vi.fn(),
  },
}));

vi.mock("@/lib/tauriVault", () => ({
  canUseNativeVaultControls: (runtime: string) => runtime === "tauri-desktop",
  chooseVaultArchiveExportPath: vi.fn(),
  chooseVaultArchiveImportPath: vi.fn(),
  chooseVaultFolder: vi.fn(),
  clearVaultPath: vi.fn(),
  getVaultFolderName: (path?: string) => {
    const parts = path?.split(/[\\/]/).filter(Boolean) ?? [];
    return parts[parts.length - 1] ?? "Starlane Notes Vault";
  },
  getVaultLocation: vi.fn(),
  revealVaultFolder: vi.fn(),
  setVaultFolderName: vi.fn(),
  setVaultPath: vi.fn(),
}));

import {
  ADMIN_COMPACT_ERROR_CLASS,
  ADMIN_ACTIVE_SPACE_NAME_LABEL,
  ADMIN_DANGER_ICON_BUTTON_CLASS,
  ADMIN_ERROR_CARD_CLASS,
  ADMIN_LOCAL_SYNC_ERROR_CARD_CLASS,
  ADMIN_MOBILE_BACK_BUTTON_CLASS,
  ADMIN_MOBILE_HOME_CLASS,
  ADMIN_MOBILE_SECTION_LIST_CLASS,
  ADMIN_MOBILE_SECTION_ROW_CLASS,
  ADMIN_PROVIDER_BLOCKER_TEXT_CLASS,
  ADMIN_PROVIDER_SUCCESS_TEXT_CLASS,
  ADMIN_PLUGIN_ERROR_CARD_CLASS,
  ADMIN_PLUGIN_SUCCESS_CARD_CLASS,
  ADMIN_RENAME_ACTIVE_SPACE_LABEL,
  ADMIN_SECRET_SUCCESS_CARD_CLASS,
  ADMIN_SECRET_SUCCESS_TITLE_CLASS,
  ADMIN_SUCCESS_CARD_CLASS,
  AdminCompactSelect,
  AdminFormSelect,
  AdminInlineSelect,
  AdminPage,
  AdminJsonTextarea,
  AdminTextInput,
  ProviderImportOperationReviewList,
  canRevealAdminVaultFolder,
  formatAdminReplayResultSummary,
  formatHiddenBlockerCount,
  formatProviderImportApplyPlanSummary,
  formatProviderImportApplyResultSummary,
  formatProviderImportAttemptProvenance,
  formatProviderImportHiddenBlockerCount,
  formatProviderImportHiddenOperationCount,
  formatProviderImportOperationLabel,
  formatProviderImportOperationStatus,
  formatProviderImportPullReviewSummary,
  formatProviderImportUnsupportedSummary,
  formatProviderRemoteSyncEnablementLabel,
  formatProviderValidationEvidenceLabel,
  formatGoogleDriveProductionOAuthEvidenceAction,
  filterAdminUsers,
  formatAdminUserBulkLifecycleMessage,
  getProviderImportApplyBlockedOperationCount,
  getProviderImportOperationStatusClass,
  getProviderImportReviewStateAfterProviderConnectionChange,
  getProviderImportReviewStateAfterSync,
  getGoogleDriveFixtureDisclosureLabel,
  getGoogleDriveManualSetupStepStates,
  getGoogleDriveManualSetupNextStepLabel,
  AdminWebhookDeliveryHistoryTable,
  buildAdminUserBulkLifecycleRequest,
  getAdminUserSelectionAfterVisibleToggle,
  getAdminUserLifecycleCounts,
  getAdminSessionDeviceLabel,
  getAdminSectionDescription,
  getAdminSessionRevokeConfirmationMessage,
  getAdminQueuedSyncClearConfirmationMessage,
  getAdminQueuedSyncReplayConfirmationMessage,
  getAdminStorageResetConfirmationMessage,
  getAdminVaultArchiveImportConfirmationMessage,
  getAdminVaultSnapshotImportConfirmationMessage,
  getAdminProviderSpaceRecoveryConfirmationMessage,
  getAdminUserVisibleSelectionState,
  getAdminWebhookDeliveryResetState,
  getAdminUserLifecycleFailureState,
  getAdminUserPasswordResetSessionState,
  getAdminUserSessionPanelClosedState,
  getAdminProviderReadinessBadgeClass,
  getAdminUserStatusBadgeClass,
  getAdminWebhookDeliveryStatusClass,
  getPluginPermissionReviewClass,
  isAdminSectionAvailable,
  getProviderImportApplyReviewState,
  getProviderImportNextActionLabel,
  parseAdminUserCsv,
  parseGoogleDriveTokenFixture,
  parseWebhookEvents,
  RoleSelect,
  sortAdminLocalSpaces,
  validateGoogleDriveOAuthLaunchUrl,
  UserRoleSelect,
} from "./AdminPage";

describe("AdminPage", () => {
  afterEach(() => {
    mocks.auth.user = null;
    mocks.launch.mode = "server";
    mocks.provider.kind = "none";
    mocks.runtime.target = "web";
  });

  it("shows the access guard in server mode for non-admin users", () => {
    mocks.auth.user = {
      id: "member-user",
      role: "member",
      display_name: "Member",
      email: "member@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage />);
    expect(html).toContain("Admin access required");
  });

  it("shows local-mode sync controls in the system section", () => {
    mocks.launch.mode = "local";
    mocks.provider.kind = "google-drive";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="system" />);
    expect(html).toContain(
      "Local mode is active. Server-admin actions are unavailable in this mode.",
    );
    expect(html).toContain("Connect Provider");
    expect(html).toContain("Disconnect Provider");
    expect(html).toContain("Resolve: Merge");
    expect(html).toContain("Provider Connection");
    expect(html).toContain("Provider Readiness");
    expect(html).toContain("queued replay payload readiness");
    expect(html).toContain("Provider Remote Contract");
    expect(html).toContain("Default provider sync is still disabled");
    expect(html).toContain("Remote Enablement Plan");
    expect(html).toContain("Default sync: disabled");
    expect(html).toContain("Google Drive Manual Connection");
    expect(html).toContain("OAuth exchange boundary: ready");
    expect(html).toContain("Root execution boundary: ready");
    expect(html).toContain("Refresh token:");
    expect(html).toContain("Default sync: disabled");
    expect(html).toContain("Manual Setup Plan");
    expect(html).toContain("Public client ID: missing");
    expect(html).toContain(
      "Production evidence: Resolve setup blockers before OAuth evidence can be recorded",
    );
    expect(html).toContain("Callback handling: state-validated request");
    expect(html).toContain("Token exchange: injected transport");
    expect(html).toContain("Root setup: injected API transport");
    expect(html).toContain("1. Generate preview");
    expect(html).toContain("2. Open Google OAuth");
    expect(html).toContain("3. Validate callback");
    expect(html).toContain("4. Complete connection");
    expect(html).toContain("Blocked");
    expect(html).toContain("Pending");
    expect(html).toContain("Google Drive OAuth callback URL");
    expect(html).toContain("Validate Callback");
    expect(html).toContain("Open Google OAuth");
    expect(html).toContain("Generate Setup Preview");
    expect(html.indexOf("Generate Setup Preview")).toBeLessThan(
      html.indexOf("Google Drive OAuth callback URL"),
    );
    expect(html.indexOf("Open Google OAuth")).toBeLessThan(
      html.indexOf("Google Drive OAuth callback URL"),
    );
    expect(html).toContain("Provider Remote Import Review");
    expect(html).toContain("Run Test Pull to download and review a provider-sync manifest.");
    expect(html).toContain("Next import action: Run Test Pull");
    expect(html).toContain("Run Test Pull");
    expect(html).toContain("Apply Last Import");
  });

  it("keeps a desktop route back to Notes in the administration rail", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage />);
    expect(html).toContain('href="/"');
    expect(html).toContain("Back to Notes");
  });

  it("renders mobile administration as a settings home with subpage rows", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="auth" />);
    expect(html).toContain("Site Administration");
    expect(html).toContain("Each row opens as its own mobile subpage.");
    expect(html).toContain("2FA, signup, OIDC, and login policy.");
    expect(html).toContain(ADMIN_MOBILE_HOME_CLASS);
    expect(html).toContain(ADMIN_MOBILE_SECTION_LIST_CLASS);
    expect(html).toContain(ADMIN_MOBILE_SECTION_ROW_CLASS);
    expect(html).toContain(ADMIN_MOBILE_BACK_BUTTON_CLASS);
    expect(getAdminSectionDescription("auth", false)).toBe(
      "2FA, signup, OIDC, and login policy.",
    );
  });

  it("derives Google Drive manual setup step state labels", () => {
    expect(
      getGoogleDriveManualSetupStepStates({
        setupPlanStatus: "blocked",
        hasBootstrap: false,
      }),
    ).toEqual(["blocked", "pending", "pending", "pending"]);
    expect(
      getGoogleDriveManualSetupStepStates({
        setupPlanStatus: "ready_to_start",
        hasBootstrap: true,
      }),
    ).toEqual(["ready", "next", "pending", "pending"]);
    expect(
      getGoogleDriveManualSetupStepStates({
        setupPlanStatus: "ready_to_start",
        hasBootstrap: true,
        callbackStatus: "ready_for_exchange",
        connectionPlanStatus: "ready_to_complete",
      }),
    ).toEqual(["ready", "ready", "ready", "next"]);
    expect(getGoogleDriveManualSetupNextStepLabel(["blocked", "pending"])).toBe(
      "Generate setup preview blocked",
    );
    expect(getGoogleDriveManualSetupNextStepLabel(["ready", "next", "pending"])).toBe(
      "Open Google OAuth",
    );
    expect(getGoogleDriveManualSetupNextStepLabel(["ready", "ready", "next"])).toBe(
      "Validate callback",
    );
    expect(getGoogleDriveManualSetupNextStepLabel(["ready", "ready", "ready", "next"])).toBe(
      "Complete connection",
    );
    expect(getGoogleDriveManualSetupNextStepLabel(["ready", "ready", "ready", "ready"])).toBe(
      "Setup complete",
    );
    expect(getGoogleDriveFixtureDisclosureLabel(false)).toBe("Show Fixture Inputs");
    expect(getGoogleDriveFixtureDisclosureLabel(true)).toBe("Hide Fixture Inputs");
  });

  it("formats provider import operation review rows", () => {
    const operation = {
      entity_type: "document",
      action: "update_content",
      entity_id: "doc-1",
      status: "would_apply" as const,
      message: "Remote document content would be merged",
    };

    expect(formatProviderImportOperationLabel(operation)).toBe("document:update_content doc-1");
    expect(formatProviderImportOperationStatus(operation)).toBe(
      "Ready to apply - Remote document content would be merged",
    );
    expect(getProviderImportOperationStatusClass(operation)).toBe("text-success");
    expect(
      getProviderImportOperationStatusClass({
        apply_status: "skipped_failed" as const,
      }),
    ).toBe("text-danger");
    expect(
      formatProviderImportOperationStatus({
        status: "unsupported" as const,
        entity_type: "webhook",
        action: "create",
        message: "Remote replay operation webhook:create cannot be merged locally",
      }),
    ).toBe("Server-only - Remote replay operation webhook:create cannot be merged locally");
    expect(
      formatProviderImportOperationStatus({
        status: "unsupported" as const,
        entity_type: "space",
        action: "delete",
        message: "Remote replay operation space:delete requires confirmed recovery",
      }),
    ).toBe("Recovery required - Remote replay operation space:delete requires confirmed recovery");
    expect(
      formatProviderImportOperationStatus({
        status: "unsupported" as const,
        message: "Remote replay operation comment:create is server-only and cannot be imported into a local vault",
      }),
    ).toBe(
      "Server-only - Remote replay operation comment:create is server-only and cannot be imported into a local vault",
    );
    expect(
      getProviderImportOperationStatusClass({
        apply_status: "skipped_unsupported" as const,
      }),
    ).toBe("text-warning");
    expect(
      formatProviderImportOperationStatus({
        apply_status: "skipped_unsupported" as const,
        message: "Remote replay operation comment:create is server-only and cannot be imported into a local vault",
      }),
    ).toBe(
      "Server-only skipped - Remote replay operation comment:create is server-only and cannot be imported into a local vault",
    );
    expect(
      formatProviderImportOperationStatus({
        status: "unsupported" as const,
        message:
          "Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow",
      }),
    ).toBe(
      "Recovery required - Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow",
    );
    expect(
      formatProviderImportOperationStatus({
        apply_status: "skipped_unsupported" as const,
        message:
          "Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow",
      }),
    ).toBe(
      "Recovery required skipped - Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow",
    );
    expect(formatProviderImportHiddenOperationCount(5)).toBeNull();
    expect(formatProviderImportHiddenOperationCount(6)).toBe("1 more operation not shown");
    expect(formatProviderImportHiddenOperationCount(8)).toBe("3 more operations not shown");
    expect(formatHiddenBlockerCount(3)).toBeNull();
    expect(formatHiddenBlockerCount(4)).toBe("1 more blocker not shown");
    expect(formatHiddenBlockerCount(6)).toBe("3 more blockers not shown");
    expect(formatHiddenBlockerCount(4, 2)).toBe("2 more blockers not shown");
    expect(formatProviderImportHiddenBlockerCount(3)).toBeNull();
    expect(formatProviderImportHiddenBlockerCount(4)).toBe("1 more blocker not shown");
    expect(formatProviderImportHiddenBlockerCount(6)).toBe("3 more blockers not shown");
    expect(formatProviderImportUnsupportedSummary(0)).toBe("0 unsupported");
    expect(
      formatProviderImportUnsupportedSummary(1, [
        {
          status: "unsupported",
          message:
            "Remote replay operation comment:create is server-only and cannot be imported into a local vault",
        },
      ]),
    ).toBe("1 server-only");
    expect(
      formatProviderImportUnsupportedSummary(5, [
        {
          status: "unsupported",
          entity_type: "webhook",
          action: "create",
          message: "Remote replay operation webhook:create cannot be merged locally",
        },
        {
          status: "unsupported",
          entity_type: "space",
          action: "delete",
          message: "Remote replay operation space:delete requires confirmed recovery",
        },
        {
          status: "unsupported",
          message:
            "Remote replay operation comment:create is server-only and cannot be imported into a local vault",
        },
        {
          status: "unsupported",
          message:
            "Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow",
        },
        {
          status: "unsupported",
          message:
            "Remote replay operation document:publish is server-only and cannot be imported into a local vault",
        },
      ]),
    ).toBe("3 server-only, 2 recovery-required");
  });

  it("renders readable provider import operation review rows", () => {
    const html = renderToStaticMarkup(
      <ProviderImportOperationReviewList
        title="Apply execution results"
        visibleLimit={1}
        operations={[
          {
            operation_id: "op-1",
            entity_type: "document",
            action: "create",
            entity_id: "doc-with-long-provider-id",
            status: "blocked" as const,
            message: "Remote document create target already exists locally with different content",
          },
          {
            operation_id: "op-2",
            entity_type: "folder",
            action: "delete",
            entity_id: "folder-2",
            status: "skipped" as const,
            message: "Remote folder delete target is already absent locally",
          },
        ]}
      />,
    );

    expect(html).toContain("Apply execution results");
    expect(html).toContain("document:create doc-with-long-provider-id");
    expect(html).toContain("Operation ID: op-1");
    expect(html).toContain(
      "Blocked - Remote document create target already exists locally with different content",
    );
    expect(html).toContain("break-words");
    expect(html).toContain("1 more operation not shown");
  });

  it("formats provider validation evidence labels", () => {
    expect(formatProviderValidationEvidenceLabel(false)).toBe("not verified");
    expect(formatProviderValidationEvidenceLabel(true)).toBe("verified");
    expect(formatProviderValidationEvidenceLabel(true, "invalid-date")).toBe(
      "verified at invalid-date",
    );
    expect(formatProviderValidationEvidenceLabel(true, "2026-07-09T04:00:00.000Z")).toContain(
      "verified at",
    );
    expect(
      formatGoogleDriveProductionOAuthEvidenceAction({
        status: "ready_to_start",
        production_oauth_candidate: true,
        oauth_redirect_environment: "production_https",
      }),
    ).toBe("Live Google API completion will record production OAuth evidence");
    expect(
      formatGoogleDriveProductionOAuthEvidenceAction({
        status: "ready_to_start",
        production_oauth_candidate: false,
        oauth_redirect_environment: "local_development",
      }),
    ).toBe(
      "Live completion will persist the session, but local/dev OAuth does not count as production validation",
    );
    expect(
      formatGoogleDriveProductionOAuthEvidenceAction({
        status: "blocked",
        production_oauth_candidate: false,
        oauth_redirect_environment: "unconfigured",
      }),
    ).toBe("Resolve setup blockers before OAuth evidence can be recorded");
  });

  it("formats provider remote sync enablement labels with downgrade reasons", () => {
    expect(formatProviderRemoteSyncEnablementLabel(null)).toBe("not checked");
    expect(
      formatProviderRemoteSyncEnablementLabel({
        provider: "google-drive",
        enabled: true,
        enablement_boundary: "explicit_manual_provider_session",
      }),
    ).toBe("enabled");
    expect(
      formatProviderRemoteSyncEnablementLabel({
        provider: "google-drive",
        enabled: false,
        enablement_boundary: "explicit_manual_provider_session",
        reason: "stored enablement is blocked by current provider readiness",
      }),
    ).toBe("disabled (stored enablement is blocked by current provider readiness)");
  });

  it("summarizes manual replay results with attempted and skipped counts", () => {
    expect(
      formatAdminReplayResultSummary({
        attempted_operation_count: 4,
        applied_operation_count: 2,
        skipped_operation_count: 1,
        failed_operation_count: 1,
        unsupported_operation_count: 0,
      }),
    ).toBe("4 attempted, 2 applied, 1 skipped, 1 failed, 0 unsupported");
    expect(
      formatAdminReplayResultSummary({
        attempted_operation_count: 6,
        applied_operation_count: 3,
        skipped_operation_count: 2,
        failed_operation_count: 0,
        unsupported_operation_count: 1,
      }),
    ).toBe("6 attempted, 3 applied, 2 skipped, 0 failed, 1 unsupported");
  });

  it("summarizes provider import apply readiness", () => {
    expect(formatProviderImportApplyPlanSummary(null)).toBe("not planned");
    expect(
      formatProviderImportApplyPlanSummary({
        selected_operation_count: 2,
        skipped_operation_count: 4,
      }),
    ).toBe("2 selected, 4 skipped");
    expect(getProviderImportApplyReviewState(null)).toEqual({
      canApply: false,
      message: "Run Test Pull to download and review a provider-sync manifest.",
    });
    expect(getProviderImportNextActionLabel(null)).toBe("Run Test Pull");
    expect(
      getProviderImportApplyReviewState({
        provider_sync_manifest: {},
        import_apply_plan: {
          selected_operation_count: 2,
          skipped_operation_count: 0,
        },
      }),
    ).toEqual({
      canApply: true,
      message: "2 remote operations ready to apply.",
    });
    expect(
      getProviderImportNextActionLabel({
        provider_sync_manifest: {},
        import_apply_plan: {
          selected_operation_count: 2,
        },
      }),
    ).toBe("Apply Last Import");
    expect(
      getProviderImportApplyReviewState(
        {
          provider_sync_manifest: {},
          import_apply_plan: {
            selected_operation_count: 2,
            skipped_operation_count: 0,
          },
        },
        { status: "completed" },
      ),
    ).toEqual({
      canApply: false,
      message: "Last provider import completed. Run Test Pull to review a new manifest.",
    });
    expect(
      getProviderImportNextActionLabel(
        {
          provider_sync_manifest: {},
          import_apply_plan: {
            selected_operation_count: 2,
          },
        },
        { status: "completed" },
      ),
    ).toBe("Run Test Pull");
    expect(
      getProviderImportApplyReviewState({
        provider_sync_manifest: {},
        import_apply_plan: {
          selected_operation_count: 0,
          skipped_operation_count: 4,
        },
        import_dry_run: {
          conflict_count: 2,
          failed_count: 1,
          unsupported_count: 2,
          operations: [
            {
              status: "conflict",
              message: "A local queued operation targets a related entity",
            },
            {
              status: "unsupported",
              message:
                "Remote replay operation comment:create is server-only and cannot be imported into a local vault",
            },
            {
              status: "unsupported",
              message:
                "Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow",
            },
          ],
        },
      }),
    ).toEqual({
      canApply: false,
      message:
        "No eligible remote operations: 1 related-entity conflict, 1 conflict, 1 failed, 1 server-only, 1 recovery-required, 4 skipped.",
    });
    expect(
      getProviderImportNextActionLabel({
        provider_sync_manifest: {},
        import_apply_plan: {
          selected_operation_count: 0,
        },
      }),
    ).toBe("Review blockers");
  });

  it("updates provider import review state after provider sync results", () => {
    const remoteAttempt = {
      schema_version: 1 as const,
      status: "applied" as const,
      provider: "google-drive" as const,
      direction: "pull" as const,
      mutation_id: "provider-attempt-1",
      transport_kind: "provider-api" as const,
      applied: true,
      attempted_at: "2026-07-10T12:00:00.000Z",
      blockers: [],
    };

    expect(
      getProviderImportReviewStateAfterSync("pull", {
        remote_mutation_attempt: remoteAttempt,
      }),
    ).toEqual({
      providerImportAttempt: remoteAttempt,
      providerImportApplyResult: null,
    });
    expect(
      getProviderImportReviewStateAfterSync("pull", {
        remote_mutation_attempt: undefined,
      }),
    ).toEqual({
      providerImportAttempt: null,
      providerImportApplyResult: null,
    });
    expect(
      getProviderImportReviewStateAfterSync("push", {
        remote_mutation_attempt: remoteAttempt,
      }),
    ).toEqual({});
    expect(formatProviderImportAttemptProvenance(remoteAttempt)).toBe(
      "Pull review / google drive / provider api / applied / 2026-07-10T12:00:00.000Z / provider-attempt-1",
    );
  });

  it("clears provider import review state after provider connection changes", () => {
    expect(getProviderImportReviewStateAfterProviderConnectionChange()).toEqual({
      providerImportAttempt: null,
      providerImportApplyResult: null,
    });
  });

  it("summarizes provider import apply results with blocked counts", () => {
    const results = [
      {
        operation_id: "op-1",
        entity_type: "document",
        entity_id: "doc-1",
        action: "create",
        status: "applied" as const,
        message: "Remote document imported",
      },
      {
        operation_id: "op-2",
        entity_type: "folder",
        entity_id: "folder-1",
        action: "create",
        status: "blocked" as const,
        message: "Local queued operation targets the same folder",
      },
    ];
    expect(getProviderImportApplyBlockedOperationCount(results)).toBe(1);
    expect(
      formatProviderImportApplyResultSummary({
        attempted_operation_count: 4,
        applied_operation_count: 2,
        skipped_operation_count: 1,
        failed_operation_count: 0,
        blocked_operation_count: 1,
        results,
      }),
    ).toBe("4 attempted, 2 applied, 1 skipped, 0 failed, 1 blocked");
    expect(
      formatProviderImportApplyResultSummary({
        attempted_operation_count: 2,
        applied_operation_count: 0,
        skipped_operation_count: 0,
        failed_operation_count: 0,
        results,
      }),
    ).toBe("2 attempted, 0 applied, 0 skipped, 0 failed, 1 blocked");
  });

  it("summarizes provider pull review outcomes with blocker categories", () => {
    expect(
      formatProviderImportPullReviewSummary({
        import_apply_plan: {
          selected_operation_count: 2,
          skipped_operation_count: 4,
        },
        import_dry_run: {
          conflict_count: 3,
          failed_count: 1,
          unsupported_count: 3,
          operations: [
            {
              status: "conflict",
              message: "A local queued operation targets a related entity",
            },
            {
              status: "conflict",
              message: "A local queued operation targets the same document",
            },
            {
              status: "unsupported",
              message:
                "Remote replay operation comment:create is server-only and cannot be imported into a local vault",
            },
            {
              status: "unsupported",
              message:
                "Remote replay operation space:delete would remove a whole local space and must be handled by an explicit vault recovery flow",
            },
            {
              status: "unsupported",
              message:
                "Remote replay operation document:publish is server-only and cannot be imported into a local vault",
            },
          ],
        },
      }),
    ).toBe(
      "2 selected, 4 skipped, 1 related-entity conflict, 1 document conflict, 1 conflict, 1 failed, 2 server-only, 1 recovery-required",
    );
    expect(
      formatProviderImportPullReviewSummary({
        import_apply_plan: {
          selected_operation_count: 0,
          skipped_operation_count: 3,
        },
        import_dry_run: {
          conflict_count: 0,
          failed_count: 2,
          unsupported_count: 1,
        },
      }),
    ).toBe("0 selected, 3 skipped, 2 failed, 1 unsupported");
  });

  it("uses shared inputs for system application config fields", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin User",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="system" />);
    expect(html).toContain("Application Configuration");
    expect(html).toContain('aria-label="New config key"');
    expect(html).toContain('placeholder="custom.key"');
    expect(html).toContain('aria-label="New config value"');
    expect(html).toContain('placeholder="Value"');
    expect(html).toContain("focus:ring-2");
  });

  it("uses the shared textarea control for admin JSON fields", () => {
    const html = renderToStaticMarkup(
      <AdminJsonTextarea
        value='{"id":"plugin"}'
        onChange={() => undefined}
        ariaLabel="Google Drive token response fixture"
        className="min-h-20"
        placeholder='{"access_token":"..."}'
      />,
    );

    expect(html).toContain('aria-label="Google Drive token response fixture"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("font-mono");
    expect(html).toContain("min-h-20");
    expect(html).toContain("access_token");
  });

  it("uses the shared select control for compact admin filters", () => {
    const html = renderToStaticMarkup(
      <AdminCompactSelect
        value="all"
        onChange={() => undefined}
        ariaLabel="Plugin runtime audit action filter"
      >
        <option value="all">All runtime events</option>
      </AdminCompactSelect>,
    );

    expect(html).toContain('aria-label="Plugin runtime audit action filter"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("text-xs");
    expect(html).toContain("All runtime events");
  });

  it("uses the shared select control for admin form filters", () => {
    const filterHtml = renderToStaticMarkup(
      <AdminFormSelect value="all" onChange={() => undefined} ariaLabel="User role filter">
        <option value="all">All roles</option>
      </AdminFormSelect>,
    );
    const themeHtml = renderToStaticMarkup(
      <AdminFormSelect
        value="light"
        onChange={() => undefined}
        ariaLabel="Static site export theme"
      >
        <option value="light">Light theme</option>
        <option value="dark">Dark theme</option>
        <option value="paper">Paper theme</option>
      </AdminFormSelect>,
    );

    expect(filterHtml).toContain('aria-label="User role filter"');
    expect(filterHtml).toContain("rounded-input");
    expect(filterHtml).toContain("focus:ring-2");
    expect(filterHtml).toContain("h-9");
    expect(filterHtml).toContain("All roles");
    expect(themeHtml).toContain('aria-label="Static site export theme"');
    expect(themeHtml).toContain("Dark theme");
    expect(themeHtml).toContain("Paper theme");
  });

  it("uses the shared input control for admin text fields", () => {
    const html = renderToStaticMarkup(
      <AdminTextInput
        value="admin@example.com"
        onChange={() => undefined}
        ariaLabel="Invite user email"
        placeholder="Email"
        type="email"
        required
        disabled
      />,
    );

    expect(html).toContain('aria-label="Invite user email"');
    expect(html).toContain('placeholder="Email"');
    expect(html).toContain('type="email"');
    expect(html).toContain("required");
    expect(html).toContain("disabled");
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("h-9");
  });

  it("uses the shared input control for locked secret admin settings", () => {
    const html = renderToStaticMarkup(
      <AdminTextInput
        value=""
        onChange={() => undefined}
        ariaLabel="SMTP password value"
        placeholder="Not set"
        type="password"
        disabled
      />,
    );

    expect(html).toContain('aria-label="SMTP password value"');
    expect(html).toContain('placeholder="Not set"');
    expect(html).toContain('type="password"');
    expect(html).toContain("disabled");
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
  });

  it("uses the shared select control for inline admin pickers", () => {
    const pickerHtml = renderToStaticMarkup(
      <AdminInlineSelect
        value=""
        onChange={() => undefined}
        ariaLabel="User to add"
        className="max-w-44"
      >
        <option value="">Select user</option>
      </AdminInlineSelect>,
    );
    const visibilityHtml = renderToStaticMarkup(
      <AdminInlineSelect
        value="private"
        onChange={() => undefined}
        ariaLabel="Space visibility"
      >
        <option value="private">Private</option>
        <option value="internal">Signed-in users</option>
        <option value="public">Public</option>
      </AdminInlineSelect>,
    );

    expect(pickerHtml).toContain('aria-label="User to add"');
    expect(pickerHtml).toContain("rounded-input");
    expect(pickerHtml).toContain("focus:ring-2");
    expect(pickerHtml).toContain("max-w-44");
    expect(pickerHtml).toContain("Select user");
    expect(visibilityHtml).toContain('aria-label="Space visibility"');
    expect(visibilityHtml).toContain("Signed-in users");
    expect(visibilityHtml).toContain("Public");
  });

  it("uses the shared select control for admin role pickers", () => {
    const permissionHtml = renderToStaticMarkup(
      <RoleSelect value="viewer" onChange={() => undefined} />,
    );
    const userRoleHtml = renderToStaticMarkup(
      <UserRoleSelect value="member" onChange={() => undefined} />,
    );

    expect(permissionHtml).toContain('aria-label="Permission role"');
    expect(permissionHtml).toContain("rounded-input");
    expect(permissionHtml).toContain("focus:ring-2");
    expect(permissionHtml).toContain("Read/write");
    expect(userRoleHtml).toContain('aria-label="User role"');
    expect(userRoleHtml).toContain("rounded-input");
    expect(userRoleHtml).toContain("focus:ring-2");
    expect(userRoleHtml).toContain("Admin");
  });

  it("validates Google Drive OAuth launch URLs", () => {
    const url = new URL("https://accounts.google.com/o/oauth2/v2/auth");
    url.searchParams.set("response_type", "code");
    url.searchParams.set("code_challenge_method", "S256");
    url.searchParams.set("client_id", "google-client-id");
    url.searchParams.set("redirect_uri", "https://notes.example.com/provider/google-drive/callback");
    url.searchParams.set("state", "oauth-state");
    url.searchParams.set("code_challenge", "pkce-code-challenge");
    url.searchParams.set("scope", "drive.file drive.metadata.readonly offline_access");
    url.searchParams.set("access_type", "offline");
    url.searchParams.set("prompt", "consent");

    expect(validateGoogleDriveOAuthLaunchUrl(url.toString())).toBe(url.toString());
    expect(() => validateGoogleDriveOAuthLaunchUrl(undefined)).toThrow(
      "Generate a Google Drive setup preview before opening OAuth.",
    );
    expect(() => validateGoogleDriveOAuthLaunchUrl("https://example.com/oauth")).toThrow(
      "Google Drive OAuth authorization URL must target Google Accounts.",
    );
    const missingClientId = new URL(url);
    missingClientId.searchParams.delete("client_id");
    expect(() => validateGoogleDriveOAuthLaunchUrl(missingClientId.toString())).toThrow(
      "Google Drive OAuth authorization URL must include a client ID.",
    );
    const missingRedirectUri = new URL(url);
    missingRedirectUri.searchParams.delete("redirect_uri");
    expect(() => validateGoogleDriveOAuthLaunchUrl(missingRedirectUri.toString())).toThrow(
      "Google Drive OAuth authorization URL must include a redirect URI.",
    );
    const missingState = new URL(url);
    missingState.searchParams.delete("state");
    expect(() => validateGoogleDriveOAuthLaunchUrl(missingState.toString())).toThrow(
      "Google Drive OAuth authorization URL must include a state value.",
    );
    const missingCodeChallenge = new URL(url);
    missingCodeChallenge.searchParams.delete("code_challenge");
    expect(() => validateGoogleDriveOAuthLaunchUrl(missingCodeChallenge.toString())).toThrow(
      "Google Drive OAuth authorization URL must include a PKCE code challenge.",
    );
    const missingDriveFileScope = new URL(url);
    missingDriveFileScope.searchParams.set("scope", "drive.metadata.readonly offline_access");
    expect(() => validateGoogleDriveOAuthLaunchUrl(missingDriveFileScope.toString())).toThrow(
      "Google Drive OAuth authorization URL must include drive.file scope.",
    );
    const missingMetadataScope = new URL(url);
    missingMetadataScope.searchParams.set("scope", "drive.file offline_access");
    expect(() => validateGoogleDriveOAuthLaunchUrl(missingMetadataScope.toString())).toThrow(
      "Google Drive OAuth authorization URL must include drive.metadata.readonly scope.",
    );
    const missingOfflineScope = new URL(url);
    missingOfflineScope.searchParams.set("scope", "drive.file drive.metadata.readonly");
    expect(() => validateGoogleDriveOAuthLaunchUrl(missingOfflineScope.toString())).toThrow(
      "Google Drive OAuth authorization URL must include offline_access scope.",
    );
    const missingOfflineAccess = new URL(url);
    missingOfflineAccess.searchParams.delete("access_type");
    expect(() => validateGoogleDriveOAuthLaunchUrl(missingOfflineAccess.toString())).toThrow(
      "Google Drive OAuth authorization URL must request offline access.",
    );
    const missingConsent = new URL(url);
    missingConsent.searchParams.delete("prompt");
    expect(() => validateGoogleDriveOAuthLaunchUrl(missingConsent.toString())).toThrow(
      "Google Drive OAuth authorization URL must request consent for refresh access.",
    );
  });

  it("parses Google Drive token response fixtures for manual setup", () => {
    expect(
      parseGoogleDriveTokenFixture(
        JSON.stringify({
          access_token: "access-token",
          refresh_token: "refresh-token",
          expires_in: 3600,
          scope: "drive.file drive.metadata.readonly offline_access",
          account_label: "eagle@example.com",
          provider_root_id: "drive-root-id",
          provider_root_name: "Starlane Notes",
        }),
      ),
    ).toEqual({
      access_token: "access-token",
      refresh_token: "refresh-token",
      expires_in: 3600,
      scope: "drive.file drive.metadata.readonly offline_access",
      issued_at: undefined,
      account_label: "eagle@example.com",
      provider_root_id: "drive-root-id",
      provider_root_name: "Starlane Notes",
    });
  });

  it("rejects incomplete Google Drive token response fixtures", () => {
    expect(() => parseGoogleDriveTokenFixture('{"expires_in":3600}')).toThrow(
      "Google Drive token response requires access_token",
    );
    expect(() => parseGoogleDriveTokenFixture('{"access_token":"token","expires_in":0}')).toThrow(
      "Google Drive token response requires positive expires_in",
    );
  });

  it("shows queued replay controls in local storage", () => {
    mocks.launch.mode = "local";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="storage" />);
    expect(html).toContain("Static Site Settings");
    expect(html).toContain("Light theme");
    expect(html).toContain("Paper theme");
    expect(html).toContain("Queued Sync Replay");
    expect(html).toContain("Native app writes land locally first");
    expect(html).toContain("selected recovery");
    expect(html).toContain("Export Replay");
    expect(html).toContain("Check Server");
    expect(html).toContain("Run Replay");
    expect(html).toContain("Recent Replay History");
    expect(html).toContain("Refresh History");
    expect(html).toContain("Export Static HTML");
    expect(html).toContain("Export Static ZIP");
    expect(html).toContain("Clear Queue");
  });

  it("shows shared-space download controls in server storage", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin User",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="storage" />);
    expect(html).toContain("Shared Space Downloads");
    expect(html).toContain("Download selected server spaces");
    expect(html).toContain("HTML");
    expect(html).toContain("ZIP");
  });

  it("renders plugin manifest validation controls", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin User",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="plugins" />);
    expect(html).toContain("Plugins");
    expect(html).toContain("plugin.json");
    expect(html).toContain("registry.json");
    expect(html).toContain("Validate");
    expect(html).toContain("Contract only");
    expect(html).toContain("No registry index validated yet.");
    expect(html).toContain("Installed Plugins");
    expect(html).toContain("Registry index validation");
    expect(html).toContain("Admin install records");
    expect(html).toContain("Install Audit");
    expect(html).toContain("Local install audit");
    expect(html).toContain("Deno server sandbox");
  });

  it("falls back from search reindex in local mode", () => {
    mocks.launch.mode = "local";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="search" />);
    expect(html).not.toContain("Search indexing is local-only in this mode");
    expect(html).not.toContain("Reindex");
    expect(html).toContain("Vault Contents");
  });

  it("renders webhook management instead of the placeholder", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="webhooks" />);
    expect(html).toContain("Manage outbound event deliveries");
    expect(html).toContain('aria-label="Webhook name"');
    expect(html).toContain('placeholder="Release automation"');
    expect(html).toContain('aria-label="Webhook URL"');
    expect(html).toContain('placeholder="https://example.com/starlane/webhook"');
    expect(html).toContain('aria-label="Webhook events"');
    expect(html).toContain('aria-label="Webhook secret"');
    expect(html).toContain('type="password"');
    expect(html).toContain('aria-label="Webhook space ID"');
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("Delivery History");
    expect(html).toContain("No webhooks configured.");
    expect(html).not.toContain("Webhooks settings");
  });

  it("renders webhook redelivery action evidence and pending lockout", () => {
    const deliveries = [
      {
        id: "delivery-1",
        event_type: "document.updated",
        response_status: 500,
        success: false,
        attempt_number: 2,
        delivered_at: "2026-07-14T12:00:00.000Z",
      },
      {
        id: "delivery-2",
        event_type: "comment.created",
        response_status: 202,
        success: true,
        attempt_number: 1,
        delivered_at: "2026-07-14T12:05:00.000Z",
      },
    ];

    const html = renderToStaticMarkup(
      <AdminWebhookDeliveryHistoryTable
        deliveries={deliveries}
        localMode={false}
        redeliveringDeliveryId={null}
        redeliveryError={null}
        onRedeliver={() => undefined}
      />,
    );
    expect(html).toContain("document.updated");
    expect(html).toContain("Redeliver");
    expect(html).not.toContain("Queueing...");
    expect(html).not.toContain('disabled=""');

    const pendingHtml = renderToStaticMarkup(
      <AdminWebhookDeliveryHistoryTable
        deliveries={deliveries}
        localMode={false}
        redeliveringDeliveryId="delivery-1"
        redeliveryError="Unable to queue redelivery"
        onRedeliver={() => undefined}
      />,
    );
    expect(pendingHtml).toContain("Queueing...");
    expect(pendingHtml).toContain("Unable to queue redelivery");
    expect(pendingHtml).toContain("border-danger/30");
    expect(pendingHtml.match(/disabled=""/g)?.length).toBe(2);
  });

  it("clears stale webhook delivery and redelivery state while switching selections", () => {
    expect(getAdminWebhookDeliveryResetState()).toEqual({
      deliveries: [],
      deliveriesError: null,
      redeliveryError: null,
      redeliveringDeliveryId: null,
      deliveriesLoading: false,
    });
  });

  it("falls back from webhook management in local mode", () => {
    mocks.launch.mode = "local";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="webhooks" />);
    expect(html).not.toContain("Webhook delivery is a server-mode feature");
    expect(html).not.toContain("Delivery History");
    expect(html).toContain("Vault Contents");
  });

  it("hides server-only sections from local Admin navigation", () => {
    expect(isAdminSectionAvailable("ai", true)).toBe(false);
    expect(isAdminSectionAvailable("ai", false)).toBe(true);
    expect(isAdminSectionAvailable("api-keys", true)).toBe(false);
    expect(isAdminSectionAvailable("api-keys", false)).toBe(true);
    expect(isAdminSectionAvailable("audit", true)).toBe(false);
    expect(isAdminSectionAvailable("audit", false)).toBe(true);
    expect(isAdminSectionAvailable("auth", true)).toBe(false);
    expect(isAdminSectionAvailable("auth", false)).toBe(true);
    expect(isAdminSectionAvailable("groups", true)).toBe(false);
    expect(isAdminSectionAvailable("groups", false)).toBe(true);
    expect(isAdminSectionAvailable("notifications", true)).toBe(false);
    expect(isAdminSectionAvailable("notifications", false)).toBe(true);
    expect(isAdminSectionAvailable("search", true)).toBe(false);
    expect(isAdminSectionAvailable("search", false)).toBe(true);
    expect(isAdminSectionAvailable("users", true)).toBe(false);
    expect(isAdminSectionAvailable("users", false)).toBe(true);
    expect(isAdminSectionAvailable("webhooks", true)).toBe(false);
    expect(isAdminSectionAvailable("webhooks", false)).toBe(true);

    mocks.launch.mode = "local";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="ai" />);
    expect(html).not.toContain("AI Settings");
    expect(html).not.toContain("Groups");
    expect(html).toContain("Storage");
    expect(html).toContain("Vault Contents");

    const groupsHtml = renderToStaticMarkup(<AdminPage initialSection="groups" />);
    expect(groupsHtml).not.toContain("Groups are unavailable in local mode.");
    expect(groupsHtml).toContain("Vault Contents");

    const searchHtml = renderToStaticMarkup(<AdminPage initialSection="search" />);
    expect(searchHtml).not.toContain("Search indexing is local-only in this mode");
    expect(searchHtml).toContain("Vault Contents");

    const notificationsHtml = renderToStaticMarkup(<AdminPage initialSection="notifications" />);
    expect(notificationsHtml).not.toContain(
      "Server email delivery is unavailable in local standalone mode",
    );
    expect(notificationsHtml).toContain("Vault Contents");

    const webhooksHtml = renderToStaticMarkup(<AdminPage initialSection="webhooks" />);
    expect(webhooksHtml).not.toContain("Webhook delivery is a server-mode feature");
    expect(webhooksHtml).toContain("Vault Contents");

    const auditHtml = renderToStaticMarkup(<AdminPage initialSection="audit" />);
    expect(auditHtml).not.toContain("Audit Log");
    expect(auditHtml).not.toContain("Export CSV");
    expect(auditHtml).toContain("Vault Contents");

    const authHtml = renderToStaticMarkup(<AdminPage initialSection="auth" />);
    expect(authHtml).not.toContain("Server authentication settings are unavailable");
    expect(authHtml).toContain("Vault Contents");

    const apiKeysHtml = renderToStaticMarkup(<AdminPage initialSection="api-keys" />);
    expect(apiKeysHtml).not.toContain("Server API keys are unavailable");
    expect(apiKeysHtml).toContain("Vault Contents");

    const usersHtml = renderToStaticMarkup(<AdminPage initialSection="users" />);
    expect(usersHtml).not.toContain("Search users");
    expect(usersHtml).not.toContain("Invite user");
    expect(usersHtml).toContain("Vault Contents");

    const defaultHtml = renderToStaticMarkup(<AdminPage />);
    expect(defaultHtml).not.toContain("Search users");
    expect(defaultHtml).toContain("Vault Contents");
  });

  it("renders notification SMTP management instead of the placeholder", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="notifications" />);
    expect(html).toContain("Configure SMTP delivery");
    expect(html).toContain("Email Delivery");
    expect(html).not.toContain("Notifications settings");
  });

  it("falls back from notification SMTP management in local mode", () => {
    mocks.launch.mode = "local";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="notifications" />);
    expect(html).not.toContain("Server email delivery is unavailable in local standalone mode");
    expect(html).not.toContain("Configure SMTP delivery");
    expect(html).not.toContain("Email Delivery");
    expect(html).toContain("Vault Contents");
  });

  it("renders authentication management instead of the placeholder", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="auth" />);
    expect(html).toContain("Configure server authentication requirements");
    expect(html).toContain("Login Policy");
    expect(html).toContain("Loading authentication settings...");
    expect(html).not.toContain("Authentication settings");
  });

  it("keeps site administration layout usable on phone-width web screens", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="auth" />);
    expect(html).toContain("min-w-0 flex-col md:flex-row");
    expect(html).toContain("Site Administration");
    expect(html).toContain(ADMIN_MOBILE_HOME_CLASS);
    expect(html).toContain("hidden md:block min-w-0 flex-1 overflow-y-auto p-3 md:p-6");
  });

  it("falls back from authentication management in local mode", () => {
    mocks.launch.mode = "local";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="auth" />);
    expect(html).not.toContain("Server authentication settings are unavailable");
    expect(html).not.toContain("Login Policy");
    expect(html).toContain("Vault Contents");
  });

  it("renders API key management instead of the placeholder", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="api-keys" />);
    expect(html).toContain("Create and revoke personal API keys");
    expect(html).toContain("New API Key");
    expect(html).toContain('aria-label="API key name"');
    expect(html).toContain('placeholder="Key name"');
    expect(html).toContain('aria-label="Expiration date"');
    expect(html).toContain('type="datetime-local"');
    expect(html).toContain("Scopes");
    expect(html).toContain("Read");
    expect(html).toContain("Write");
    expect(html).toContain("Admin");
    expect(html).toContain("Active Keys");
    expect(html).toContain("New Service Account Key");
    expect(html).toContain('aria-label="Service account key name"');
    expect(html).toContain('aria-label="Service account expiration date"');
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("Service Account Keys");
    expect(html).toContain("No service account keys created.");
    expect(html).toContain("No API keys created.");
    expect(html).not.toContain("API Keys settings");
  });

  it("falls back from API key management in local mode", () => {
    mocks.launch.mode = "local";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="api-keys" />);
    expect(html).not.toContain("Server API keys are unavailable");
    expect(html).not.toContain("New API Key");
    expect(html).toContain("Vault Contents");
  });

  it("renders admin user lifecycle filters", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin User",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="users" />);
    expect(html).toContain("Search users");
    expect(html).toContain('aria-label="Search users"');
    expect(html).toContain("All roles");
    expect(html).toContain("All statuses");
    expect(html).toContain("Activate selected");
    expect(html).toContain("Deactivate selected");
    expect(html).toContain("Loading users...");
  });

  it("clears stale admin user lifecycle state after list load failures", () => {
    const state = getAdminUserLifecycleFailureState<
      { id: string; email: string },
      { id: string }
    >();

    expect(state.users).toEqual([]);
    expect(state.sessions).toEqual([]);
    expect(state.sessionUser).toBeNull();
    expect(Array.from(state.selectedUserIds)).toEqual([]);
  });

  it("clears stale admin user session rows when the session panel closes", () => {
    const state = getAdminUserSessionPanelClosedState<
      { id: string; email: string },
      { id: string }
    >();

    expect(state.sessionUser).toBeNull();
    expect(state.sessions).toEqual([]);
    expect(state.sessionsLoading).toBe(false);
  });

  it("uses explicit admin session revoke confirmation copy", () => {
    expect(getAdminSessionDeviceLabel({ device_name: "Harley's Mac", user_agent: "Safari" })).toBe(
      "Harley's Mac",
    );
    expect(getAdminSessionDeviceLabel({ user_agent: "Firefox" })).toBe("Firefox");
    expect(getAdminSessionDeviceLabel({})).toBe("Unknown device");
    expect(
      getAdminSessionRevokeConfirmationMessage("eagle@example.test", "Harley's Mac"),
    ).toBe(
      'Revoke the session for eagle@example.test on "Harley\'s Mac"? That device will need to sign in again.',
    );
  });

  it("uses explicit admin storage reset confirmation copy", () => {
    expect(getAdminStorageResetConfirmationMessage()).toBe(
      "Reset storage setup and return to the vault chooser?",
    );
  });

  it("uses explicit admin vault import confirmation copy", () => {
    expect(getAdminVaultArchiveImportConfirmationMessage()).toBe(
      "Importing this vault will replace the current local vault.",
    );
    expect(
      getAdminVaultSnapshotImportConfirmationMessage({
        schemaVersion: 1,
        spaceCount: 1,
        folderCount: 2,
        documentCount: 3,
        assetCount: 4,
        templateCount: 5,
        queuedOperationCount: 6,
        syncState: "idle",
      }),
    ).toBe(
      "Import snapshot contains 1 space, 3 documents, 2 folders, 4 assets, 5 templates. Sync state: idle; 6 queued operations.\n\nImporting this vault will replace the current local vault.",
    );
  });

  it("uses explicit admin provider space recovery confirmation copy", () => {
    expect(getAdminProviderSpaceRecoveryConfirmationMessage(1, "space-1")).toBe(
      "Recover provider deletion for local space space-1? This permanently removes the selected local space data.",
    );
    expect(getAdminProviderSpaceRecoveryConfirmationMessage(2, "space-1, space-2")).toBe(
      "Recover provider deletion for local spaces space-1, space-2? This permanently removes the selected local space data.",
    );
  });

  it("uses explicit admin queued-sync clear confirmation copy", () => {
    expect(getAdminQueuedSyncClearConfirmationMessage()).toBe(
      "Clear all queued local sync operations?",
    );
    expect(getAdminQueuedSyncClearConfirmationMessage(1)).toBe(
      "Clear 1 selected queued replay operation?",
    );
    expect(getAdminQueuedSyncClearConfirmationMessage(3)).toBe(
      "Clear 3 selected queued replay operations?",
    );
  });

  it("uses explicit admin queued-sync replay confirmation copy", () => {
    expect(getAdminQueuedSyncReplayConfirmationMessage()).toBe(
      "Replay supported queued local operations to the linked server? Unsupported or failed operations will remain queued.",
    );
    expect(getAdminQueuedSyncReplayConfirmationMessage(1)).toBe(
      "Retry 1 failed or unsupported queued operation against the linked server?",
    );
    expect(getAdminQueuedSyncReplayConfirmationMessage(4)).toBe(
      "Retry 4 failed or unsupported queued operations against the linked server?",
    );
  });

  it("clears viewed admin user sessions and loading state after password reset", () => {
    const viewedUser = { id: "user-1", email: "one@example.com" };
    const otherUser = { id: "user-2", email: "two@example.com" };
    const sessions = [{ id: "session-1" }];

    expect(
      getAdminUserPasswordResetSessionState(viewedUser, viewedUser, sessions, true),
    ).toEqual({
      sessions: [],
      sessionsLoading: false,
    });

    expect(
      getAdminUserPasswordResetSessionState(viewedUser, otherUser, sessions, true),
    ).toEqual({
      sessions,
      sessionsLoading: true,
    });
  });

  it("builds admin bulk lifecycle request payloads with selected users and action", () => {
    const request = buildAdminUserBulkLifecycleRequest(["user-1", "user-2"], "deactivate");

    expect(request).toEqual({
      user_ids: ["user-1", "user-2"],
      action: "deactivate",
    });
    expect(Object.keys(request)).toEqual(["user_ids", "action"]);
  });

  it("formats admin bulk lifecycle result messages with skipped reasons", () => {
    expect(
      formatAdminUserBulkLifecycleMessage({
        action: "activate",
        updated_count: 1,
        skipped_count: 0,
        skipped: [],
      }),
    ).toBe("1 user activated.");

    expect(
      formatAdminUserBulkLifecycleMessage({
        action: "deactivate",
        updated_count: 2,
        skipped_count: 2,
        skipped: [{ reason: "last admin" }, { reason: "already inactive" }],
      }),
    ).toBe("2 users deactivated. 2 skipped: last admin, already inactive.");
  });

  it("summarizes admin user lifecycle counts", () => {
    expect(
      getAdminUserLifecycleCounts([
        { is_active: true, role: "admin" },
        { is_active: false, role: "member" },
        { is_active: true, role: "member" },
      ]),
    ).toEqual({
      total: 3,
      active: 2,
      inactive: 1,
      admins: 1,
      members: 2,
    });
  });

  it("filters admin user lifecycle rows by query role and status", () => {
    const users = [
      {
        id: "user-1",
        email: "admin@example.com",
        display_name: "Primary Admin",
        role: "admin",
        is_active: true,
      },
      {
        id: "user-2",
        email: "ops-member@example.com",
        display_name: "Ops Member",
        role: "member",
        is_active: false,
      },
      {
        id: "user-3",
        email: "writer@example.com",
        display_name: "Writer",
        role: "member",
        is_active: true,
      },
    ];

    expect(
      filterAdminUsers(users, { query: "ops", role: "all", status: "all" }).map(
        (item) => item.id,
      ),
    ).toEqual(["user-2"]);
    expect(
      filterAdminUsers(users, { query: "", role: "admin", status: "active" }).map(
        (item) => item.id,
      ),
    ).toEqual(["user-1"]);
    expect(
      filterAdminUsers(users, { query: "member", role: "member", status: "inactive" }).map(
        (item) => item.id,
      ),
    ).toEqual(["user-2"]);
  });

  it("summarizes visible admin user lifecycle selection state", () => {
    const visibleUsers = [{ id: "user-1" }, { id: "user-2" }];

    expect(
      getAdminUserVisibleSelectionState(
        visibleUsers,
        new Set(["user-1", "hidden-user"]),
      ),
    ).toEqual({
      selectedVisibleUserIds: ["user-1"],
      allVisibleUsersSelected: false,
    });
    expect(
      getAdminUserVisibleSelectionState(visibleUsers, new Set(["user-1", "user-2"])),
    ).toEqual({
      selectedVisibleUserIds: ["user-1", "user-2"],
      allVisibleUsersSelected: true,
    });
    expect(getAdminUserVisibleSelectionState([], new Set(["user-1"]))).toEqual({
      selectedVisibleUserIds: [],
      allVisibleUsersSelected: false,
    });
  });

  it("toggles visible admin user lifecycle selection while preserving hidden rows", () => {
    const visibleUsers = [{ id: "user-1" }, { id: "user-2" }];

    expect(
      Array.from(
        getAdminUserSelectionAfterVisibleToggle(
          visibleUsers,
          new Set(["hidden-user"]),
          true,
        ),
      ).sort(),
    ).toEqual(["hidden-user", "user-1", "user-2"]);

    expect(
      Array.from(
        getAdminUserSelectionAfterVisibleToggle(
          visibleUsers,
          new Set(["hidden-user", "user-1", "user-2"]),
          false,
        ),
      ),
    ).toEqual(["hidden-user"]);
  });

  it("uses semantic status tokens for admin user lifecycle badges", () => {
    expect(getAdminUserStatusBadgeClass(true)).toBe("bg-success/10 text-success");
    expect(getAdminUserStatusBadgeClass(false)).toBe("bg-danger/10 text-danger");
    expect(getAdminUserStatusBadgeClass(true)).not.toMatch(/green-|red-/);
    expect(getAdminUserStatusBadgeClass(false)).not.toMatch(/green-|red-/);
    expect(getAdminWebhookDeliveryStatusClass(true)).toBe("text-success");
    expect(getAdminWebhookDeliveryStatusClass(false)).toBe("text-danger");
    expect(getAdminWebhookDeliveryStatusClass(true)).not.toMatch(/green-|red-/);
    expect(getAdminWebhookDeliveryStatusClass(false)).not.toMatch(/green-|red-/);
    expect(getAdminProviderReadinessBadgeClass(true)).toBe("bg-success/10 text-success");
    expect(getAdminProviderReadinessBadgeClass(false)).toBe("bg-warning/10 text-warning");
    expect(getAdminProviderReadinessBadgeClass(undefined)).toBe("bg-warning/10 text-warning");
    expect(getAdminProviderReadinessBadgeClass(true)).not.toMatch(/green-|amber-/);
    expect(getAdminProviderReadinessBadgeClass(false)).not.toMatch(/green-|amber-/);
    expect(ADMIN_PROVIDER_BLOCKER_TEXT_CLASS).toBe("text-xs text-warning");
    expect(ADMIN_PROVIDER_BLOCKER_TEXT_CLASS).not.toMatch(/amber-/);
    expect(ADMIN_PROVIDER_SUCCESS_TEXT_CLASS).toBe("text-xs text-success");
    expect(ADMIN_PROVIDER_SUCCESS_TEXT_CLASS).not.toMatch(/green-/);
    expect(getPluginPermissionReviewClass("high")).toContain("border-warning");
    expect(getPluginPermissionReviewClass("medium")).toContain("border-accent");
    expect(getPluginPermissionReviewClass("low")).toContain("border-success");
    expect(getPluginPermissionReviewClass("high")).not.toMatch(/amber-|sky-|emerald-/);
    expect(getPluginPermissionReviewClass("medium")).not.toMatch(/amber-|sky-|emerald-/);
    expect(getPluginPermissionReviewClass("low")).not.toMatch(/amber-|sky-|emerald-/);
  });

  it("uses semantic status tokens for shared admin message and icon actions", () => {
    expect(ADMIN_ERROR_CARD_CLASS).toContain("border-danger");
    expect(ADMIN_ERROR_CARD_CLASS).toContain("text-danger");
    expect(ADMIN_ERROR_CARD_CLASS).not.toMatch(/red-/);
    expect(ADMIN_COMPACT_ERROR_CLASS).toContain("border-danger");
    expect(ADMIN_COMPACT_ERROR_CLASS).toContain("bg-danger");
    expect(ADMIN_COMPACT_ERROR_CLASS).toContain("text-danger");
    expect(ADMIN_COMPACT_ERROR_CLASS).not.toMatch(/red-/);
    expect(ADMIN_SUCCESS_CARD_CLASS).toContain("border-success");
    expect(ADMIN_SUCCESS_CARD_CLASS).toContain("text-success");
    expect(ADMIN_SUCCESS_CARD_CLASS).not.toMatch(/green-/);
    expect(ADMIN_SECRET_SUCCESS_CARD_CLASS).toContain("border-success");
    expect(ADMIN_SECRET_SUCCESS_CARD_CLASS).not.toMatch(/green-/);
    expect(ADMIN_SECRET_SUCCESS_TITLE_CLASS).toContain("text-success");
    expect(ADMIN_SECRET_SUCCESS_TITLE_CLASS).not.toMatch(/green-/);
    expect(ADMIN_PLUGIN_SUCCESS_CARD_CLASS).toContain("border-success");
    expect(ADMIN_PLUGIN_SUCCESS_CARD_CLASS).toContain("text-success");
    expect(ADMIN_PLUGIN_SUCCESS_CARD_CLASS).not.toMatch(/emerald-/);
    expect(ADMIN_PLUGIN_ERROR_CARD_CLASS).toContain("border-danger");
    expect(ADMIN_PLUGIN_ERROR_CARD_CLASS).toContain("text-danger");
    expect(ADMIN_PLUGIN_ERROR_CARD_CLASS).not.toMatch(/red-/);
    expect(ADMIN_LOCAL_SYNC_ERROR_CARD_CLASS).toContain("border-danger");
    expect(ADMIN_LOCAL_SYNC_ERROR_CARD_CLASS).toContain("text-danger");
    expect(ADMIN_LOCAL_SYNC_ERROR_CARD_CLASS).not.toMatch(/red-/);
    expect(ADMIN_DANGER_ICON_BUTTON_CLASS).toContain("hover:text-danger");
    expect(ADMIN_DANGER_ICON_BUTTON_CLASS).not.toMatch(/red-/);
  });

  it("shows local space create and rename controls in local mode", () => {
    mocks.launch.mode = "local";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="spaces" />);
    expect(html).toContain("Local space settings");
    expect(ADMIN_ACTIVE_SPACE_NAME_LABEL).toBe("Active space name");
    expect(ADMIN_RENAME_ACTIVE_SPACE_LABEL).toBe("Rename Active Space");
    expect(html).toContain("Create Space");
    expect(html).toContain('aria-label="New space name"');
    expect(html).toContain("New space name");
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).not.toContain("Select user");
    expect(html).not.toContain("Select group");
  });

  it("sorts Admin local spaces alphabetically without mutating the source list", () => {
    const spaces = [
      { id: "space-z", name: "Zeta" },
      { id: "space-a", name: "alpha" },
      { id: "space-m", name: "Middle" },
    ];

    expect(sortAdminLocalSpaces(spaces).map((space) => space.id)).toEqual([
      "space-a",
      "space-m",
      "space-z",
    ]);
    expect(spaces.map((space) => space.id)).toEqual(["space-z", "space-a", "space-m"]);
  });

  it("keeps Admin local spaces sorted after renaming an existing space", () => {
    const spaces = [
      { id: "space-a", name: "Alpha" },
      { id: "space-b", name: "Beta" },
      { id: "space-c", name: "Gamma" },
    ];
    const renamed = spaces.map((space) =>
      space.id === "space-c" ? { ...space, name: "Archive" } : space,
    );

    expect(sortAdminLocalSpaces(renamed).map((space) => space.id)).toEqual([
      "space-a",
      "space-c",
      "space-b",
    ]);
  });

  it("uses shared inputs for admin group creation fields", () => {
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin User",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="groups" />);
    expect(html).toContain("Create group");
    expect(html).toContain('aria-label="Group name"');
    expect(html).toContain('placeholder="Group name"');
    expect(html).toContain('aria-label="Group description"');
    expect(html).toContain('placeholder="Description"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
  });

  it("shows storage controls in local mode", () => {
    mocks.launch.mode = "local";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="storage" />);
    expect(html).toContain("Vault Folder");
    expect(html).toContain("Vault Switcher");
    expect(html).toContain("Vault Contents");
    expect(html).toContain("Export JSON");
    expect(html).toContain("Import JSON");
    expect(html).toContain("Local standalone");
    expect(html).toContain("Native vault folders are available in the desktop app");
    expect(html).toContain("Vault folder switching is available in the desktop app");
    expect(html).toContain("Sync Server Vault");
    expect(html).toContain('aria-label="Sync server URL"');
    expect(html).toContain('placeholder="https://notes.example.com"');
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("Add Server Vault");
    expect(html).not.toContain("Linked Server");
  });

  it("shows linked server controls in server mode", () => {
    mocks.launch.mode = "server";
    mocks.runtime.target = "tauri-desktop";
    mocks.auth.user = {
      id: "admin-user",
      role: "admin",
      display_name: "Admin User",
      email: "admin@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="storage" />);
    expect(html).toContain("Linked Server");
    expect(html).toContain("Switch the server-backed app shell");
    expect(html).toContain('aria-label="Linked server URL"');
    expect(html).toContain('placeholder="https://notes.example.com"');
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("Save Server");
    expect(html).toContain("Reset Setup");
    expect(html).toContain("Server-backed");
  });

  it("shows desktop vault switcher controls in local desktop mode", () => {
    mocks.launch.mode = "local";
    mocks.runtime.target = "tauri-desktop";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="storage" />);
    expect(html).toContain("Vault Switcher");
    expect(html).toContain('id="vault-path"');
    expect(html).toContain('aria-label="Vault folder path"');
    expect(html).toContain("font-mono");
    expect(html).toContain("Active vault folder name");
    expect(html).toContain(
      "Renames the active vault folder on this device when the native filesystem allows it.",
    );
    expect(html).toContain('id="active-vault-name"');
    expect(html).toContain('aria-label="Active vault folder name"');
    expect(html).toContain('placeholder="Active vault folder name"');
    expect(html).toContain("Rename Vault Folder");
    expect(html).toContain("No sibling vault folders found");
    expect(html).toContain('aria-label="New vault name"');
    expect(html).toContain('placeholder="New vault name"');
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("Create Vault");
    expect(html).toContain("Export Vault");
    expect(html).not.toContain("Open Folder");
    expect(canRevealAdminVaultFolder(true, null)).toBe(false);
    expect(canRevealAdminVaultFolder(true, { path: "" })).toBe(false);
    expect(canRevealAdminVaultFolder(false, { path: "/vaults/notes" })).toBe(false);
    expect(canRevealAdminVaultFolder(true, { path: "/vaults/notes" })).toBe(true);
  });

  it("shows mobile-safe storage copy instead of desktop vault folders", () => {
    mocks.launch.mode = "local";
    mocks.runtime.target = "tauri-mobile";
    mocks.auth.user = {
      id: "local-user",
      role: "member",
      display_name: "Local User",
      email: "local@example.com",
    };

    const html = renderToStaticMarkup(<AdminPage initialSection="storage" />);
    expect(html).toContain("Mobile standalone notes use the app sandbox");
    expect(html).toContain("Export JSON");
    expect(html).toContain("Import JSON");
    expect(html).not.toContain("Choose Folder");
    expect(html).not.toContain("Rename Vault Folder");
    expect(html).not.toContain("Export Vault");
  });

  it("parses admin user CSV with headers and quoted display names", () => {
    expect(
      parseAdminUserCsv('email,display_name,role\nnew@example.com,"New, User",admin'),
    ).toEqual([
      {
        email: "new@example.com",
        display_name: "New, User",
        role: "admin",
      },
    ]);
  });

  it("parses admin user CSV without headers using member role defaults", () => {
    expect(parseAdminUserCsv("member@example.com,Member User")).toEqual([
      {
        email: "member@example.com",
        display_name: "Member User",
        role: "member",
      },
    ]);
  });

  it("rejects admin user CSV rows with invalid roles", () => {
    expect(() => parseAdminUserCsv("email,display_name,role\nbad@example.com,Bad,owner")).toThrow(
      "invalid role",
    );
  });

  it("parses comma-separated webhook events", () => {
    expect(parseWebhookEvents("document.created, workflow.transitioned, , comment.created")).toEqual(
      ["document.created", "workflow.transitioned", "comment.created"],
    );
  });
});
