import { getApiBase, isTauri } from "@/lib/api";
import { dataClient } from "@/lib/dataClient";
import type { LocalSyncReplayServerResultRecord } from "@/lib/dataClient";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import type { ProviderStatus } from "@/lib/providerAdapters";
import {
  readProviderDefaultSyncValidationEvidence,
  writeProviderDefaultSyncValidationEvidence,
} from "@/lib/providerSession";
import { getSyncProvider, type SyncProviderKind } from "@/lib/storageProviders";

const BACKGROUND_REPLAY_STATE_KEY = "starlane_background_replay_state_v1";
export const BACKGROUND_REPLAY_INTERVAL_MS = 60_000;
export const BACKGROUND_REPLAY_MIN_BACKOFF_MS = 30_000;
export const BACKGROUND_REPLAY_MAX_BACKOFF_MS = 15 * 60_000;

export interface BackgroundReplayState {
  enabled: boolean;
  in_flight: boolean;
  attempt_count: number;
  last_attempt_at?: string;
  next_attempt_at?: string;
  last_error?: string;
}

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

interface NativeBackgroundReplayScheduleArgs {
  provider: SyncProviderKind;
  linkedServerUrl: string;
  minIntervalSeconds: number;
}

export interface NativeBackgroundReplaySchedulerDescriptor {
  schema_version: number;
  provider: SyncProviderKind;
  platform: string;
  durable_registration: boolean;
  supports_os_scheduler: boolean;
  status: string;
  default_min_interval_seconds: number;
  os_scheduler_kind: string;
  os_task_identifier: string;
  execution_command: string;
  registration_mode: string;
}

export interface NativeBackgroundReplayRegistrationRequirement {
  kind: string;
  key: string;
  value: string;
  reason: string;
}

export interface NativeBackgroundReplayOsRegistrationPlan {
  schema_version: number;
  provider: SyncProviderKind;
  platform: string;
  supports_os_scheduler: boolean;
  status: string;
  os_scheduler_kind: string;
  os_task_identifier: string;
  execution_command: string;
  registration_mode: string;
  required_native_entries: NativeBackgroundReplayRegistrationRequirement[];
  execution_flow: string[];
}

export interface NativeBackgroundReplayWakeReceipt {
  provider: SyncProviderKind;
  wake_reason: string;
  wake_epoch_millis: number;
  os_task_identifier?: string;
  os_scheduler_kind?: string;
  expired?: boolean;
  source?: string;
}

export interface NativeBackgroundReplayWakeReceiptImport {
  imported: boolean;
  receipt: NativeBackgroundReplayWakeReceipt;
  schedule: {
    schema_version: number;
    provider: SyncProviderKind;
    linked_server_url?: string;
    enabled: boolean;
    min_interval_seconds: number;
    registered_at_epoch_seconds: number;
    updated_at_epoch_seconds: number;
    next_wake_after_epoch_seconds: number;
    platform_scheduler: string;
    os_scheduler_kind: string;
    os_task_identifier: string;
    execution_command: string;
    last_wake_reason?: string;
    last_wake_at_epoch_seconds?: number;
  };
}

export function defaultBackgroundReplayState(): BackgroundReplayState {
  return {
    enabled: true,
    in_flight: false,
    attempt_count: 0,
  };
}

export function computeBackgroundReplayBackoffMs(attemptCount: number): number {
  const boundedAttempt = Math.max(1, attemptCount);
  const backoff = BACKGROUND_REPLAY_MIN_BACKOFF_MS * 2 ** (boundedAttempt - 1);
  return Math.min(backoff, BACKGROUND_REPLAY_MAX_BACKOFF_MS);
}

export function buildNativeBackgroundReplayScheduleArgs(
  provider: SyncProviderKind = getSyncProvider(),
  linkedServerUrl: string = getApiBase(),
): NativeBackgroundReplayScheduleArgs {
  return {
    provider,
    linkedServerUrl,
    minIntervalSeconds: Math.ceil(BACKGROUND_REPLAY_INTERVAL_MS / 1000),
  };
}

