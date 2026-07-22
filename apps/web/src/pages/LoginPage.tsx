import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useAuth } from "@/hooks/useAuth";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { BrandLogo } from "@/components/BrandLogo";
import { AUTH_ERROR_ALERT_CLASS } from "./setupShared";
import { isPasskeySupported } from "@/lib/passkeys";
import { isApiError, isTauri } from "@/lib/api";
import { beginNativeAuthHandoff } from "@/lib/nativeAuthHandoff";

export function canSubmitPasskeyLogin(
  email: string,
  passkeysAvailable: boolean,
  loading: boolean,
): boolean {
  return !getPasskeyLoginDisabledReason(email, passkeysAvailable, loading);
}

export function getPasskeyLoginDisabledReason(
  email: string,
  passkeysAvailable: boolean,
  loading: boolean,
): string | null {
  if (!passkeysAvailable) return "Passkey sign-in is unavailable in this browser or webview.";
  if (loading) return "Passkey sign-in is paused while another sign-in is in progress.";
  if (!email.trim()) return "Enter your email first so Starlane can find your saved passkey.";
  return null;
}

export function getPasswordLoginDisabledReason(
  needsTwoFactor: boolean,
  twoFactorCode: string,
  loading: boolean,
): string | null {
  if (loading) return "Sign-in is already in progress.";
  if (needsTwoFactor) {
    const trimmedCode = twoFactorCode.trim();
    if (!trimmedCode) return "Enter an authenticator or recovery code to continue.";
    if (!isLoginTwoFactorCodeFormat(trimmedCode)) {
      return "Enter a 6-digit authenticator code or recovery code like 1234-5678.";
    }
  }
  return null;
}

export function isLoginTwoFactorCodeFormat(twoFactorCode: string): boolean {
  return (
    /^\d{6}$/u.test(twoFactorCode) ||
    /^\d{8}$/u.test(twoFactorCode) ||
    /^\d{4}-\d{4}$/u.test(twoFactorCode)
  );
}

export function getLoginTwoFactorCodeForSubmit(
  needsTwoFactor: boolean,
  twoFactorCode: string,
): string | undefined {
  if (!needsTwoFactor) return undefined;
  return twoFactorCode.trim();
}

export function shouldResetTwoFactorPrompt(currentValue: string, nextValue: string): boolean {
  return currentValue !== nextValue;
}

export function isTwoFactorRequiredError(error: unknown): boolean {
  if (isApiError(error) && error.code === "2fa_required") return true;

  const message = error instanceof Error ? error.message : String(error);
  const normalized = message.toLowerCase();
  return (
    normalized.includes("2fa_required") ||
    normalized.includes("2fa required") ||
    normalized.includes("2fa code required") ||
    (normalized.includes("two-factor") && normalized.includes("required"))
  );
}

export const LOGIN_TWO_FACTOR_REQUIRED_NOTICE_COPY =
  "This account requires two-factor authentication. Enter an authenticator or recovery code to continue.";

export const LOGIN_TWO_FACTOR_REQUIRED_NOTICE_CLASS =
  "rounded-input border border-border bg-surface-raised px-3 py-2 text-sm text-text-muted";

export function TwoFactorRequiredNotice() {
  return (
    <div role="status" className={LOGIN_TWO_FACTOR_REQUIRED_NOTICE_CLASS}>
      {LOGIN_TWO_FACTOR_REQUIRED_NOTICE_COPY}
    </div>
  );
}

export const LOGIN_PASSKEY_UNAVAILABLE_NOTICE_COPY =
  "Passkey sign-in is unavailable in this browser or webview. Use email and password instead.";

export const LOGIN_PASSKEY_UNAVAILABLE_NOTICE_CLASS =
  "mt-2 rounded-input border border-border bg-surface-raised px-3 py-2 text-center text-xs text-text-muted";

export function PasskeyUnavailableNotice() {
  return (
    <p role="status" className={LOGIN_PASSKEY_UNAVAILABLE_NOTICE_CLASS}>
      {LOGIN_PASSKEY_UNAVAILABLE_NOTICE_COPY}
    </p>
  );
}

export interface TwoFactorLoginCodeInputProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  autoFocus?: boolean;
}

