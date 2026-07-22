import { useEffect, useState } from "react";
import { Download, X, RefreshCw } from "lucide-react";
import { isTauri } from "@/lib/api";

interface UpdateInfo {
  version: string;
  body?: string;
}

type UpdateStatus = "idle" | "available" | "downloading" | "ready";

interface TauriEvent<T = unknown> {
  payload: T;
}

interface TauriEventModule {
  listen: <T>(event: string, handler: (e: TauriEvent<T>) => void) => Promise<() => void>;
}

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

export function UpdateBanner() {
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [progress, setProgress] = useState(0);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;

    const unlisteners: Array<() => void> = [];

    async function setup() {
      try {
        const eventModulePath = "@tauri-apps/api/event";
        const coreModulePath = "@tauri-apps/api/core";
        const eventMod = (await import(/* @vite-ignore */ eventModulePath)) as unknown as TauriEventModule;
        const coreMod = (await import(/* @vite-ignore */ coreModulePath)) as unknown as TauriCoreModule;

        // Background checker emits this when a new version is found
        const unlistenAvailable = await eventMod.listen<UpdateInfo>(
          "update-available",
          (event) => {
            setUpdate(event.payload);
            setStatus("available");
            setDismissed(false);
          },
        );
        unlisteners.push(unlistenAvailable);

        // Download progress (0-100)
        const unlistenProgress = await eventMod.listen<number>(
          "update-progress",
          (event) => {
            setProgress(event.payload);
          },
        );
        unlisteners.push(unlistenProgress);

        // Store coreMod ref for use in handlers
        (window as unknown as Record<string, unknown>).__tauriCore = coreMod;
      } catch {
        // Not in Tauri
      }
    }

    setup();
    return () => {
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  const handleUpdate = async () => {
    setStatus("downloading");
    setProgress(0);
    try {
      const coreMod = (window as unknown as Record<string, unknown>).__tauriCore as TauriCoreModule | undefined;
      if (coreMod) {
        await coreMod.invoke("install_update");
        // App will restart — this line is unreachable on success
        setStatus("ready");
      }
    } catch {
      // If install fails, go back to available
      setStatus("available");
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
            onClick={handleUpdate}
            className="rounded-button border border-primary px-2 py-0.5 text-xs text-primary transition-colors duration-100 hover:bg-primary hover:text-white"
          >
            Update now
          </button>
          <div className="flex-1" />
          <button
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
          <div className="h-1 flex-1 overflow-hidden rounded-full bg-surface-raised">
            <div
              className="h-full bg-primary transition-all duration-200"
              style={{ width: `${progress}%` }}
            />
          </div>
          <span className="w-8 text-right text-xs text-text-muted">{progress}%</span>
        </>
      )}

      {status === "ready" && (
        <>
          <span className="text-xs text-text">Update downloaded</span>
          <button
            onClick={handleUpdate}
            className="flex items-center gap-1 rounded-button border border-primary px-2 py-0.5 text-xs text-primary transition-colors duration-100 hover:bg-primary hover:text-white"
          >
            <RefreshCw size={11} />
            Restart to apply
          </button>
        </>
      )}
    </div>
  );
}
