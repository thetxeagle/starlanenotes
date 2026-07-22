import { isTauri } from "@/lib/api";
import type { RuntimeTarget } from "@/lib/storageProviders";

export interface VaultLocation {
  path: string;
  using_default: boolean;
  configured?: boolean;
}

export interface VaultFolderSummary {
  name: string;
  path: string;
  current: boolean;
}

export interface CorruptedVaultRecord {
  path: string;
  name: string;
  message: string;
  detectedAt: string;
}

const VAULT_NAME_KEY = "starlane_vault_name";
const CORRUPTED_VAULTS_KEY = "starlane_corrupted_vaults";

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

interface TauriDialogModule {
  open: (options: {
    title?: string;
    directory?: boolean;
    multiple?: boolean;
    defaultPath?: string;
    canCreateDirectories?: boolean;
    filters?: { name: string; extensions: string[] }[];
  }) => Promise<string | string[] | null>;
  save: (options: {
    title?: string;
    defaultPath?: string;
    canCreateDirectories?: boolean;
    filters?: { name: string; extensions: string[] }[];
  }) => Promise<string | null>;
}

async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriCoreModule })
    .__TAURI_INTERNALS__;
  if (internals?.invoke) {
    return internals.invoke<T>(cmd, args);
  }

  const coreModulePath = "@tauri-apps/api/core";
  const coreMod = (await import(/* @vite-ignore */ coreModulePath)) as unknown as TauriCoreModule;
  return coreMod.invoke<T>(cmd, args);
}

export function canUseNativeVaultControls(runtime: RuntimeTarget): boolean {
  return isTauri() && runtime === "tauri-desktop";
}

export async function getVaultLocation(): Promise<VaultLocation> {
  return invokeTauri<VaultLocation>("get_vault_location");
}

export async function getVaultLocationStatus(): Promise<VaultLocation> {
  return invokeTauri<VaultLocation>("get_vault_location_status");
}

export async function listVaultFolders(): Promise<VaultFolderSummary[]> {
  return invokeTauri<VaultFolderSummary[]>("list_vault_folders");
}

export async function createVaultFolder(name: string): Promise<VaultLocation> {
  const folderName = sanitizeVaultFolderName(name);
  localStorage.setItem(VAULT_NAME_KEY, folderName);
  const next = await invokeTauri<VaultLocation>("create_vault_folder", { name: folderName });
  clearCorruptedVault(next.path);
  return next;
}

export async function moveVaultToICloud(name?: string): Promise<VaultLocation> {
  const folderName = name ? sanitizeVaultFolderName(name) : undefined;
  const next = await invokeTauri<VaultLocation>("move_vault_to_icloud", {
    folderName,
  });
  localStorage.setItem(VAULT_NAME_KEY, getVaultFolderName(next.path));
  return next;
}

export async function setVaultPath(path: string): Promise<VaultLocation> {
  const next = await invokeTauri<VaultLocation>("set_vault_path", { path });
  localStorage.setItem(VAULT_NAME_KEY, getVaultFolderName(next.path));
  clearCorruptedVault(next.path);
  return next;
}

