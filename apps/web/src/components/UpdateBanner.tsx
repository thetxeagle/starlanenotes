import { useEffect, useRef, useState } from "react";
import { Download, X, RefreshCw } from "lucide-react";
import { isTauri } from "@/lib/api";

interface UpdateInfo {
  version: string;
  body?: string;
}

type UpdateStatus = "idle" | "available" | "downloading" | "ready" | "error";

export function applyUpdateAvailableStatus(current: UpdateStatus): UpdateStatus {
  return current === "downloading" || current === "ready" ? current : "available";
}

interface TauriEvent<T = unknown> {
  payload: T;
}

interface TauriEventModule {
  listen: <T>(event: string, handler: (e: TauriEvent<T>) => void) => Promise<() => void>;
}

export interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

export interface UpdateInstallLock {
  pending: boolean;
}

export function claimUpdateInstall(lock: UpdateInstallLock): boolean {
  if (lock.pending) return false;
  lock.pending = true;
  return true;
}

export function releaseUpdateInstall(lock: UpdateInstallLock): void {
  lock.pending = false;
}

export async function installDesktopUpdate(coreMod: TauriCoreModule | undefined): Promise<void> {
  if (!coreMod) throw new Error("Desktop updater bridge is unavailable.");
  await coreMod.invoke("install_update");
}

export function releaseTauriCore(
  registry: Record<string, unknown>,
  coreMod: TauriCoreModule,
): boolean {
  if (registry.__tauriCore !== coreMod) return false;
  delete registry.__tauriCore;
  return true;
}

export function clampUpdateProgress(progress: number): number {
  return Math.min(100, Math.max(0, Math.round(progress)));
}

export function UpdateDownloadProgress({ progress }: { progress: number }) {
  const boundedProgress = clampUpdateProgress(progress);
  return (
    <>
      <div
        className="bg-surface-raised h-1 flex-1 overflow-hidden rounded-full"
        role="progressbar"
        aria-label="Update download progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={boundedProgress}
      >
        <div
          aria-hidden="true"
          className="bg-primary h-full transition-all duration-200"
          style={{ width: `${boundedProgress}%` }}
        />
      </div>
      <span className="text-text-muted w-8 text-right text-xs">{boundedProgress}%</span>
    </>
  );
}

export function retainUpdateListener(
  unlisten: () => void,
  isDisposed: () => boolean,
  listeners: Array<() => void>,
): boolean {
  if (isDisposed()) {
    unlisten();
    return false;
  }
  listeners.push(unlisten);
  return true;
}

export function UpdateBanner() {
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [progress, setProgress] = useState(0);
  const [dismissed, setDismissed] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");
  const [setupAttempt, setSetupAttempt] = useState(0);
  const installLockRef = useRef<UpdateInstallLock>({ pending: false });

  useEffect(() => {
    if (!isTauri()) return;

    const unlisteners: Array<() => void> = [];
    let disposed = false;
    let registeredCore: TauriCoreModule | null = null;

    async function setup() {
      try {
        const eventModulePath = "@tauri-apps/api/event";
        const coreModulePath = "@tauri-apps/api/core";
        const eventMod = (await import(/* @vite-ignore */ eventModulePath)) as unknown as TauriEventModule;
        const coreMod = (await import(/* @vite-ignore */ coreModulePath)) as unknown as TauriCoreModule;
        if (disposed) return;

        // Background checker emits this when a new version is found
        const unlistenAvailable = await eventMod.listen<UpdateInfo>(
          "update-available",
          (event) => {
            setUpdate(event.payload);
            setStatus(applyUpdateAvailableStatus);
            setErrorMessage("");
            setDismissed(false);
          },
        );
        if (!retainUpdateListener(unlistenAvailable, () => disposed, unlisteners)) return;

        // Download progress (0-100)
        const unlistenProgress = await eventMod.listen<number>(
          "update-progress",
          (event) => {
            setProgress(clampUpdateProgress(event.payload));
          },
        );
        if (!retainUpdateListener(unlistenProgress, () => disposed, unlisteners)) return;

        // Store coreMod ref for use in handlers
        (window as unknown as Record<string, unknown>).__tauriCore = coreMod;
        registeredCore = coreMod;
      } catch {
        if (disposed) return;
        setErrorMessage("Update service could not be started.");
        setStatus("error");
        setDismissed(false);
      }
    }

    setup();
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
      if (registeredCore) {
        releaseTauriCore(window as unknown as Record<string, unknown>, registeredCore);
      }
    };
  }, [setupAttempt]);

  const handleUpdate = async () => {
    if (!claimUpdateInstall(installLockRef.current)) return;
    setStatus("downloading");
    setProgress(0);
    try {
      const coreMod = (window as unknown as Record<string, unknown>).__tauriCore as TauriCoreModule | undefined;
      await installDesktopUpdate(coreMod);
      // App will restart — this line is unreachable on success
      setStatus("ready");
    } catch {
      setErrorMessage("Update could not be installed.");
      setStatus("error");
    } finally {
      releaseUpdateInstall(installLockRef.current);
    }
  };

  if (dismissed || status === "idle") return null;

  return (
    <div className="flex h-8 shrink-0 items-center gap-3 border-b border-border bg-surface px-4">
      <Download size={13} className="shrink-0 text-primary" />

      {status === "available" && (
        <>
          <span className="text-xs text-text">
            Starlane Notes{" "}
            <span className="font-medium text-text">v{update?.version}</span> is available
          </span>
          <button
            type="button"
            onClick={handleUpdate}
            className="rounded-button border border-primary px-2 py-0.5 text-xs text-primary transition-colors duration-100 hover:bg-primary hover:text-white"
          >
            Update now
          </button>
          <div className="flex-1" />
          <button
            type="button"
            onClick={() => setDismissed(true)}
            className="text-text-muted transition-colors duration-100 hover:text-text"
            aria-label="Dismiss"
          >
            <X size={13} />
          </button>
        </>
      )}

      {status === "downloading" && (
        <>
          <span className="text-xs text-text-muted">Downloading update…</span>
          <UpdateDownloadProgress progress={progress} />
        </>
      )}

      {status === "ready" && (
        <>
          <span className="text-xs text-text">Update downloaded</span>
          <button
            type="button"
            onClick={handleUpdate}
            className="flex items-center gap-1 rounded-button border border-primary px-2 py-0.5 text-xs text-primary transition-colors duration-100 hover:bg-primary hover:text-white"
          >
            <RefreshCw size={11} />
            Restart to apply
          </button>
        </>
      )}

      {status === "error" && (
        <>
          <span className="text-danger text-xs" role="alert">
            {errorMessage || "Update could not be installed."}
          </span>
          <button
            type="button"
            onClick={() => {
              if (errorMessage === "Update service could not be started.") {
                setStatus("idle");
                setErrorMessage("");
                setSetupAttempt((attempt) => attempt + 1);
                return;
              }
              void handleUpdate();
            }}
            className="border-danger text-danger hover:bg-danger rounded-button border px-2 py-0.5 text-xs transition-colors duration-100 hover:text-white"
          >
            {errorMessage === "Update service could not be started."
              ? "Retry setup"
              : "Retry update"}
          </button>
          <div className="flex-1" />
          <button
            type="button"
            onClick={() => setDismissed(true)}
            className="text-text-muted hover:text-text transition-colors duration-100"
            aria-label="Dismiss update error"
          >
            <X size={13} />
          </button>
        </>
      )}
    </div>
  );
}
