import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useAuth } from "@/hooks/useAuth";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AUTH_ERROR_ALERT_CLASS } from "./setupShared";

export const PUBLIC_SIGNUPS_DISABLED_COPY =
  "Public signups are disabled for this server. Ask an administrator for an invite or sign in with an existing account.";

export const REGISTER_PUBLIC_SIGNUPS_NOTICE_CLASS =
  "rounded-card border border-border bg-surface p-4 text-sm text-text-muted shadow-card";

export function RegisterPage() {
  const navigate = useNavigate();
  const { register, config } = useAuth();
  const [email, setEmail] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const publicSignupsDisabled =
    config !== null && config.setup_completed && !config.allow_public_signups;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");

    if (publicSignupsDisabled) {
      setError(PUBLIC_SIGNUPS_DISABLED_COPY);
      return;
    }

    if (password !== confirmPassword) {
      setError("Passwords do not match");
      return;
    }

    if (password.length < 8) {
      setError("Password must be at least 8 characters");
      return;
    }

    setLoading(true);

    try {
      await register(email, displayName, password);
      navigate("/");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Registration failed");
    } finally {
      setLoading(false);
    }
  };

  return (
    <main className="app-viewport app-background flex items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <h1 className="mb-8 text-xl font-semibold text-text">
          {publicSignupsDisabled ? "Account creation is invite only" : "Create your account"}
        </h1>

        {error && (
          <div
            role="alert"
            aria-live="assertive"
            className={`mb-4 ${AUTH_ERROR_ALERT_CLASS}`}
          >
            {error}
          </div>
        )}

        {publicSignupsDisabled ? (
          <div className={REGISTER_PUBLIC_SIGNUPS_NOTICE_CLASS}>
            <p>{PUBLIC_SIGNUPS_DISABLED_COPY}</p>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-3" aria-busy={loading}>
            <div>
              <label htmlFor="email" className="mb-1 block text-sm text-text-muted">
                Email
              </label>
              <Input
                id="email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                autoComplete="email"
                disabled={loading}
                required
                autoFocus
              />
            </div>

            <div>
              <label htmlFor="name" className="mb-1 block text-sm text-text-muted">
                Display name
              </label>
              <Input
                id="name"
                type="text"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
                autoComplete="name"
                disabled={loading}
                required
              />
            </div>

            <div>
              <label htmlFor="password" className="mb-1 block text-sm text-text-muted">
                Password
              </label>
              <Input
                id="password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="new-password"
                disabled={loading}
                required
                minLength={8}
              />
            </div>

            <div>
              <label htmlFor="confirm" className="mb-1 block text-sm text-text-muted">
                Confirm password
              </label>
              <Input
                id="confirm"
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                autoComplete="new-password"
                disabled={loading}
                required
              />
            </div>

            <Button type="submit" className="w-full" disabled={loading}>
              {loading ? "Creating account..." : "Create account"}
            </Button>
          </form>
        )}

        <p className="mt-4 text-center text-sm text-text-muted">
          Already have an account?{" "}
          <Link to="/login" className="font-medium text-primary underline underline-offset-2 hover:opacity-90">
            Sign in
          </Link>
        </p>
      </div>
    </main>
  );
}
