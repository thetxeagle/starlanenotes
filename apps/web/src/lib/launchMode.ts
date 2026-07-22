import type { RuntimeTarget, StorageMode, StorageSelection } from "@/lib/storageProviders";
import { supportsStorageSelection } from "@/lib/storageProviders";

export type AppLaunchMode = StorageMode;
let currentLaunchMode: AppLaunchMode = "server";

export interface LaunchModeResolution {
  selected: StorageSelection;
  runtime: RuntimeTarget;
  mode: AppLaunchMode;
  fallbackApplied: boolean;
}

export function resolveLaunchMode(
  selected: StorageSelection,
  runtime: RuntimeTarget,
): LaunchModeResolution {
  if (supportsStorageSelection(selected, runtime)) {
    return {
      selected,
      runtime,
      mode: selected.mode,
      fallbackApplied: false,
    };
  }

  return {
    selected,
    runtime,
    mode: "server",
    fallbackApplied: true,
  };
}

export function setCurrentLaunchMode(mode: AppLaunchMode): void {
  currentLaunchMode = mode;
}

export function getCurrentLaunchMode(): AppLaunchMode {
  return currentLaunchMode;
}
