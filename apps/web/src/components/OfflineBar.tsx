import { useSyncExternalStore } from "react";
import { api } from "@/lib/api";
import { useOnlineStatus } from "@/hooks/useOnlineStatus";
import {
  getOfflineServerWorkspaceStatus,
  OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT,
  serverCacheScopeForSession,
} from "@/lib/offlineServerCache";
import { getCurrentLaunchMode, type AppLaunchMode } from "@/lib/launchMode";

interface OfflineBarProps {
  zoomPercent?: number;
}

function readLinkedServerWorkspaceStatus() {
  if (getCurrentLaunchMode() !== "server") return null;
  try {
    const serverUrl = globalThis.localStorage?.getItem("server_url");
    if (!serverUrl) return null;
    return getOfflineServerWorkspaceStatus(`${serverUrl.replace(/\/+$/, "")}/api/v1`, serverCacheScopeForSession(api.getToken(), api.getRefreshToken()));
  } catch {
    return null;
  }
}

function subscribeToWorkspaceStatus(listener: () => void) {
  globalThis.addEventListener?.("storage", listener);
  globalThis.addEventListener?.("online", listener);
  globalThis.addEventListener?.("offline", listener);
  globalThis.addEventListener?.(OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT, listener);
  return () => {
    globalThis.removeEventListener?.("storage", listener);
    globalThis.removeEventListener?.("online", listener);
    globalThis.removeEventListener?.("offline", listener);
    globalThis.removeEventListener?.(OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT, listener);
  };
}

export function shouldShowOfflineMessage(online: boolean, launchMode: AppLaunchMode): boolean {
  return !online && launchMode === "server";
}

export function OfflineBar({ zoomPercent }: OfflineBarProps) {
  const online = useOnlineStatus();
  const workspace = useSyncExternalStore(
    subscribeToWorkspaceStatus,
    readLinkedServerWorkspaceStatus,
    () => null,
  );
  const showZoom = zoomPercent !== undefined && zoomPercent !== 100;
  const showOffline = shouldShowOfflineMessage(online, getCurrentLaunchMode());

  if (!showOffline && !showZoom) return null;

  return (
    <div className="starlane-status-bar flex h-6 shrink-0 items-center border-t border-border bg-surface-raised px-3 text-xs text-text-muted">
      {showOffline && (
        <span>
          {workspace?.ready
            ? `Offline — downloaded workspace ready${workspace.queuedMutationCount ? ` · ${workspace.queuedMutationCount} change${workspace.queuedMutationCount === 1 ? "" : "s"} queued` : ""}`
            : "Working offline — changes will sync when reconnected"}
        </span>
      )}
      {showZoom && (
        <span className="ml-auto">
          Zoom: {zoomPercent}%
        </span>
      )}
    </div>
  );
}
