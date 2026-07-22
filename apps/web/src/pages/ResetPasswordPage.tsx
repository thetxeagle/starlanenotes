import { useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { BrandLogo } from "@/components/BrandLogo";
import { AUTH_ERROR_ALERT_CLASS } from "./setupShared";

type ResetMode = "request" | "confirm";

export const RESET_PASSWORD_SUCCESS_ALERT_CLASS =
  "rounded-input border border-success/30 bg-success/10 px-3 py-2 text-sm text-success";

export function ResetPasswordPage() {
  const [searchParams] = useSearchParams();
  const initialToken = searchParams.get("token") ?? "";
  const mode: ResetMode = initialToken ? "confirm" : "request";
  const [email, setEmail] = useState("");
  const [token, setToken] = useState(initialToken);
  const [password, setPassword] = useState("");
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const title = mode === "confirm" ? "Set a new password" : "Reset your password";
  const submitLabel = useMemo(() => {
    if (loading) return mode === "confirm" ? "Saving..." : "Sending...";
    return mode === "confirm" ? "Save new password" : "Send reset link";
  }, [loading, mode]);

  const requestReset = async () => {
    const response = await api.fetch<{ message: string }>("/auth/password-reset/request", {
      method: "POST",
      body: { email },
    });
    setMessage(response.message);
  };

  const confirmReset = async () => {
    await api.fetch("/auth/password-reset/confirm", {
      method: "POST",
      body: { token, password },
    });
    setMessage("Password updated. You can sign in with your new password.");
    setPassword("");
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    setMessage("");
    setLoading(true);
    try {
      if (mode === "confirm") {
        await confirmReset();
      } else {
        await requestReset();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to reset password");
    } finally {
      setLoading(false);
    }
  };

  return (
    <main className="app-viewport app-background flex items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-8">
          <BrandLogo size="lg" />
          <h1 className="mt-6 text-xl font-semibold text-text">{title}</h1>
        </div>

        {message && (
          <div
            role="status"
            aria-live="polite"
            className={`mb-4 ${RESET_PASSWORD_SUCCESS_ALERT_CLASS}`}
          >
            {message}
          </div>
        )}
        {error && (
          <div
            role="alert"
            aria-live="assertive"
            className={`mb-4 ${AUTH_ERROR_ALERT_CLASS}`}
          >
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-3" aria-busy={loading}>
          {mode === "request" ? (
            <div>
              <label htmlFor="reset-email" className="mb-1 block text-sm text-text-muted">
                Email
              </label>
              <Input
                id="reset-email"
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                autoComplete="email"
                disabled={loading}
                required
                autoFocus
              />
            </div>
          ) : (
            <>
              <div>
                <label htmlFor="reset-token" className="mb-1 block text-sm text-text-muted">
                  Reset token
                </label>
                <Input
                  id="reset-token"
                  value={token}
                  onChange={(event) => setToken(event.target.value)}
                  autoComplete="one-time-code"
                  disabled={loading || Boolean(initialToken)}
                  required
                />
              </div>
              <div>
                <label htmlFor="new-password" className="mb-1 block text-sm text-text-muted">
                  New password
                </label>
                <Input
                  id="new-password"
                  type="password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  autoComplete="new-password"
                  disabled={loading}
                  minLength={8}
                  maxLength={128}
                  required
                  autoFocus
                />
              </div>
            </>
          )}

          <Button type="submit" className="w-full" disabled={loading}>
            {submitLabel}
          </Button>
        </form>

        <p className="mt-4 text-center text-sm text-text-muted">
          <Link to="/login" className="font-medium text-primary underline underline-offset-2 hover:opacity-90">
            Back to sign in
          </Link>
        </p>
      </div>
    </main>
  );
}