export const LOGIN_TWO_FACTOR_CODE_INPUT_ID = "totp";
export const LOGIN_TWO_FACTOR_CODE_HELP_ID = "totp-help";
export const LOGIN_TWO_FACTOR_CODE_PLACEHOLDER = "123456 or 1234-5678";
export const LOGIN_TWO_FACTOR_CODE_MAX_LENGTH = 9;
export const LOGIN_TWO_FACTOR_CODE_HELP_COPY =
  "Use a 6-digit authenticator code or one unused recovery code.";
export const LOGIN_TWO_FACTOR_CODE_HELP_CLASS = "mt-1 text-xs text-text-muted";

export function TwoFactorLoginCodeInput({
  value,
  onChange,
  disabled = false,
  autoFocus = false,
}: TwoFactorLoginCodeInputProps) {
  return (
    <div>
      <label htmlFor={LOGIN_TWO_FACTOR_CODE_INPUT_ID} className="mb-1 block text-sm text-text-muted">
        Authenticator or recovery code
      </label>
      <Input
        id={LOGIN_TWO_FACTOR_CODE_INPUT_ID}
        type="text"
        inputMode="text"
        maxLength={LOGIN_TWO_FACTOR_CODE_MAX_LENGTH}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={LOGIN_TWO_FACTOR_CODE_PLACEHOLDER}
        autoComplete="one-time-code"
        autoCapitalize="none"
        autoCorrect="off"
        aria-describedby={LOGIN_TWO_FACTOR_CODE_HELP_ID}
        disabled={disabled}
        autoFocus={autoFocus}
        required
      />
      <p id={LOGIN_TWO_FACTOR_CODE_HELP_ID} className={LOGIN_TWO_FACTOR_CODE_HELP_CLASS}>
        {LOGIN_TWO_FACTOR_CODE_HELP_COPY}
      </p>
    </div>
  );
}

