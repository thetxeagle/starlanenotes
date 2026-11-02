import { KeyRound, Plus, RefreshCw, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  Button,
  RESPONSIVE_TOUCH_ACTION_CLASS,
  RESPONSIVE_TOUCH_ICON_ACTION_CLASS,
} from "@/components/ui/button";

export interface ApiKeyDisplayItem {
  id: string;
  name: string;
  key_prefix: string;
  is_service_account: boolean;
  scopes: string[];
  last_used_at?: string;
  expires_at?: string;
  created_at: string;
}

interface ApiKeyManagementCardProps {
  keys: ApiKeyDisplayItem[];
  localMode: boolean;
  loading?: boolean;
  onCreate: () => void;
  onRefresh: () => void;
  onRevoke: (key: ApiKeyDisplayItem) => void;
}

function pluralizeKey(count: number): string {
  return `${count} ${count === 1 ? "key" : "keys"}`;
}

export function isApiKeyExpired(key: ApiKeyDisplayItem, now = Date.now()): boolean {
  if (!key.expires_at) return false;
  const expiresAt = Date.parse(key.expires_at);
  return Number.isFinite(expiresAt) && expiresAt <= now;
}

export function formatApiKeyActivity(value?: string, now = Date.now()): string {
  if (!value) return "Never";
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return "Unknown";
  const elapsedMinutes = Math.max(0, Math.floor((now - timestamp) / 60_000));
  if (elapsedMinutes < 1) return "Just now";
  if (elapsedMinutes < 60) return `${elapsedMinutes}m ago`;
  const elapsedHours = Math.floor(elapsedMinutes / 60);
  if (elapsedHours < 24) return `${elapsedHours}h ago`;
  const elapsedDays = Math.floor(elapsedHours / 24);
  if (elapsedDays < 30) return `${elapsedDays}d ago`;
  return new Date(timestamp).toLocaleDateString();
}

export function formatApiKeyDate(value?: string): string {
  if (!value) return "No expiry";
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return "Unknown";
  return new Date(timestamp).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function AdminApiKeyManagementCard({
  keys,
  localMode,
  loading = false,
  onCreate,
  onRefresh,
  onRevoke,
}: ApiKeyManagementCardProps) {
  const activeCount = keys.filter((key) => !isApiKeyExpired(key)).length;

  return (
    <section
      className="rounded-card border-border bg-surface overflow-hidden border"
      aria-labelledby="api-keys-card-title"
    >
      <header className="border-border flex items-start justify-between gap-4 border-b px-4 py-3 sm:px-5">
        <div className="min-w-0">
          <h1 id="api-keys-card-title" className="text-text text-base font-medium">
            API Keys
          </h1>
          <p className="text-text-muted mt-0.5 text-xs">
            Create and manage keys for API access.
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          className={RESPONSIVE_TOUCH_ACTION_CLASS}
          disabled={localMode}
          onClick={onCreate}
        >
          <Plus size={14} aria-hidden="true" />
          Create key
        </Button>
      </header>

      <div className="flex items-center justify-between gap-3 px-4 py-3 sm:px-5">
        <p className="text-text-muted flex items-center gap-2 text-xs">
          <KeyRound size={14} aria-hidden="true" />
          {pluralizeKey(activeCount)} active
        </p>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className={RESPONSIVE_TOUCH_ICON_ACTION_CLASS}
          disabled={localMode || loading}
          aria-label="Refresh API keys"
          title="Refresh API keys"
          onClick={onRefresh}
        >
          <RefreshCw size={14} aria-hidden="true" />
        </Button>
      </div>

      {localMode ? (
        <p className="text-text-muted px-4 pb-4 text-sm sm:px-5">
          API keys are available only for server-linked workspaces.
        </p>
      ) : keys.length === 0 ? (
        <div className="px-4 pb-5 sm:px-5">
          <div className="rounded-input border-border bg-bg border px-3 py-4 text-center">
            <p className="text-text text-sm">No API keys yet</p>
            <p className="text-text-muted mt-1 text-xs">
              Create a key for scripts, integrations, or service automation.
            </p>
          </div>
        </div>
      ) : (
        <div className="divide-y divide-border">
          {keys.map((key) => {
            const expired = isApiKeyExpired(key);
            return (
              <article key={key.id} className="px-4 py-3 sm:px-5">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <h2 className="text-text truncate text-sm font-medium">{key.name}</h2>
                      {key.is_service_account && <Badge variant="outline">service</Badge>}
                      {key.scopes.map((scope) => (
                        <Badge key={scope}>{scope}</Badge>
                      ))}
                      {expired && <Badge variant="danger">expired</Badge>}
                    </div>
                    <div className="text-text-muted mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                      <code className="rounded-input bg-surface-raised px-2 py-1 font-mono text-[11px]">
                        {key.key_prefix}••••••••
                      </code>
                      <span>Last used {formatApiKeyActivity(key.last_used_at)}</span>
                      <span aria-hidden="true">·</span>
                      <span>Created {formatApiKeyDate(key.created_at)}</span>
                      <span aria-hidden="true">·</span>
                      <span>
                        {key.expires_at
                          ? `${expired ? "Expired" : "Expires"} ${formatApiKeyDate(key.expires_at)}`
                          : "No expiry"}
                      </span>
                    </div>
                  </div>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    className={`${RESPONSIVE_TOUCH_ACTION_CLASS} shrink-0 gap-1.5 text-danger hover:text-danger`}
                    disabled={localMode}
                    onClick={() => onRevoke(key)}
                  >
                    <Trash2 size={13} aria-hidden="true" />
                    Revoke
                  </Button>
                </div>
              </article>
            );
          })}
        </div>
      )}

      <footer className="border-border text-text-muted border-t px-4 py-3 text-xs leading-5 sm:px-5">
        API keys can access your account through the API. Full values appear only once when created,
        so keep them secret and rotate them regularly.
      </footer>
    </section>
  );
}
