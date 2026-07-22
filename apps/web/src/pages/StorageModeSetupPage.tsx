import { useEffect, useState, type ReactNode } from "react";
import { ChevronLeft, Cloud, Folder, HardDrive, Server } from "lucide-react";
import { BrandLogo } from "@/components/BrandLogo";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ThemeModeSwitch } from "@/components/ThemeModeSwitch";
import { isTauri } from "@/lib/api";
import { checkServerStartup, type ServerAuthConfigPreview } from "@/lib/serverStartup";
import {
  getCorruptedVaultRecord,
  listVaultFolders,
  type CorruptedVaultRecord,
  type VaultFolderSummary,
} from "@/lib/tauriVault";

interface StorageModeSetupPageProps {
  onChooseLocal: (vaultName: string, vaultPath?: string) => Promise<void> | void;
  onChooseServer: (serverUrl?: string) => void;
}

type SetupView = "home" | "create" | "open";

export function StorageModeSetupPage({ onChooseLocal, onChooseServer }: StorageModeSetupPageProps) {
  const [view, setView] = useState<SetupView>("home");
  const [vaultName, setVaultName] = useState("Personal Space");
  const [serverUrl, setServerUrl] = useState("");
  const [serverAuthConfig, setServerAuthConfig] = useState<ServerAuthConfigPreview | null>(null);
  const [vaultFolders, setVaultFolders] = useState<VaultFolderSummary[]>([]);
  const [corruptedVaults, setCorruptedVaults] = useState<Record<string, CorruptedVaultRecord>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const canChooseStandaloneVault = isTauri();

  useEffect(() => {
    if (!canChooseStandaloneVault) return;
    let cancelled = false;
    listVaultFolders()
      .then((folders) => {
        if (!cancelled) {
          setVaultFolders(folders);
          setCorruptedVaults(
            Object.fromEntries(
              folders
                .map((folder) => [folder.path, getCorruptedVaultRecord(folder.path)] as const)
                .filter((entry): entry is readonly [string, CorruptedVaultRecord] =>
                  Boolean(entry[1]),
                ),
            ),
          );
        }
      })
      .catch(() => {
        if (!cancelled) setVaultFolders([]);
      });
    return () => {
      cancelled = true;
    };
  }, [canChooseStandaloneVault]);

  const chooseLocal = async (name = vaultName, path?: string) => {
    setLoading(true);
    setError(null);
    try {
      await onChooseLocal(name, path);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to prepare that vault.");
    } finally {
      setLoading(false);
    }
  };

  const connectServer = async () => {
    setError(null);
    setServerAuthConfig(null);

    setLoading(true);
    try {
      const result = await checkServerStartup(serverUrl);
      setServerAuthConfig(result.config);
      localStorage.setItem("server_url", result.serverUrl);
      onChooseServer(result.serverUrl);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not connect to that server.");
    } finally {
      setLoading(false);
    }
  };

  const authModeLabel = serverAuthConfig
    ? serverAuthConfig.oidc_enabled
      ? `${serverAuthConfig.oidc_display_name || "SSO"} login detected`
      : serverAuthConfig.google_enabled || serverAuthConfig.github_enabled
        ? "OAuth login detected"
        : "Standard login detected"
    : null;

  return (
    <main className="app-viewport app-background flex items-center justify-center overflow-x-hidden px-4 py-6">
      <div className="w-full min-w-0 max-w-xl">
        <div className="mb-6">
          <BrandLogo size="lg" />
        </div>

        {view !== "home" && (
          <button
            type="button"
            onClick={() => {
              setView("home");
              setError(null);
            }}
            className="text-text-muted hover:text-text mb-4 inline-flex items-center gap-1 text-sm transition-colors duration-100"
          >
            <ChevronLeft size={15} />
            Back
          </button>
        )}

        <h1 className="text-text mb-2 text-lg font-semibold">
          {view === "home"
            ? "Choose your vault"
            : view === "create"
              ? "Create a local vault"
                : "Open an existing vault"}
        </h1>
        <p className="text-text-muted mb-5 text-sm">
          {view === "home"
            ? "Create a local vault, open an existing vault, or connect to a Starlane Notes server."
            : view === "create"
              ? "Create a visible local vault folder on this device."
              : "Select an app-visible vault from local storage, iCloud, or a downloaded server copy."}
        </p>

        {view === "home" && (
          <div className="grid min-w-0 grid-cols-1 gap-3">
            {canChooseStandaloneVault && (
              <SetupActionCard
                icon={<HardDrive size={18} />}
                title="Create local vault"
                description="Start fresh with a named vault folder stored on this device."
                action="Create vault"
                onClick={() => setView("create")}
              />
            )}
            <SetupActionCard
              icon={<Folder size={18} />}
              title="Open existing vault"
              description="Browse visible local, iCloud, or downloaded server vaults."
              action="Open vault"
              onClick={() => setView("open")}
              disabled={!canChooseStandaloneVault}
            />
            <div className="rounded-card border-border bg-surface min-w-0 border p-4 text-left">
              <div className="rounded-button border-border text-primary mb-3 flex h-9 w-9 items-center justify-center border">
                <Server size={18} />
              </div>
              <p className="text-text text-sm font-medium">Connect server</p>
              <p className="text-text-muted mt-1 text-xs leading-5">
                Enter a server URL to detect standard login, OAuth, or SSO.
              </p>
              <Input
                value={serverUrl}
                onChange={(event) => setServerUrl(event.target.value)}
                className="mt-3 h-8 text-sm"
                placeholder="https://notes.example.com"
                type="url"
                inputMode="url"
              />
              <Button
                className="mt-3 w-full min-w-0 overflow-hidden px-2"
                type="button"
                size="sm"
                onClick={() => void connectServer()}
                disabled={loading || !serverUrl.trim()}
              >
                <span className="min-w-0 truncate">
                  {loading ? "Checking..." : "Connect and detect login"}
                </span>
              </Button>
              {authModeLabel && <p className="text-success mt-2 text-xs">{authModeLabel}</p>}
            </div>
          </div>
        )}

        {view === "create" && canChooseStandaloneVault && (
          <div className="rounded-card border-border bg-surface min-w-0 border p-4 text-left">
            <div className="rounded-button border-border text-primary mb-3 flex h-9 w-9 items-center justify-center border">
              <HardDrive size={18} />
            </div>
            <p className="text-text text-sm font-medium">New local vault</p>
            <p className="text-text-muted mt-1 text-xs leading-5">
              Store notes locally in a named vault folder. The last selected vault opens
              automatically after it passes a startup check.
            </p>
            <label htmlFor="local-vault-name" className="sr-only">
              Vault folder name
            </label>
            <Input
              id="local-vault-name"
              value={vaultName}
              onChange={(event) => setVaultName(event.target.value)}
              className="mt-3 h-8 text-sm"
              placeholder="Vault folder name"
            />
            <Button
              className="mt-3 w-full min-w-0 overflow-hidden px-2"
              type="button"
              size="sm"
              onClick={() => void chooseLocal()}
              disabled={loading || !vaultName.trim()}
            >
              <span className="min-w-0 truncate">
                {loading ? "Preparing..." : "Create / Use Vault"}
              </span>
            </Button>
            {error && <p className="text-danger mt-2 text-xs">{error}</p>}
          </div>
        )}

        {view === "open" && canChooseStandaloneVault && (
          <div className="grid gap-3">
            <div className="rounded-card border-border bg-surface min-w-0 border p-4 text-left">
              <div className="mb-3 grid grid-cols-3 gap-2">
                <StorageSourceButton icon={<HardDrive size={15} />} label="Local" active />
                <StorageSourceButton icon={<Cloud size={15} />} label="iCloud" />
                <StorageSourceButton icon={<Server size={15} />} label="Server" />
              </div>
              <p className="text-text text-sm font-medium">Visible vaults</p>
              <p className="text-text-muted mt-1 text-xs leading-5">
                Server vaults are downloaded to this phone before opening when supported.
              </p>
              <div className="rounded-card border-border bg-surface mt-3 border">
                {vaultFolders.length === 0 ? (
                  <p className="text-text-muted px-3 py-3 text-sm">
                    No visible vault folders found yet.
                  </p>
                ) : (
                  <div className="max-h-64 overflow-y-auto">
                    {vaultFolders.map((folder) => (
                      <button
                        key={folder.path}
                        type="button"
                        disabled={loading}
                        onClick={() => void chooseLocal(folder.name, folder.path)}
                        className="border-border/70 active:bg-surface-raised flex w-full min-w-0 items-center gap-2 border-b px-3 py-3 text-left text-xs last:border-b-0"
                      >
                        <Folder size={14} className="text-text-muted shrink-0" />
                        <span className="min-w-0 flex-1">
                          <span className="flex min-w-0 items-center gap-2">
                            <span className="text-text block truncate">{folder.name}</span>
                            {corruptedVaults[folder.path] && (
                              <span className="text-warning border-warning/40 rounded-full border px-1.5 py-0.5 text-[10px] uppercase tracking-wide">
                                Needs repair
                              </span>
                            )}
                          </span>
                          <span className="text-text-muted block truncate text-[11px]">
                            {folder.path}
                          </span>
                          {corruptedVaults[folder.path] && (
                            <span className="text-warning mt-1 block truncate text-[11px]">
                              {corruptedVaults[folder.path].message}
                            </span>
                          )}
                        </span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}

        {error && <p className="text-danger mt-3 text-sm">{error}</p>}

        <div className="mt-5 flex justify-center">
          <ThemeModeSwitch />
        </div>
      </div>
    </main>
  );
}

function SetupActionCard({
  icon,
  title,
  description,
  action,
  disabled = false,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  action: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <div className="rounded-card border-border bg-surface min-w-0 border p-4 text-left">
      <div className="rounded-button border-border text-primary mb-3 flex h-9 w-9 items-center justify-center border">
        {icon}
      </div>
      <p className="text-text text-sm font-medium">{title}</p>
      <p className="text-text-muted mt-1 text-xs leading-5">{description}</p>
      <Button
        className="mt-3 w-full min-w-0 overflow-hidden px-2"
        type="button"
        size="sm"
        variant={disabled ? "secondary" : "primary"}
        disabled={disabled}
        onClick={onClick}
      >
        <span className="min-w-0 truncate">{action}</span>
      </Button>
    </div>
  );
}

function StorageSourceButton({
  icon,
  label,
  active = false,
}: {
  icon: ReactNode;
  label: string;
  active?: boolean;
}) {
  return (
    <button
      type="button"
      disabled={!active}
      className={`rounded-button border px-2 py-2 text-xs transition-colors duration-100 ${
        active
          ? "border-primary text-primary bg-surface-raised"
          : "border-border text-text-muted bg-surface opacity-60"
      }`}
    >
      <span className="flex items-center justify-center gap-1">
        {icon}
        {label}
      </span>
    </button>
  );
}
