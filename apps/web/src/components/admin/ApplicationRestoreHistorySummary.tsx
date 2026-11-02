import { sqliteApplicationHistoryScope, type ApplicationRestoreHistory } from "@/lib/applicationRestoreHistory";
import { storageQuantity } from "@/lib/adminStorageHealth";
export function ApplicationRestoreHistorySummary({ value }: { value?: ApplicationRestoreHistory }) {
  if (!value || value.status === "unavailable") return <>Unavailable</>;
  return (
    <div className="space-y-1">
      <p>
        Completed{" "}
        <time dateTime={value.completed_at}>{new Date(value.completed_at).toLocaleString()}</time>
      </p>
      <p className="text-text-muted text-xs">
        Scope: {value.scope === sqliteApplicationHistoryScope ? "SQLite" : "PostgreSQL"} application test with preserved versioned attachments.
      </p>
      <p className="text-text-muted text-xs">
        {value.pages.toLocaleString()} {value.pages === 1 ? "page" : "pages"} ·{" "}
        {value.drafts.toLocaleString()} {value.drafts === 1 ? "draft" : "drafts"} ·{" "}
        {value.sampled_humans.toLocaleString()} sampled people
      </p>
      <p className="text-text-muted text-xs">
        {value.registered_assets.toLocaleString()} versioned{" "}
        {value.registered_assets === 1 ? "asset" : "assets"} ·{" "}
        {value.legacy_assets.toLocaleString()} legacy{" "}
        {value.legacy_assets === 1 ? "asset" : "assets"} ·{" "}
        {storageQuantity(value.exact_binary_bytes, true)} compared
      </p>
      <p className="text-text-muted text-xs">
        Historical result at the recorded time. Current edits and complete restore verification are
        outside this test.
      </p>
    </div>
  );
}
