import { useEffect, useState } from "react";
import { dataClient } from "@/lib/dataClient";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import type { LocalVaultSyncState } from "@/lib/localVault";

export type SyncState = "synced" | "pending" | "error" | "conflict";

const POLL_MS = 2000;

function mapVaultState(state: LocalVaultSyncState): SyncState {
  if (state === "syncing") return "pending";
  if (state === "error") return "error";
  if (state === "conflict") return "conflict";
  return "synced";
}

export function useSyncState(): SyncState {
  const [state, setState] = useState<SyncState>("synced");

  useEffect(() => {
    let cancelled = false;

    async function refresh() {
      if (getCurrentLaunchMode() !== "local") {
        if (!cancelled) setState("synced");
        return;
      }

      try {
        const vaultState = await dataClient.getSyncState();
        if (!cancelled) setState(mapVaultState(vaultState));
      } catch {
        if (!cancelled) setState("error");
      }
    }

    void refresh();
    const interval = window.setInterval(() => {
      void refresh();
    }, POLL_MS);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, []);

  return state;
}