export function LoginPage() {
  const navigate = useNavigate();
  const { login, loginWithPasskey, config } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [totpCode, setTotpCode] = useState("");
  const [needs2FA, setNeeds2FA] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const passkeysAvailable = isPasskeySupported();
  const nativeApp = isTauri();
  const passkeyLoginAvailable = passkeysAvailable || nativeApp;
  const passkeyLoginDisabledReason = getPasskeyLoginDisabledReason(
    email,
    passkeyLoginAvailable,
    loading,
  );
  const passwordLoginDisabledReason = getPasswordLoginDisabledReason(
    needs2FA,
    totpCode,
    loading,
  );

  const resetTwoFactorPrompt = () => {
    setNeeds2FA(false);
    setTotpCode("");
  };

  const handleEmailChange = (nextEmail: string) => {
    if (shouldResetTwoFactorPrompt(email, nextEmail)) {
      resetTwoFactorPrompt();
    }
    setError("");
    setEmail(nextEmail);
  };

  const handlePasswordChange = (nextPassword: string) => {
    if (shouldResetTwoFactorPrompt(password, nextPassword)) {
      resetTwoFactorPrompt();
    }
    setError("");
    setPassword(nextPassword);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const disabledReason = getPasswordLoginDisabledReason(needs2FA, totpCode, loading);
    if (disabledReason) {
      setError(disabledReason);
      return;
    }

    setError("");
    setLoading(true);

    try {
      await login(email, password, getLoginTwoFactorCodeForSubmit(needs2FA, totpCode));
      navigate("/");
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : "Login failed";
      if (isTwoFactorRequiredError(err)) {
        setNeeds2FA(true);
      } else {
        setError(message);
      }
    } finally {
      setLoading(false);
    }
  };

  const handlePasskeyLogin = async () => {
    if (nativeApp) {
      setLoading(true);
      try { await beginNativeAuthHandoff(); } catch (err) { setError(err instanceof Error ? err.message : "Unable to open browser sign-in."); } finally { setLoading(false); }
      return;
    }
    const disabledReason = getPasskeyLoginDisabledReason(email, passkeysAvailable, loading);
    if (disabledReason) {
      setError(disabledReason);
      return;
    }

    const normalizedEmail = email.trim();

    setError("");
    setLoading(true);
    try {
      await loginWithPasskey(normalizedEmail);
      navigate("/");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Passkey sign-in failed");
    } finally {
      setLoading(false);
    }
  };

  const handleBrowserSignIn = async (event: React.MouseEvent<HTMLAnchorElement>) => {
    if (!nativeApp) return;
    event.preventDefault();
    setLoading(true);
    try { await beginNativeAuthHandoff(); } catch (err) { setError(err instanceof Error ? err.message : "Unable to open browser sign-in."); } finally { setLoading(false); }
  };

  return (
    <main className="app-viewport app-background flex items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-8">
          <BrandLogo size="lg" />
          <h1 className="mt-6 text-xl font-semibold text-text">Sign in</h1>
        </div>

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
          <div>
            <label htmlFor="email" className="mb-1 block text-sm text-text-muted">
              Email
            </label>
            <Input
              id="email"
              type="email"
              value={email}
              onChange={(e) => handleEmailChange(e.target.value)}
              autoComplete="email"
              disabled={loading}
              required
              autoFocus
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
              onChange={(e) => handlePasswordChange(e.target.value)}
              autoComplete="current-password"
              disabled={loading}
              required
            />
          </div>

          {needs2FA && (
            <>
              <TwoFactorRequiredNotice />
              <TwoFactorLoginCodeInput
                value={totpCode}
                onChange={setTotpCode}
                disabled={loading}
                autoFocus
              />
            </>
          )}

          <Button
            type="submit"
            className="w-full"
            disabled={Boolean(passwordLoginDisabledReason)}
            aria-describedby={passwordLoginDisabledReason ? "login-submit-help" : undefined}
          >
            {loading ? "Signing in..." : "Sign in"}
          </Button>
          {passwordLoginDisabledReason && (
            <p
              id="login-submit-help"
              role="status"
              className="text-center text-xs leading-5 text-text-muted"
            >
              {passwordLoginDisabledReason}
            </p>
          )}
          {passkeyLoginAvailable && (
            <Button
              type="button"
              variant="secondary"
              size="lg"
              onClick={handlePasskeyLogin}
              disabled={Boolean(passkeyLoginDisabledReason)}
              className="h-10 w-full"
              aria-describedby={passkeyLoginDisabledReason ? "passkey-login-help" : undefined}
            >
              {nativeApp ? "Sign in with passkey in browser" : "Sign in with passkey"}
            </Button>
          )}
        </form>

        {passkeyLoginAvailable && passkeyLoginDisabledReason && (
          <p
            id="passkey-login-help"
            role="status"
            className="mt-2 text-center text-xs leading-5 text-text-muted"
          >
            {passkeyLoginDisabledReason}
          </p>
        )}
        {!passkeyLoginAvailable && <PasskeyUnavailableNotice />}

        <p className="mt-3 text-center text-sm text-text-muted">
          <Link to="/reset-password" className="font-medium text-primary underline underline-offset-2 hover:opacity-90">
            Forgot password?
          </Link>
        </p>

        {/* OAuth buttons — only shown if configured */}
        {(config?.google_enabled || config?.github_enabled || config?.oidc_enabled) && (
          <>
            <div className="my-4 flex items-center gap-3">
              <div className="h-px flex-1 bg-border" />
              <span className="text-xs text-text-muted">or</span>
              <div className="h-px flex-1 bg-border" />
            </div>

            <div className="space-y-2">
              {config?.google_enabled && (
                <a
                  href="/api/v1/auth/oauth/google"
                  onClick={handleBrowserSignIn}
                  className="flex h-9 w-full items-center justify-center gap-2 rounded-button border border-border bg-surface text-sm text-text transition-colors duration-100 hover:bg-surface-raised"
                >
                  Continue with Google
                </a>
              )}
              {config?.github_enabled && (
                <a
                  href="/api/v1/auth/oauth/github"
                  onClick={handleBrowserSignIn}
                  className="flex h-9 w-full items-center justify-center gap-2 rounded-button border border-border bg-surface text-sm text-text transition-colors duration-100 hover:bg-surface-raised"
                >
                  Continue with GitHub
                </a>
              )}
              {config?.oidc_enabled && (
                <a
                  href="/api/v1/auth/oidc"
                  onClick={handleBrowserSignIn}
                  className="flex h-9 w-full items-center justify-center gap-2 rounded-button border border-border bg-surface text-sm text-text transition-colors duration-100 hover:bg-surface-raised"
                >
                  Continue with {config.oidc_display_name}
                </a>
              )}
            </div>
          </>
        )}

        <p className="mt-4 text-center text-sm text-text-muted">
          Don't have an account?{" "}
          <Link to="/register" className="font-medium text-primary underline underline-offset-2 hover:opacity-90">
            Sign up
          </Link>
        </p>
      </div>
    </main>
  );
}