export function buildNativeBackgroundReplayWakeArgs(
  provider: SyncProviderKind = getSyncProvider(),
  wakeReason = "frontend-lifecycle",
): { provider: SyncProviderKind; wakeReason: string; minIntervalSeconds: number } {
  return {
    provider,
    wakeReason,
    minIntervalSeconds: Math.ceil(BACKGROUND_REPLAY_INTERVAL_MS / 1000),
  };
}

export function buildNativeBackgroundReplayWakeReceipt(
  provider: SyncProviderKind,
  wakeReason: string,
  wakeEpochMillis: number,
  options: {
    osTaskIdentifier?: string;
    osSchedulerKind?: string;
    expired?: boolean;
    source?: string;
  } = {},
): NativeBackgroundReplayWakeReceipt {
  return {
    provider,
    wake_reason: wakeReason,
    wake_epoch_millis: wakeEpochMillis,
    os_task_identifier: options.osTaskIdentifier,
    os_scheduler_kind: options.osSchedulerKind,
    expired: options.expired,
    source: options.source,
  };
}

export function isOsDeliveredBackgroundWakeReceipt(
  receipt: NativeBackgroundReplayWakeReceipt,
): boolean {
  if (receipt.expired) return false;
  return (
    receipt.os_scheduler_kind === "apple-bg-task-scheduler" ||
    receipt.os_scheduler_kind === "android-workmanager"
  );
}

export function recordProviderDeviceBackgroundWakeValidation(
  receipt: NativeBackgroundReplayWakeReceipt,
): void {
  if (!isOsDeliveredBackgroundWakeReceipt(receipt)) return;
  const verifiedAt = new Date(receipt.wake_epoch_millis).toISOString();
  const existingEvidence = readProviderDefaultSyncValidationEvidence(receipt.provider);
  writeProviderDefaultSyncValidationEvidence(receipt.provider, {
    ...existingEvidence,
    provider: receipt.provider,
    device_background_wake_verified: true,
    device_background_wake_verified_at: verifiedAt,
    notes: [
      `${receipt.provider} native background wake receipt imported from ${receipt.source ?? "native scheduler"} at ${verifiedAt}.`,
      ...existingEvidence.notes.filter(
        (note) => !note.includes("OS-delivered native background wake execution"),
      ),
    ],
  });
}

async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriCoreModule })
    .__TAURI_INTERNALS__;
  if (internals?.invoke) return internals.invoke<T>(cmd, args);

  const coreModulePath = "@tauri-apps/api/core";
  const coreMod = (await import(/* @vite-ignore */ coreModulePath)) as unknown as TauriCoreModule;
  return coreMod.invoke<T>(cmd, args);
}

export async function describeNativeBackgroundReplayScheduler(
  provider: SyncProviderKind = getSyncProvider(),
): Promise<NativeBackgroundReplaySchedulerDescriptor | null> {
  if (!isTauri()) return null;
  try {
    return await invokeTauri<NativeBackgroundReplaySchedulerDescriptor>(
      "describe_native_background_replay_scheduler",
      { provider },
    );
  } catch {
    return null;
  }
}

export async function describeNativeBackgroundReplayOsRegistrationPlan(
  provider: SyncProviderKind = getSyncProvider(),
): Promise<NativeBackgroundReplayOsRegistrationPlan | null> {
  if (!isTauri()) return null;
  try {
    return await invokeTauri<NativeBackgroundReplayOsRegistrationPlan>(
      "describe_native_background_replay_os_registration_plan",
      { provider },
    );
  } catch {
    return null;
  }
}

