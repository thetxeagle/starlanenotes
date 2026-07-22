import { useSyncState, type SyncState } from "@/hooks/useSyncState";
import { Link } from "react-router-dom";

export const SYNC_STATUS_CONFIG: Record<SyncState, { color: string; label: string }> = {
  synced: { color: "bg-success", label: "Synced" },
  pending: { color: "bg-warning", label: "Syncing..." },
  conflict: { color: "bg-accent", label: "Sync conflict" },
  error: { color: "bg-danger", label: "Sync error" },
};

export function SyncStatus() {
  const state = useSyncState();

  const { color, label } = SYNC_STATUS_CONFIG[state];

  const status = (
    <div className="flex items-center gap-1.5" title={label}>
      <span className={`h-1.5 w-1.5 rounded-full ${color}`} />
      <span className="text-[10px] text-text-muted">{label}</span>
    </div>
  );

  if (state === "conflict") {
    return (
      <Link
        to="/admin/storage"
        aria-label="Review sync conflict in Storage settings"
        className="rounded-button focus-ring"
      >
        {status}
      </Link>
    );
  }

  return status;
}
