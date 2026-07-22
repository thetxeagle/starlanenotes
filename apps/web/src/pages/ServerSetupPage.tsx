import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { BrandLogo } from "@/components/BrandLogo";
import { ThemeModeSwitch } from "@/components/ThemeModeSwitch";
import { checkServerStartup } from "@/lib/serverStartup";
import { AUTH_ERROR_ALERT_CLASS } from "./setupShared";

interface ServerSetupPageProps {
  onConnected: (serverUrl?: string) => void;
  onBack?: () => void;
}

export function ServerSetupPage({ onConnected, onBack }: ServerSetupPageProps) {
  const [url, setUrl] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleConnect = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");

    setLoading(true);
    try {
      const result = await checkServerStartup(url);
      localStorage.setItem("server_url", result.serverUrl);
      onConnected(result.serverUrl);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not connect to server.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <main className="app-viewport app-background flex items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-6">
          <BrandLogo size="lg" />
        </div>
        <h1 className="text-text-muted mb-6 text-sm font-normal">Connect to your server</h1>

        {error && (
          <div
            role="alert"
            aria-live="assertive"
            className={`mb-4 ${AUTH_ERROR_ALERT_CLASS}`}
          >
            {error}
          </div>
        )}

        <form onSubmit={handleConnect} className="space-y-3" aria-busy={loading}>
          <div>
            <label htmlFor="server-url" className="text-text-muted mb-1 block text-sm">
              Server URL
            </label>
            <Input
              id="server-url"
              type="url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://notes.example.com"
              autoComplete="url"
              autoFocus
              required
              aria-describedby="server-url-help"
              disabled={loading}
            />
            <p id="server-url-help" className="text-text-muted mt-1 text-xs">
              Include protocol, for example https://notes.example.com.
            </p>
          </div>

          <Button type="submit" className="w-full" disabled={loading}>
            {loading ? "Connecting..." : "Connect"}
          </Button>

          {onBack && (
            <Button
              type="button"
              variant="secondary"
              className="w-full"
              onClick={onBack}
              disabled={loading}
            >
              Back
            </Button>
          )}

          <div className="flex justify-center pt-1">
            <ThemeModeSwitch />
          </div>
        </form>
      </div>
    </main>
  );
}