function sanitizeVaultFolderName(name: string): string {
  const sanitized = name
    .trim()
    .replace(/[\\/:*?"<>|]+/g, "-")
    .replace(/\s+/g, " ")
    .slice(0, 80);
  return sanitized || "Starlane Notes Vault";
}

export function getVaultFolderName(path?: string): string {
  if (!path) {
    try {
      return localStorage.getItem(VAULT_NAME_KEY) || "Starlane Notes Vault";
    } catch {
      return "Starlane Notes Vault";
    }
  }
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || "Starlane Notes Vault";
}

export function readCorruptedVaults(): CorruptedVaultRecord[] {
  try {
    const raw = localStorage.getItem(CORRUPTED_VAULTS_KEY);
    if (!raw) return [];
    const records = JSON.parse(raw);
    if (!Array.isArray(records)) return [];
    return records.filter(isCorruptedVaultRecord);
  } catch {
    return [];
  }
}

export function getCorruptedVaultRecord(path: string): CorruptedVaultRecord | null {
  const normalizedPath = normalizeVaultPathForComparison(path);
  return (
    readCorruptedVaults().find(
      (record) => normalizeVaultPathForComparison(record.path) === normalizedPath,
    ) ?? null
  );
}

export function markCorruptedVault(path: string, message: string): CorruptedVaultRecord {
  const record: CorruptedVaultRecord = {
    path,
    name: getVaultFolderName(path),
    message: message.trim() || "Vault health check failed.",
    detectedAt: new Date().toISOString(),
  };
  const normalizedPath = normalizeVaultPathForComparison(path);
  const nextRecords = [
    record,
    ...readCorruptedVaults().filter(
      (item) => normalizeVaultPathForComparison(item.path) !== normalizedPath,
    ),
  ].slice(0, 10);
  localStorage.setItem(CORRUPTED_VAULTS_KEY, JSON.stringify(nextRecords));
  return record;
}

export function clearCorruptedVault(path: string): void {
  const normalizedPath = normalizeVaultPathForComparison(path);
  const nextRecords = readCorruptedVaults().filter(
    (record) => normalizeVaultPathForComparison(record.path) !== normalizedPath,
  );
  if (nextRecords.length === 0) {
    localStorage.removeItem(CORRUPTED_VAULTS_KEY);
    return;
  }
  localStorage.setItem(CORRUPTED_VAULTS_KEY, JSON.stringify(nextRecords));
}

function normalizeVaultPathForComparison(path: string): string {
  return path.trim().replace(/[\\/]+$/g, "");
}

function isCorruptedVaultRecord(value: unknown): value is CorruptedVaultRecord {
  if (!value || typeof value !== "object") return false;
  const record = value as Partial<CorruptedVaultRecord>;
  return (
    typeof record.path === "string" &&
    record.path.trim().length > 0 &&
    typeof record.name === "string" &&
    typeof record.message === "string" &&
    typeof record.detectedAt === "string"
  );
}

export async function setVaultFolderName(name: string): Promise<VaultLocation> {
  const folderName = sanitizeVaultFolderName(name);
  const next = await invokeTauri<VaultLocation>("rename_vault_folder", { name: folderName });
  localStorage.setItem(VAULT_NAME_KEY, getVaultFolderName(next.path));
  return next;
}

export async function renameVaultFolderAt(path: string, name: string): Promise<void> {
  const folderName = sanitizeVaultFolderName(name);
  await invokeTauri<void>("rename_vault_folder_at", { path, name: folderName });
  clearCorruptedVault(path);
}

export async function deleteVaultFolderAt(path: string): Promise<void> {
  await invokeTauri<void>("delete_vault_folder_at", { path });
  clearCorruptedVault(path);
}

export async function clearVaultPath(): Promise<VaultLocation> {
  return invokeTauri<VaultLocation>("clear_vault_path");
}

export async function clearVaultPathWithoutInitializing(): Promise<VaultLocation> {
  return invokeTauri<VaultLocation>("clear_vault_path_without_initializing");
}

export async function revealVaultFolder(): Promise<void> {
  return invokeTauri<void>("reveal_vault_folder");
}

export async function chooseVaultFolder(currentPath?: string): Promise<string | null> {
  const dialogModulePath = "@tauri-apps/plugin-dialog";
  const dialog = (await import(
    /* @vite-ignore */ dialogModulePath
  )) as unknown as TauriDialogModule;
  const selected = await dialog.open({
    title: "Choose Starlane Notes vault folder",
    directory: true,
    multiple: false,
    defaultPath: currentPath,
    canCreateDirectories: true,
  });

  if (!selected || Array.isArray(selected)) return null;
  return selected;
}

export async function chooseVaultArchiveExportPath(defaultPath: string): Promise<string | null> {
  const dialogModulePath = "@tauri-apps/plugin-dialog";
  const dialog = (await import(
    /* @vite-ignore */ dialogModulePath
  )) as unknown as TauriDialogModule;

  return dialog.save({
    title: "Export Starlane Notes vault",
    defaultPath,
    canCreateDirectories: true,
    filters: [{ name: "Starlane Vault Archive", extensions: ["starlane-vault", "zip"] }],
  });
}

export async function chooseVaultArchiveImportPath(): Promise<string | null> {
  const dialogModulePath = "@tauri-apps/plugin-dialog";
  const dialog = (await import(
    /* @vite-ignore */ dialogModulePath
  )) as unknown as TauriDialogModule;
  const selected = await dialog.open({
    title: "Import Starlane Notes vault",
    multiple: false,
    filters: [{ name: "Starlane Vault Archive", extensions: ["starlane-vault", "zip"] }],
  });

  if (!selected || Array.isArray(selected)) return null;
  return selected;
}
