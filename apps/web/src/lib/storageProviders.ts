export type StorageMode = "server" | "local";

export type RuntimeTarget = "web" | "tauri-desktop" | "tauri-mobile";

export type SyncProviderKind = "none" | "icloud-drive" | "google-drive";

export interface SyncProviderDescriptor {
  kind: SyncProviderKind;
  label: string;
  requiresAuth: boolean;
  supportedTargets: RuntimeTarget[];
}

export interface StorageSelection {
  mode: StorageMode;
  provider: SyncProviderKind;
}

const STORAGE_MODE_KEY = "starlane_storage_mode";
const SYNC_PROVIDER_KEY = "starlane_sync_provider";

const PROVIDERS: Record<SyncProviderKind, SyncProviderDescriptor> = {
  none: {
    kind: "none",
    label: "No provider (local only)",
    requiresAuth: false,
    supportedTargets: ["web", "tauri-desktop", "tauri-mobile"],
  },
  "icloud-drive": {
    kind: "icloud-drive",
    label: "iCloud Drive",
    requiresAuth: true,
    supportedTargets: ["tauri-desktop", "tauri-mobile"],
  },
  "google-drive": {
    kind: "google-drive",
    label: "Google Drive",
    requiresAuth: true,
    supportedTargets: ["web", "tauri-desktop", "tauri-mobile"],
  },
};

function safeLocalStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  return window.localStorage;
}

function isStorageMode(value: string | null): value is StorageMode {
  return value === "server" || value === "local";
}

function isSyncProvider(value: string | null): value is SyncProviderKind {
  return value === "none" || value === "icloud-drive" || value === "google-drive";
}

export function getStorageMode(): StorageMode {
  const value = safeLocalStorage()?.getItem(STORAGE_MODE_KEY) ?? null;
  return isStorageMode(value) ? value : "server";
}

export function setStorageMode(mode: StorageMode): void {
  safeLocalStorage()?.setItem(STORAGE_MODE_KEY, mode);
}

export function getSyncProvider(): SyncProviderKind {
  const value = safeLocalStorage()?.getItem(SYNC_PROVIDER_KEY) ?? null;
  return isSyncProvider(value) ? value : "none";
}

export function setSyncProvider(provider: SyncProviderKind): void {
  safeLocalStorage()?.setItem(SYNC_PROVIDER_KEY, provider);
}

export function clearStorageSelection(): void {
  const storage = safeLocalStorage();
  storage?.removeItem(STORAGE_MODE_KEY);
  storage?.removeItem(SYNC_PROVIDER_KEY);
}

export function hasStoredStorageSelection(): boolean {
  return isStorageMode(safeLocalStorage()?.getItem(STORAGE_MODE_KEY) ?? null);
}

export function getProviderDescriptor(provider: SyncProviderKind): SyncProviderDescriptor {
  return PROVIDERS[provider];
}

export function listProvidersForTarget(target: RuntimeTarget): SyncProviderDescriptor[] {
  return Object.values(PROVIDERS).filter((provider) => provider.supportedTargets.includes(target));
}

export function getStorageSelection(): StorageSelection {
  return { mode: getStorageMode(), provider: getSyncProvider() };
}

export function supportsStorageSelection(
  selection: StorageSelection,
  target: RuntimeTarget,
): boolean {
  if (selection.mode === "server") return true;
  const provider = getProviderDescriptor(selection.provider);
  return provider.supportedTargets.includes(target);
}
