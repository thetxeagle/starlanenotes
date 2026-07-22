import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  BACKGROUND_REPLAY_MAX_BACKOFF_MS,
  BACKGROUND_REPLAY_MIN_BACKOFF_MS,
  buildNativeBackgroundReplayScheduleArgs,
  buildNativeBackgroundReplayWakeArgs,
  buildNativeBackgroundReplayWakeReceipt,
  computeBackgroundReplayBackoffMs,
  defaultBackgroundReplayState,
  formatBackgroundReplayPartialResult,
  isOsDeliveredBackgroundWakeReceipt,
  recordBackgroundReplayFailure,
  recordProviderDeviceBackgroundWakeValidation,
  recordBackgroundReplaySuccess,
  shouldRunBackgroundReplay,
  shouldRunBackgroundReplayOnLifecycleWake,
  shouldRunBackgroundProviderPush,
} from "./backgroundReplay";
import type { ProviderStatus } from "./providerAdapters";
import { readProviderDefaultSyncValidationEvidence } from "./providerSession";

describe("backgroundReplay", () => {
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

  it("computes bounded exponential replay backoff", () => {
    expect(computeBackgroundReplayBackoffMs(1)).toBe(BACKGROUND_REPLAY_MIN_BACKOFF_MS);
    expect(computeBackgroundReplayBackoffMs(2)).toBe(BACKGROUND_REPLAY_MIN_BACKOFF_MS * 2);
    expect(computeBackgroundReplayBackoffMs(99)).toBe(BACKGROUND_REPLAY_MAX_BACKOFF_MS);
  });

  it("summarizes partial background replay outcomes with skipped operations", () => {
    expect(
      formatBackgroundReplayPartialResult({
        attempted_operation_count: 5,
        applied_operation_count: 2,
        skipped_operation_count: 1,
        failed_operation_count: 1,
        unsupported_operation_count: 1,
      }),
    ).toBe("5 attempted, 2 applied, 1 skipped, 1 failed, 1 unsupported");
  });

  it("builds native schedule registration args from provider and replay interval", () => {
    expect(buildNativeBackgroundReplayScheduleArgs("google-drive", "http://127.0.0.1/api/v1"))
      .toEqual({
        provider: "google-drive",
        linkedServerUrl: "http://127.0.0.1/api/v1",
        minIntervalSeconds: 60,
      });
  });

  it("builds native wake execution args from provider and replay interval", () => {
    expect(buildNativeBackgroundReplayWakeArgs("icloud-drive", "apple-bg-task")).toEqual({
      provider: "icloud-drive",
      wakeReason: "apple-bg-task",
      minIntervalSeconds: 60,
    });
  });

  it("builds native OS wake receipt payloads for Tauri import", () => {
    const receipt = buildNativeBackgroundReplayWakeReceipt(
      "google-drive",
      "android-workmanager",
      1_783_569_600_000,
      {
        osTaskIdentifier: "dev.starlane.notes.background-replay.google-drive",
        osSchedulerKind: "android-workmanager",
        source: "shared-preferences",
      },
    );

    expect(receipt).toEqual({
      provider: "google-drive",
      wake_reason: "android-workmanager",
      wake_epoch_millis: 1_783_569_600_000,
      os_task_identifier: "dev.starlane.notes.background-replay.google-drive",
      os_scheduler_kind: "android-workmanager",
      expired: undefined,
      source: "shared-preferences",
    });
    expect(isOsDeliveredBackgroundWakeReceipt(receipt)).toBe(true);
    expect(
      isOsDeliveredBackgroundWakeReceipt(
        buildNativeBackgroundReplayWakeReceipt(
          "icloud-drive",
          "apple-bg-task",
          1_783_569_600_000,
          {
            osSchedulerKind: "apple-bg-task-scheduler",
            source: "bg-task",
          },
        ),
      ),
    ).toBe(true);
  });

  it("records device background wake validation from imported native receipts", () => {
    recordProviderDeviceBackgroundWakeValidation(
      buildNativeBackgroundReplayWakeReceipt(
        "google-drive",
        "android-workmanager",
        1_783_569_600_000,
        {
          osSchedulerKind: "android-workmanager",
          source: "shared-preferences",
        },
      ),
    );

    expect(readProviderDefaultSyncValidationEvidence("google-drive")).toMatchObject({
      provider: "google-drive",
      production_oauth_verified: false,
      device_background_wake_verified: true,
      device_background_wake_verified_at: "2026-07-09T04:00:00.000Z",
      notes: [
        "google-drive native background wake receipt imported from shared-preferences at 2026-07-09T04:00:00.000Z.",
      ],
    });
  });

  it("does not record device wake validation from expired native receipts", () => {
    recordProviderDeviceBackgroundWakeValidation(
      buildNativeBackgroundReplayWakeReceipt(
        "google-drive",
        "apple-bg-task",
        1_783_569_600_000,
        {
          expired: true,
          source: "bg-task",
        },
      ),
    );

    expect(readProviderDefaultSyncValidationEvidence("google-drive")).toMatchObject({
      device_background_wake_verified: false,
    });
  });

  it("does not record device wake validation from foreground or synthetic receipts", () => {
    recordProviderDeviceBackgroundWakeValidation(
      buildNativeBackgroundReplayWakeReceipt(
        "google-drive",
        "frontend-lifecycle",
        1_783_569_600_000,
        {
          source: "frontend",
        },
      ),
    );

    expect(readProviderDefaultSyncValidationEvidence("google-drive")).toMatchObject({
      device_background_wake_verified: false,
    });
  });

  it("blocks background replay while disabled, in flight, or waiting for backoff", () => {
    const now = new Date("2026-07-08T12:00:00.000Z");

    expect(shouldRunBackgroundReplay(defaultBackgroundReplayState(), now)).toBe(true);
    expect(
      shouldRunBackgroundReplay({ ...defaultBackgroundReplayState(), enabled: false }, now),
    ).toBe(false);
    expect(
      shouldRunBackgroundReplay({ ...defaultBackgroundReplayState(), in_flight: true }, now),
    ).toBe(false);
    expect(
      shouldRunBackgroundReplay(
        { ...defaultBackgroundReplayState(), next_attempt_at: "2026-07-08T12:01:00.000Z" },
        now,
      ),
    ).toBe(false);
    expect(
      shouldRunBackgroundReplay(
        { ...defaultBackgroundReplayState(), next_attempt_at: "2026-07-08T11:59:00.000Z" },
        now,
      ),
    ).toBe(true);
  });

  it("records retry state after failure and clears it after success", () => {
    const failed = recordBackgroundReplayFailure(
      defaultBackgroundReplayState(),
      "Linked server is unreachable",
      new Date("2026-07-08T12:00:00.000Z"),
    );

    expect(failed).toMatchObject({
      in_flight: false,
      attempt_count: 1,
      last_attempt_at: "2026-07-08T12:00:00.000Z",
      next_attempt_at: "2026-07-08T12:00:30.000Z",
      last_error: "Linked server is unreachable",
    });

    expect(recordBackgroundReplaySuccess(failed, new Date("2026-07-08T12:05:00.000Z"))).toEqual({
      enabled: true,
      in_flight: false,
      attempt_count: 0,
      last_attempt_at: "2026-07-08T12:05:00.000Z",
      next_attempt_at: undefined,
      last_error: undefined,
    });
  });

  it("allows lifecycle wake retry for reachability failures only", () => {
    const now = new Date("2026-07-08T12:00:00.000Z");
    const waiting = {
      ...defaultBackgroundReplayState(),
      next_attempt_at: "2026-07-08T12:05:00.000Z",
    };

    expect(
      shouldRunBackgroundReplayOnLifecycleWake(
        { ...waiting, last_error: "Linked server is unreachable" },
        now,
      ),
    ).toBe(true);
    expect(
      shouldRunBackgroundReplayOnLifecycleWake(
        { ...waiting, last_error: "Replay preflight blocked: missing payload" },
        now,
      ),
    ).toBe(false);
  });

  it("gates background provider push on explicit enablement and a ready push plan", () => {
    const baseStatus: ProviderStatus = {
      connected: true,
      remote_contract: {
        status: "enabled",
        root_strategy: "drive-appdata-or-user-selected-folder",
        auth_model: "oauth2-offline-access",
        required_scopes: ["drive.file"],
        write_model: "local-sqlite-first-then-provider-api-push",
        conflict_model: "provider-revision-plus-local-replay-queue-validation",
      },
      remote_enablement: {
        provider: "google-drive",
        status: "ready_to_enable",
        transport_kind: "provider-api",
        enablement_boundary: "explicit_manual_provider_session",
        default_sync_enabled: false,
        required_scopes: ["drive.file"],
        blockers: [],
        steps: [],
      },
      remote_sync_enablement: {
        provider: "google-drive",
        enabled: true,
        enablement_boundary: "explicit_manual_provider_session",
      },
      default_sync_policy: {
        provider: "google-drive",
        status: "eligible_after_explicit_enablement",
        default_sync_enabled: false,
        automatic_enablement_allowed: false,
        policy_boundary: "explicit_manual_provider_session",
        readiness_gates: [],
        blockers: [],
        required_user_actions: [],
        validation_evidence: {
          production_oauth_verified: false,
          device_background_wake_verified: false,
          notes: [],
        },
      },
      remote_mutation_plans: [
        {
          provider: "google-drive",
          direction: "push",
          status: "ready",
          remote_mutation_enabled: true,
          dry_run: false,
          root_strategy: "drive-appdata-or-user-selected-folder",
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
          required_scopes: ["drive.file"],
          blockers: [],
        },
      ],
    };

    expect(shouldRunBackgroundProviderPush(baseStatus)).toBe(true);
    expect(
      shouldRunBackgroundProviderPush({
        ...baseStatus,
        remote_sync_enablement: {
          ...baseStatus.remote_sync_enablement,
          enabled: false,
        },
      }),
    ).toBe(false);
    expect(
      shouldRunBackgroundProviderPush({
        ...baseStatus,
        remote_enablement: {
          ...baseStatus.remote_enablement,
          status: "blocked",
          blockers: ["Google Drive remote sync requires a refresh token"],
        },
      }),
    ).toBe(false);
    expect(
      shouldRunBackgroundProviderPush({
        ...baseStatus,
        remote_mutation_plans: [
          {
            ...baseStatus.remote_mutation_plans[0],
            remote_mutation_enabled: false,
            dry_run: true,
            status: "dry_run_only",
          },
        ],
      }),
    ).toBe(false);
  });
});