export async function importNativeBackgroundReplayWakeReceipt(
  receipt: NativeBackgroundReplayWakeReceipt,
): Promise<NativeBackgroundReplayWakeReceiptImport | null> {
  if (!isTauri()) return null;
  try {
    const result = await invokeTauri<NativeBackgroundReplayWakeReceiptImport>(
      "import_native_background_replay_wake_receipt",
      {
        receipt,
        minIntervalSeconds: Math.ceil(BACKGROUND_REPLAY_INTERVAL_MS / 1000),
      },
    );
    if (result.imported) {
      recordProviderDeviceBackgroundWakeValidation(result.receipt);
    }
    return result;
  } catch {
    return null;
  }
}

async function registerNativeBackgroundReplaySchedule(): Promise<void> {
  try {
    await invokeTauri("register_native_background_replay_schedule", {
      ...buildNativeBackgroundReplayScheduleArgs(),
    });
  } catch {
    // Older native shells can still run foreground/lifecycle replay.
  }
}

export function shouldRunBackgroundReplay(
  state: BackgroundReplayState,
  now: Date = new Date(),
): boolean {
  if (!state.enabled || state.in_flight) return false;
  if (!state.next_attempt_at) return true;
  return Date.parse(state.next_attempt_at) <= now.getTime();
}

export function shouldRunBackgroundReplayOnLifecycleWake(
  state: BackgroundReplayState,
  now: Date = new Date(),
): boolean {
  if (!state.enabled || state.in_flight) return false;
  if (!state.next_attempt_at) return true;
  if (Date.parse(state.next_attempt_at) <= now.getTime()) return true;
  return state.last_error === "Linked server is unreachable";
}

export function recordBackgroundReplaySuccess(
  state: BackgroundReplayState,
  now: Date = new Date(),
): BackgroundReplayState {
  return {
    ...state,
    in_flight: false,
    attempt_count: 0,
    last_attempt_at: now.toISOString(),
    next_attempt_at: undefined,
    last_error: undefined,
  };
}

export function recordBackgroundReplayFailure(
  state: BackgroundReplayState,
  error: string,
  now: Date = new Date(),
): BackgroundReplayState {
  const attempt_count = state.attempt_count + 1;
  return {
    ...state,
    in_flight: false,
    attempt_count,
    last_attempt_at: now.toISOString(),
    next_attempt_at: new Date(now.getTime() + computeBackgroundReplayBackoffMs(attempt_count))
      .toISOString(),
    last_error: error,
  };
}

export function shouldRunBackgroundProviderPush(status: ProviderStatus): boolean {
  if (!status.remote_sync_enablement.enabled) return false;
  if (status.remote_enablement.status !== "ready_to_enable") return false;
  const pushPlan = status.remote_mutation_plans.find((plan) => plan.direction === "push");
  return Boolean(pushPlan?.remote_mutation_enabled && !pushPlan.dry_run);
}

export function formatBackgroundReplayPartialResult(
  result: Pick<
    LocalSyncReplayServerResultRecord,
    | "attempted_operation_count"
    | "applied_operation_count"
    | "skipped_operation_count"
    | "failed_operation_count"
    | "unsupported_operation_count"
  >,
): string {
  return [
    `${result.attempted_operation_count} attempted`,
    `${result.applied_operation_count} applied`,
    `${result.skipped_operation_count} skipped`,
    `${result.failed_operation_count} failed`,
    `${result.unsupported_operation_count} unsupported`,
  ].join(", ");
}

function readBackgroundReplayState(): BackgroundReplayState {
  if (typeof window === "undefined") return defaultBackgroundReplayState();
  const raw = window.localStorage.getItem(BACKGROUND_REPLAY_STATE_KEY);
  if (!raw) return defaultBackgroundReplayState();
  try {
    return { ...defaultBackgroundReplayState(), ...JSON.parse(raw) };
  } catch {
    return defaultBackgroundReplayState();
  }
}

function writeBackgroundReplayState(state: BackgroundReplayState): void {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(BACKGROUND_REPLAY_STATE_KEY, JSON.stringify(state));
}

async function linkedServerReachable(): Promise<boolean> {
  try {
    const response = await fetch(`${getApiBase()}/health`, { method: "GET" });
    return response.ok;
  } catch {
    return false;
  }
}

