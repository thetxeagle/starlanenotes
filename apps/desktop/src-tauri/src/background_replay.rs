use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const BACKGROUND_REPLAY_SCHEMA_VERSION: u64 = 1;
const DEFAULT_MIN_INTERVAL_SECONDS: u64 = 60;
const MIN_INTERVAL_SECONDS: u64 = 30;
const MAX_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeBackgroundReplaySchedulerDescriptor {
    schema_version: u64,
    provider: String,
    platform: String,
    durable_registration: bool,
    supports_os_scheduler: bool,
    status: String,
    default_min_interval_seconds: u64,
    os_scheduler_kind: String,
    os_task_identifier: String,
    execution_command: String,
    registration_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeBackgroundReplaySchedule {
    schema_version: u64,
    provider: String,
    linked_server_url: Option<String>,
    enabled: bool,
    min_interval_seconds: u64,
    registered_at_epoch_seconds: u64,
    updated_at_epoch_seconds: u64,
    next_wake_after_epoch_seconds: u64,
    platform_scheduler: String,
    os_scheduler_kind: String,
    os_task_identifier: String,
    execution_command: String,
    last_wake_reason: Option<String>,
    last_wake_at_epoch_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeBackgroundReplayWakeReceipt {
    provider: String,
    wake_reason: String,
    wake_epoch_millis: u64,
    os_task_identifier: Option<String>,
    os_scheduler_kind: Option<String>,
    expired: Option<bool>,
    source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeBackgroundReplayWakeReceiptImport {
    imported: bool,
    receipt: NativeBackgroundReplayWakeReceipt,
    schedule: NativeBackgroundReplaySchedule,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeBackgroundReplayRegistrationRequirement {
    kind: String,
    key: String,
    value: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeBackgroundReplayOsRegistrationPlan {
    schema_version: u64,
    provider: String,
    platform: String,
    supports_os_scheduler: bool,
    status: String,
    os_scheduler_kind: String,
    os_task_identifier: String,
    execution_command: String,
    registration_mode: String,
    required_native_entries: Vec<NativeBackgroundReplayRegistrationRequirement>,
    execution_flow: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlatformSchedulerTarget {
    platform: &'static str,
    supports_os_scheduler: bool,
    status: &'static str,
    os_scheduler_kind: &'static str,
    os_task_identifier_prefix: &'static str,
    registration_mode: &'static str,
}

#[tauri::command]
pub fn describe_native_background_replay_scheduler(
    provider: String,
) -> NativeBackgroundReplaySchedulerDescriptor {
    let target = platform_scheduler_target();
    let task_identifier = os_task_identifier(&target, &provider);
    NativeBackgroundReplaySchedulerDescriptor {
        schema_version: BACKGROUND_REPLAY_SCHEMA_VERSION,
        provider: provider.clone(),
        platform: target.platform.to_string(),
        durable_registration: true,
        supports_os_scheduler: target.supports_os_scheduler,
        status: target.status.to_string(),
        default_min_interval_seconds: DEFAULT_MIN_INTERVAL_SECONDS,
        os_scheduler_kind: target.os_scheduler_kind.to_string(),
        os_task_identifier: task_identifier,
        execution_command: "record_native_background_replay_wake".to_string(),
        registration_mode: target.registration_mode.to_string(),
    }
}

#[tauri::command]
pub fn describe_native_background_replay_os_registration_plan(
    provider: String,
) -> NativeBackgroundReplayOsRegistrationPlan {
    let target = platform_scheduler_target();
    build_os_registration_plan(&target, &provider)
}

#[tauri::command]
pub fn register_native_background_replay_schedule(
    app: AppHandle,
    provider: String,
    linked_server_url: Option<String>,
    min_interval_seconds: Option<u64>,
) -> Result<NativeBackgroundReplaySchedule, String> {
    let path = schedule_path(&app, &provider)?;
    let now = epoch_seconds();
    let previous = read_schedule_file(&path)?;
    let target = platform_scheduler_target();
    let normalized_min_interval = normalize_min_interval_seconds(min_interval_seconds);
    let registered_at = previous
        .as_ref()
        .map(|schedule| schedule.registered_at_epoch_seconds)
        .unwrap_or(now);
    let schedule = NativeBackgroundReplaySchedule {
        schema_version: BACKGROUND_REPLAY_SCHEMA_VERSION,
        provider: provider.clone(),
        linked_server_url,
        enabled: true,
        min_interval_seconds: normalized_min_interval,
        registered_at_epoch_seconds: registered_at,
        updated_at_epoch_seconds: now,
        next_wake_after_epoch_seconds: now + normalized_min_interval,
        platform_scheduler: target.status.to_string(),
        os_scheduler_kind: target.os_scheduler_kind.to_string(),
        os_task_identifier: os_task_identifier(&target, &provider),
        execution_command: "record_native_background_replay_wake".to_string(),
        last_wake_reason: previous
            .as_ref()
            .and_then(|schedule| schedule.last_wake_reason.clone()),
        last_wake_at_epoch_seconds: previous
            .and_then(|schedule| schedule.last_wake_at_epoch_seconds),
    };

    write_schedule_file(&path, &schedule)?;
    Ok(schedule)
}

#[tauri::command]
pub fn read_native_background_replay_schedule(
    app: AppHandle,
    provider: String,
) -> Result<Option<NativeBackgroundReplaySchedule>, String> {
    let path = schedule_path(&app, &provider)?;
    read_schedule_file(&path)
}

#[tauri::command]
pub fn clear_native_background_replay_schedule(
    app: AppHandle,
    provider: String,
) -> Result<bool, String> {
    let path = schedule_path(&app, &provider)?;
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(path).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn record_native_background_replay_wake(
    app: AppHandle,
    provider: String,
    wake_reason: String,
    min_interval_seconds: Option<u64>,
) -> Result<NativeBackgroundReplaySchedule, String> {
    let path = schedule_path(&app, &provider)?;
    let now = epoch_seconds();
    let target = platform_scheduler_target();
    let normalized_min_interval = normalize_min_interval_seconds(min_interval_seconds);
    let previous = read_schedule_file(&path)?;
    let schedule = NativeBackgroundReplaySchedule {
        schema_version: BACKGROUND_REPLAY_SCHEMA_VERSION,
        provider: provider.clone(),
        linked_server_url: previous
            .as_ref()
            .and_then(|schedule| schedule.linked_server_url.clone()),
        enabled: previous
            .as_ref()
            .map(|schedule| schedule.enabled)
            .unwrap_or(true),
        min_interval_seconds: previous
            .as_ref()
            .map(|schedule| schedule.min_interval_seconds)
            .unwrap_or(normalized_min_interval),
        registered_at_epoch_seconds: previous
            .as_ref()
            .map(|schedule| schedule.registered_at_epoch_seconds)
            .unwrap_or(now),
        updated_at_epoch_seconds: now,
        next_wake_after_epoch_seconds: now
            + previous
                .as_ref()
                .map(|schedule| schedule.min_interval_seconds)
                .unwrap_or(normalized_min_interval),
        platform_scheduler: target.status.to_string(),
        os_scheduler_kind: target.os_scheduler_kind.to_string(),
        os_task_identifier: os_task_identifier(&target, &provider),
        execution_command: "record_native_background_replay_wake".to_string(),
        last_wake_reason: Some(wake_reason),
        last_wake_at_epoch_seconds: Some(now),
    };

    write_schedule_file(&path, &schedule)?;
    Ok(schedule)
}

#[tauri::command]
pub fn import_native_background_replay_wake_receipt(
    app: AppHandle,
    receipt: NativeBackgroundReplayWakeReceipt,
    min_interval_seconds: Option<u64>,
) -> Result<NativeBackgroundReplayWakeReceiptImport, String> {
    let schedule = schedule_from_wake_receipt(&app, &receipt, min_interval_seconds)?;
    Ok(NativeBackgroundReplayWakeReceiptImport {
        imported: true,
        receipt,
        schedule,
    })
}

fn schedule_from_wake_receipt(
    app: &AppHandle,
    receipt: &NativeBackgroundReplayWakeReceipt,
    min_interval_seconds: Option<u64>,
) -> Result<NativeBackgroundReplaySchedule, String> {
    let path = schedule_path(app, &receipt.provider)?;
    let previous = read_schedule_file(&path)?;
    let target = platform_scheduler_target();
    let schedule =
        build_schedule_from_wake_receipt(&previous, &target, receipt, min_interval_seconds);
    write_schedule_file(&path, &schedule)?;
    Ok(schedule)
}

fn build_schedule_from_wake_receipt(
    previous: &Option<NativeBackgroundReplaySchedule>,
    target: &PlatformSchedulerTarget,
    receipt: &NativeBackgroundReplayWakeReceipt,
    min_interval_seconds: Option<u64>,
) -> NativeBackgroundReplaySchedule {
    let normalized_min_interval = normalize_min_interval_seconds(min_interval_seconds);
    let wake_epoch_seconds = receipt.wake_epoch_millis / 1000;
    let schedule_interval = previous
        .as_ref()
        .map(|schedule| schedule.min_interval_seconds)
        .unwrap_or(normalized_min_interval);
    NativeBackgroundReplaySchedule {
        schema_version: BACKGROUND_REPLAY_SCHEMA_VERSION,
        provider: receipt.provider.clone(),
        linked_server_url: previous
            .as_ref()
            .and_then(|schedule| schedule.linked_server_url.clone()),
        enabled: previous
            .as_ref()
            .map(|schedule| schedule.enabled)
            .unwrap_or(true),
        min_interval_seconds: schedule_interval,
        registered_at_epoch_seconds: previous
            .as_ref()
            .map(|schedule| schedule.registered_at_epoch_seconds)
            .unwrap_or(wake_epoch_seconds),
        updated_at_epoch_seconds: wake_epoch_seconds,
        next_wake_after_epoch_seconds: wake_epoch_seconds + schedule_interval,
        platform_scheduler: target.status.to_string(),
        os_scheduler_kind: receipt
            .os_scheduler_kind
            .clone()
            .unwrap_or_else(|| target.os_scheduler_kind.to_string()),
        os_task_identifier: receipt
            .os_task_identifier
            .clone()
            .unwrap_or_else(|| os_task_identifier(&target, &receipt.provider)),
        execution_command: "record_native_background_replay_wake".to_string(),
        last_wake_reason: Some(receipt.wake_reason.clone()),
        last_wake_at_epoch_seconds: Some(wake_epoch_seconds),
    }
}

fn schedule_path(app: &AppHandle, provider: &str) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("background-replay");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(format!("{}.json", sanitize_provider_file_stem(provider))))
}

fn read_schedule_file(path: &Path) -> Result<Option<NativeBackgroundReplaySchedule>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| e.to_string())
}

fn write_schedule_file(
    path: &Path,
    schedule: &NativeBackgroundReplaySchedule,
) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(schedule).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

fn normalize_min_interval_seconds(value: Option<u64>) -> u64 {
    value
        .unwrap_or(DEFAULT_MIN_INTERVAL_SECONDS)
        .clamp(MIN_INTERVAL_SECONDS, MAX_INTERVAL_SECONDS)
}

fn platform_scheduler_target() -> PlatformSchedulerTarget {
    match std::env::consts::OS {
        "macos" => PlatformSchedulerTarget {
            platform: "macos",
            supports_os_scheduler: true,
            status: "os_scheduler_registration_scaffold",
            os_scheduler_kind: "apple-bg-task-scheduler",
            os_task_identifier_prefix: "dev.starlane.notes.background-replay",
            registration_mode: "native-task-registration-required",
        },
        "ios" => PlatformSchedulerTarget {
            platform: "ios",
            supports_os_scheduler: true,
            status: "os_scheduler_registration_scaffold",
            os_scheduler_kind: "apple-bg-task-scheduler",
            os_task_identifier_prefix: "dev.starlane.notes.background-replay",
            registration_mode: "native-task-registration-required",
        },
        "android" => PlatformSchedulerTarget {
            platform: "android",
            supports_os_scheduler: true,
            status: "os_scheduler_registration_scaffold",
            os_scheduler_kind: "android-workmanager",
            os_task_identifier_prefix: "dev.starlane.notes.background-replay",
            registration_mode: "native-work-request-registration-required",
        },
        other => PlatformSchedulerTarget {
            platform: other,
            supports_os_scheduler: false,
            status: "durable_wake_intent_only",
            os_scheduler_kind: "unsupported",
            os_task_identifier_prefix: "dev.starlane.notes.background-replay",
            registration_mode: "foreground-lifecycle-only",
        },
    }
}

fn os_task_identifier(target: &PlatformSchedulerTarget, provider: &str) -> String {
    format!(
        "{}.{}",
        target.os_task_identifier_prefix,
        sanitize_provider_file_stem(provider)
    )
}

fn build_os_registration_plan(
    target: &PlatformSchedulerTarget,
    provider: &str,
) -> NativeBackgroundReplayOsRegistrationPlan {
    let task_identifier = os_task_identifier(target, provider);
    NativeBackgroundReplayOsRegistrationPlan {
        schema_version: BACKGROUND_REPLAY_SCHEMA_VERSION,
        provider: provider.to_string(),
        platform: target.platform.to_string(),
        supports_os_scheduler: target.supports_os_scheduler,
        status: if target.supports_os_scheduler {
            "native_registration_plan_ready".to_string()
        } else {
            "foreground_lifecycle_only".to_string()
        },
        os_scheduler_kind: target.os_scheduler_kind.to_string(),
        os_task_identifier: task_identifier.clone(),
        execution_command: "record_native_background_replay_wake".to_string(),
        registration_mode: target.registration_mode.to_string(),
        required_native_entries: native_registration_requirements(target, &task_identifier),
        execution_flow: native_execution_flow(target),
    }
}

fn native_registration_requirements(
    target: &PlatformSchedulerTarget,
    task_identifier: &str,
) -> Vec<NativeBackgroundReplayRegistrationRequirement> {
    match target.os_scheduler_kind {
        "apple-bg-task-scheduler" => vec![
            NativeBackgroundReplayRegistrationRequirement {
                kind: "info-plist".to_string(),
                key: "BGTaskSchedulerPermittedIdentifiers".to_string(),
                value: task_identifier.to_string(),
                reason: "Apple BGTaskScheduler refuses registration unless the task identifier is present in Info.plist".to_string(),
            },
            NativeBackgroundReplayRegistrationRequirement {
                kind: "native-code".to_string(),
                key: "StarlaneBackgroundReplayScheduler.registerDefaultSchedules".to_string(),
                value: task_identifier.to_string(),
                reason: "The native Apple bootstrap registers BGProcessingTask handlers for replay wakeups before the Tauri app starts".to_string(),
            },
            NativeBackgroundReplayRegistrationRequirement {
                kind: "native-code".to_string(),
                key: "BGTaskRequest.earliestBeginDate".to_string(),
                value: "min_interval_seconds".to_string(),
                reason: "The OS-managed task should honor the durable replay schedule interval".to_string(),
            },
        ],
        "android-workmanager" => vec![
            NativeBackgroundReplayRegistrationRequirement {
                kind: "android-worker".to_string(),
                key: "WorkManager.enqueueUniquePeriodicWork".to_string(),
                value: task_identifier.to_string(),
                reason: "Android replay wakeups should be scheduled as unique periodic work per provider".to_string(),
            },
            NativeBackgroundReplayRegistrationRequirement {
                kind: "android-worker".to_string(),
                key: "ExistingPeriodicWorkPolicy.UPDATE".to_string(),
                value: task_identifier.to_string(),
                reason: "Updating the durable replay schedule should replace the previous provider work request".to_string(),
            },
            NativeBackgroundReplayRegistrationRequirement {
                kind: "android-worker".to_string(),
                key: "NetworkType.CONNECTED".to_string(),
                value: "required".to_string(),
                reason: "Queued replay should only wake for server/provider sync when the device is connected".to_string(),
            },
        ],
        _ => Vec::new(),
    }
}

fn native_execution_flow(target: &PlatformSchedulerTarget) -> Vec<String> {
    if !target.supports_os_scheduler {
        return vec![
            "record foreground lifecycle wake intent".to_string(),
            "let the web scheduler run queued replay while the app is open".to_string(),
        ];
    }

    match target.os_scheduler_kind {
        "apple-bg-task-scheduler" => vec![
            "OS launches registered BGTask handler".to_string(),
            "native handler records provider, task identifier, wake reason, expiry, and wake time to the Apple wake receipt store".to_string(),
            "app runtime imports the wake receipt through import_native_background_replay_wake_receipt".to_string(),
            "handler reschedules the next BGTaskRequest using next_wake_after_epoch_seconds".to_string(),
        ],
        "android-workmanager" => vec![
            "WorkManager runs the provider's unique periodic Worker".to_string(),
            "Worker records provider, unique work name, wake reason, and wake time to the Android wake receipt store".to_string(),
            "app runtime imports the wake receipt through import_native_background_replay_wake_receipt".to_string(),
            "Worker keeps the unique periodic work aligned with min_interval_seconds".to_string(),
        ],
        _ => Vec::new(),
    }
}

fn sanitize_provider_file_stem(provider: &str) -> String {
    let sanitized: String = provider
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_background_replay_min_interval() {
        assert_eq!(
            normalize_min_interval_seconds(None),
            DEFAULT_MIN_INTERVAL_SECONDS
        );
        assert_eq!(
            normalize_min_interval_seconds(Some(1)),
            MIN_INTERVAL_SECONDS
        );
        assert_eq!(
            normalize_min_interval_seconds(Some(MAX_INTERVAL_SECONDS + 1)),
            MAX_INTERVAL_SECONDS
        );
    }

    #[test]
    fn sanitizes_provider_file_stem() {
        assert_eq!(sanitize_provider_file_stem("icloud-drive"), "icloud-drive");
        assert_eq!(
            sanitize_provider_file_stem("../google drive"),
            "google_drive"
        );
        assert_eq!(sanitize_provider_file_stem("///"), "default");
    }

    #[test]
    fn builds_stable_os_task_identifier() {
        let target = PlatformSchedulerTarget {
            platform: "ios",
            supports_os_scheduler: true,
            status: "os_scheduler_registration_scaffold",
            os_scheduler_kind: "apple-bg-task-scheduler",
            os_task_identifier_prefix: "dev.starlane.notes.background-replay",
            registration_mode: "native-task-registration-required",
        };

        assert_eq!(
            os_task_identifier(&target, "../google drive"),
            "dev.starlane.notes.background-replay.google_drive"
        );
    }

    #[test]
    fn current_platform_scheduler_target_is_explicit() {
        let target = platform_scheduler_target();
        assert!(!target.platform.is_empty());
        assert!(!target.status.is_empty());
        assert!(!target.os_scheduler_kind.is_empty());
        assert!(!target.registration_mode.is_empty());
    }

    #[test]
    fn builds_apple_background_registration_plan() {
        let target = PlatformSchedulerTarget {
            platform: "ios",
            supports_os_scheduler: true,
            status: "os_scheduler_registration_scaffold",
            os_scheduler_kind: "apple-bg-task-scheduler",
            os_task_identifier_prefix: "dev.starlane.notes.background-replay",
            registration_mode: "native-task-registration-required",
        };
        let plan = build_os_registration_plan(&target, "icloud-drive");

        assert_eq!(plan.status, "native_registration_plan_ready");
        assert_eq!(plan.os_scheduler_kind, "apple-bg-task-scheduler");
        assert_eq!(
            plan.os_task_identifier,
            "dev.starlane.notes.background-replay.icloud-drive"
        );
        assert!(plan.required_native_entries.iter().any(|entry| {
            entry.kind == "info-plist"
                && entry.key == "BGTaskSchedulerPermittedIdentifiers"
                && entry.value == "dev.starlane.notes.background-replay.icloud-drive"
        }));
        assert!(plan.required_native_entries.iter().any(|entry| {
            entry.kind == "native-code"
                && entry.key == "StarlaneBackgroundReplayScheduler.registerDefaultSchedules"
        }));
        assert!(plan
            .execution_flow
            .iter()
            .any(|step| step.contains("BGTask")));
    }

    #[test]
    fn builds_android_background_registration_plan() {
        let target = PlatformSchedulerTarget {
            platform: "android",
            supports_os_scheduler: true,
            status: "os_scheduler_registration_scaffold",
            os_scheduler_kind: "android-workmanager",
            os_task_identifier_prefix: "dev.starlane.notes.background-replay",
            registration_mode: "native-work-request-registration-required",
        };
        let plan = build_os_registration_plan(&target, "google-drive");

        assert_eq!(plan.status, "native_registration_plan_ready");
        assert_eq!(plan.os_scheduler_kind, "android-workmanager");
        assert!(plan.required_native_entries.iter().any(|entry| {
            entry.kind == "android-worker"
                && entry.key == "WorkManager.enqueueUniquePeriodicWork"
                && entry.value == "dev.starlane.notes.background-replay.google-drive"
        }));
        assert!(plan
            .execution_flow
            .iter()
            .any(|step| step.contains("WorkManager")));
    }

    #[test]
    fn builds_schedule_from_apple_wake_receipt() {
        let target = PlatformSchedulerTarget {
            platform: "ios",
            supports_os_scheduler: true,
            status: "os_scheduler_registration_scaffold",
            os_scheduler_kind: "apple-bg-task-scheduler",
            os_task_identifier_prefix: "dev.starlane.notes.background-replay",
            registration_mode: "native-task-registration-required",
        };
        let receipt = NativeBackgroundReplayWakeReceipt {
            provider: "icloud-drive".to_string(),
            wake_reason: "apple-bg-task".to_string(),
            wake_epoch_millis: 1_783_569_600_000,
            os_task_identifier: Some(
                "dev.starlane.notes.background-replay.icloud-drive".to_string(),
            ),
            os_scheduler_kind: Some("apple-bg-task-scheduler".to_string()),
            expired: Some(false),
            source: Some("ns-user-defaults".to_string()),
        };

        let schedule = build_schedule_from_wake_receipt(&None, &target, &receipt, Some(120));

        assert_eq!(schedule.provider, "icloud-drive");
        assert_eq!(schedule.os_scheduler_kind, "apple-bg-task-scheduler");
        assert_eq!(
            schedule.os_task_identifier,
            "dev.starlane.notes.background-replay.icloud-drive"
        );
        assert_eq!(schedule.last_wake_reason, Some("apple-bg-task".to_string()));
        assert_eq!(schedule.last_wake_at_epoch_seconds, Some(1_783_569_600));
        assert_eq!(schedule.next_wake_after_epoch_seconds, 1_783_569_720);
    }

    #[test]
    fn wake_receipt_preserves_existing_schedule_interval_and_server() {
        let target = PlatformSchedulerTarget {
            platform: "android",
            supports_os_scheduler: true,
            status: "os_scheduler_registration_scaffold",
            os_scheduler_kind: "android-workmanager",
            os_task_identifier_prefix: "dev.starlane.notes.background-replay",
            registration_mode: "native-work-request-registration-required",
        };
        let previous = NativeBackgroundReplaySchedule {
            schema_version: BACKGROUND_REPLAY_SCHEMA_VERSION,
            provider: "google-drive".to_string(),
            linked_server_url: Some("https://notes.example.test/api/v1".to_string()),
            enabled: true,
            min_interval_seconds: 300,
            registered_at_epoch_seconds: 1_783_560_000,
            updated_at_epoch_seconds: 1_783_560_000,
            next_wake_after_epoch_seconds: 1_783_560_300,
            platform_scheduler: "os_scheduler_registration_scaffold".to_string(),
            os_scheduler_kind: "android-workmanager".to_string(),
            os_task_identifier: "dev.starlane.notes.background-replay.google-drive".to_string(),
            execution_command: "record_native_background_replay_wake".to_string(),
            last_wake_reason: None,
            last_wake_at_epoch_seconds: None,
        };
        let receipt = NativeBackgroundReplayWakeReceipt {
            provider: "google-drive".to_string(),
            wake_reason: "android-workmanager".to_string(),
            wake_epoch_millis: 1_783_569_600_000,
            os_task_identifier: Some(
                "dev.starlane.notes.background-replay.google-drive".to_string(),
            ),
            os_scheduler_kind: Some("android-workmanager".to_string()),
            expired: None,
            source: Some("shared-preferences".to_string()),
        };

        let schedule =
            build_schedule_from_wake_receipt(&Some(previous), &target, &receipt, Some(60));

        assert_eq!(
            schedule.linked_server_url,
            Some("https://notes.example.test/api/v1".to_string())
        );
        assert_eq!(schedule.min_interval_seconds, 300);
        assert_eq!(schedule.registered_at_epoch_seconds, 1_783_560_000);
        assert_eq!(schedule.next_wake_after_epoch_seconds, 1_783_569_900);
    }
}