async function runBackgroundProviderPushAttempt(): Promise<boolean> {
  const provider = getSyncProvider();
  if (provider !== "google-drive" && provider !== "icloud-drive") return false;

  const status = await dataClient.getProviderStatus();
  if (!shouldRunBackgroundProviderPush(status)) return false;

  const readiness = await dataClient.getProviderSyncReadiness();
  if (!readiness.ready) {
    throw new Error(`Provider background sync blocked: ${readiness.blockers.join("; ")}`);
  }

  const result = await dataClient.syncWithProvider("push");
  if (result.remote_mutation_attempt?.status === "blocked") {
    throw new Error(
      `Provider background sync blocked: ${result.remote_mutation_attempt.blockers.join("; ")}`,
    );
  }

  return Boolean(result.remote_mutation_attempt?.applied || result.pushed > 0);
}

async function runBackgroundReplayAttempt(): Promise<void> {
  const queued = await dataClient.listQueuedSyncOperations();
  let replayWorkCompleted = false;
  if (queued.length > 0) {
    if (!(await linkedServerReachable())) {
      throw new Error("Linked server is unreachable");
    }
    const preflight = await dataClient.preflightQueuedSyncReplayManifest();
    if (!preflight.ready) {
      throw new Error(`Replay preflight blocked: ${preflight.blockers.join("; ")}`);
    }
    const result = await dataClient.runQueuedSyncReplay();
    const clearableIds = result.results
      .filter((item) => item.status === "applied" || item.status === "skipped")
      .map((item) => item.operation_id);
    if (clearableIds.length > 0) {
      await dataClient.clearQueuedSyncOperations(clearableIds);
    }
    if (result.failed_operation_count > 0 || result.unsupported_operation_count > 0) {
      throw new Error(
        `Replay completed partially: ${formatBackgroundReplayPartialResult(result)}`,
      );
    }
    replayWorkCompleted = true;
  }

  const providerWorkCompleted = await runBackgroundProviderPushAttempt();
  if (!replayWorkCompleted && !providerWorkCompleted) {
    writeBackgroundReplayState(recordBackgroundReplaySuccess(readBackgroundReplayState()));
    return;
  }
}

export function startNativeBackgroundReplayScheduler(): () => void {
  if (!isTauri() || getCurrentLaunchMode() !== "local") {
    return () => {};
  }

  let stopped = false;
  let running = false;

  void registerNativeBackgroundReplaySchedule();

  const tick = async (lifecycleWake = false) => {
    if (stopped || running) return;
    const state = readBackgroundReplayState();
    const shouldRun = lifecycleWake
      ? shouldRunBackgroundReplayOnLifecycleWake(state)
      : shouldRunBackgroundReplay(state);
    if (!shouldRun) return;
    running = true;
    writeBackgroundReplayState({ ...state, in_flight: true });
    try {
      await runBackgroundReplayAttempt();
      writeBackgroundReplayState(recordBackgroundReplaySuccess(readBackgroundReplayState()));
    } catch (error) {
      writeBackgroundReplayState(
        recordBackgroundReplayFailure(
          readBackgroundReplayState(),
          error instanceof Error ? error.message : "Background replay failed",
        ),
      );
    } finally {
      running = false;
    }
  };

  void tick();
  const interval = window.setInterval(() => {
    void tick();
  }, BACKGROUND_REPLAY_INTERVAL_MS);
  const lifecycleTick = () => {
    void tick(true);
  };
  const visibilityTick = () => {
    if (document.visibilityState === "visible") {
      lifecycleTick();
    }
  };

  window.addEventListener("online", lifecycleTick);
  window.addEventListener("focus", lifecycleTick);
  document.addEventListener("visibilitychange", visibilityTick);

  return () => {
    stopped = true;
    window.clearInterval(interval);
    window.removeEventListener("online", lifecycleTick);
    window.removeEventListener("focus", lifecycleTick);
    document.removeEventListener("visibilitychange", visibilityTick);
  };
}
