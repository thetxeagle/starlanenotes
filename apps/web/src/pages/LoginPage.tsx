import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { HardDrive, Server } from "lucide-react";
import { useAuth } from "@/hooks/useAuth";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { BrandLogo } from "@/components/BrandLogo";
import { AUTH_ERROR_ALERT_CLASS } from "./setupShared";
import { isPasskeySupported } from "@/lib/passkeys";
import { api, isApiError, isTauri } from "@/lib/api";
import { beginNativeAuthHandoff } from "@/lib/nativeAuthHandoff";
import { clearStorageSelection, setStorageMode, setSyncProvider } from "@/lib/storageProviders";

export type LoginConnectionDestination = "local" | "server";

export function getLoginServerUrl(
  nativeApp: boolean,
  storedServerUrl?: string | null,
  webOrigin?: string,
): string {
  const browserAvailable = typeof window !== "undefined";
  if (nativeApp) {
    const serverUrl =
      storedServerUrl ?? (browserAvailable ? window.localStorage.getItem("server_url") : null);
    return serverUrl?.replace(/\/+$/, "") ?? "";
  }
  return webOrigin ?? (browserAvailable ? window.location.origin : "");
}

export function switchLoginConnection(destination: LoginConnectionDestination): void {
  if (typeof window === "undefined") return;

  api.setToken(null);
  api.setRefreshToken(null);
  window.localStorage.removeItem("server_url");
  setSyncProvider("none");

  if (destination === "local") {
    clearStorageSelection();
  } else {
    setStorageMode("server");
  }

  window.location.replace("/");
}

export function getLoginSuccessPath(search: string): string {
  const handoffId = new URLSearchParams(search).get("native_handoff");
  return handoffId ? `/app-signin/?handoff_id=${encodeURIComponent(handoffId)}` : "/";
}

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

export function isLikelyLoginEmail(value: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/u.test(value.trim());
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
      <label
        htmlFor={LOGIN_TWO_FACTOR_CODE_INPUT_ID}
        className="text-text-muted mb-1 block text-sm"
      >
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
  const location = useLocation();
  const { login, loginWithPasskey, config } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [totpCode, setTotpCode] = useState("");
  const [needs2FA, setNeeds2FA] = useState(false);
  const [emailConfirmed, setEmailConfirmed] = useState(false);
  const [passwordMode, setPasswordMode] = useState(false);
  const [emailOptions, setEmailOptions] = useState({
    password_available: false,
    passkey_available: false,
  });
  const [emailFlowOpen, setEmailFlowOpen] = useState(false);
  const [checkingEmailOptions, setCheckingEmailOptions] = useState(false);
  const emailOptionsRequest = useRef(0);
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
  const passwordLoginDisabledReason = getPasswordLoginDisabledReason(needs2FA, totpCode, loading);

  const resetTwoFactorPrompt = () => {
    setNeeds2FA(false);
    setTotpCode("");
  };

  const discoverEmailOptions = useCallback(async (value: string) => {
    const normalizedEmail = value.trim().toLowerCase();
    if (!isLikelyLoginEmail(normalizedEmail)) return;
    const requestId = ++emailOptionsRequest.current;
    setCheckingEmailOptions(true);
    setError("");
    try {
      const options = await api.fetch<{ password_available: boolean; passkey_available: boolean }>(
        "/auth/sign-in-options",
        { method: "POST", body: { email: normalizedEmail } },
      );
      if (requestId !== emailOptionsRequest.current) return;
      setEmailOptions(options);
      setEmailConfirmed(true);
      setPasswordMode(!options.passkey_available || !passkeyLoginAvailable);
    } catch (err) {
      if (requestId !== emailOptionsRequest.current) return;
      setError(err instanceof Error ? err.message : "Unable to check sign-in options.");
    } finally {
      if (requestId === emailOptionsRequest.current) setCheckingEmailOptions(false);
    }
  }, [passkeyLoginAvailable]);

  const handleEmailChange = (nextEmail: string) => {
    emailOptionsRequest.current += 1;
    setCheckingEmailOptions(false);
    if (shouldResetTwoFactorPrompt(email, nextEmail)) {
      resetTwoFactorPrompt();
    }
    setError("");
    setEmail(nextEmail);
    setEmailConfirmed(false);
    setPasswordMode(false);
    setEmailOptions({ password_available: false, passkey_available: false });
  };

  useEffect(() => {
    if (!isLikelyLoginEmail(email) || emailConfirmed) return;
    const timer = window.setTimeout(() => {
      void discoverEmailOptions(email);
    }, 350);
    return () => window.clearTimeout(timer);
  }, [discoverEmailOptions, email, emailConfirmed]);

  const handleEmailContinue = async (event: React.FormEvent) => {
    event.preventDefault();
    const normalizedEmail = email.trim().toLowerCase();
    if (!normalizedEmail) return;
    await discoverEmailOptions(normalizedEmail);
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
      navigate(getLoginSuccessPath(location.search));
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
      try {
        await beginNativeAuthHandoff();
      } catch (err) {
        setError(err instanceof Error ? err.message : "Unable to open browser sign-in.");
      } finally {
        setLoading(false);
      }
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
      navigate(getLoginSuccessPath(location.search));
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
    try {
      await beginNativeAuthHandoff();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to open browser sign-in.");
    } finally {
      setLoading(false);
    }
  };

  const ssoEnabled = Boolean(
    config?.google_enabled || config?.github_enabled || config?.oidc_enabled,
  );
  const canUsePasskey = emailConfirmed && emailOptions.passkey_available && passkeyLoginAvailable;
  const canUsePassword = emailConfirmed && emailOptions.password_available;
  const showCredentialStep = !ssoEnabled || emailFlowOpen;
  const serverUrl = getLoginServerUrl(nativeApp);
  const signupAvailable =
    config !== null && (!config.setup_completed || config.allow_public_signups);

  return (
    <main className="app-viewport app-background flex items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-8">
          <BrandLogo size="lg" />
          <h1 className="text-text mt-6 text-xl font-semibold">Sign in</h1>
          {nativeApp && serverUrl && (
            <div className="rounded-card border-border bg-surface mt-3 flex items-start gap-2 border px-3 py-2">
              <Server size={15} className="text-text-muted mt-0.5 shrink-0" aria-hidden="true" />
              <div className="min-w-0">
                <p className="text-text-muted text-xs">Connected server</p>
                <p className="text-text truncate text-sm" title={serverUrl}>
                  {serverUrl}
                </p>
              </div>
            </div>
          )}
        </div>

        {error && (
          <div role="alert" aria-live="assertive" className={`mb-4 ${AUTH_ERROR_ALERT_CLASS}`}>
            {error}
          </div>
        )}

        {ssoEnabled && !emailFlowOpen && (
          <div className="space-y-3">
            <p className="text-text-muted text-sm">
              Choose your organization sign-in, or continue with your email.
            </p>
            {config?.google_enabled && (
              <a
                href="/api/v1/auth/oauth/google"
                onClick={handleBrowserSignIn}
                className="rounded-button border-border bg-surface text-text hover:bg-surface-raised flex h-10 w-full items-center justify-center border text-sm"
              >
                Continue with Google
              </a>
            )}
            {config?.github_enabled && (
              <a
                href="/api/v1/auth/oauth/github"
                onClick={handleBrowserSignIn}
                className="rounded-button border-border bg-surface text-text hover:bg-surface-raised flex h-10 w-full items-center justify-center border text-sm"
              >
                Continue with GitHub
              </a>
            )}
            {config?.oidc_enabled && (
              <a
                href="/api/v1/auth/oidc"
                onClick={handleBrowserSignIn}
                className="rounded-button border-border bg-surface text-text hover:bg-surface-raised flex h-10 w-full items-center justify-center border text-sm"
              >
                Continue with {config.oidc_display_name}
              </a>
            )}
            <Button
              type="button"
              variant="secondary"
              className="w-full"
              onClick={() => setEmailFlowOpen(true)}
            >
              Sign in with email or password
            </Button>
          </div>
        )}

        {showCredentialStep && (
          <form
            onSubmit={emailConfirmed ? handleSubmit : handleEmailContinue}
            className="space-y-3"
            aria-busy={loading}
          >
            <div>
              <label htmlFor="email" className="text-text-muted mb-1 block text-sm">
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

            {!emailConfirmed && (
              <>
                <Button type="submit" className="w-full" disabled={loading || checkingEmailOptions}>
                  {checkingEmailOptions ? "Checking sign-in methods…" : loading ? "Checking…" : "Continue"}
                </Button>
                {checkingEmailOptions && (
                  <p role="status" className="text-text-muted text-center text-xs">
                    Finding the safest sign-in option for this account…
                  </p>
                )}
              </>
            )}

            {emailConfirmed && (
              <>
                <div className="text-text-muted flex items-center justify-between text-xs">
                  <span>Sign-in options found for this email.</span>
                  <button
                    type="button"
                    className="text-primary hover:underline"
                    onClick={() => {
                      setEmailConfirmed(false);
                      setPasswordMode(false);
                    }}
                  >
                    Change
                  </button>
                </div>

                {canUsePasskey && (
                  <Button
                    type="button"
                    variant="secondary"
                    size="lg"
                    onClick={handlePasskeyLogin}
                    disabled={Boolean(passkeyLoginDisabledReason)}
                    className="h-10 w-full"
                  >
                    {nativeApp ? "Sign in with passkey in browser" : "Sign in with passkey"}
                  </Button>
                )}

                {canUsePasskey && canUsePassword && !passwordMode && (
                  <button
                    type="button"
                    className="text-primary mx-auto block text-xs hover:underline"
                    onClick={() => setPasswordMode(true)}
                  >
                    Use password instead
                  </button>
                )}
                {canUsePasskey && canUsePassword && passwordMode && (
                  <button
                    type="button"
                    className="text-primary mx-auto block text-xs hover:underline"
                    onClick={() => setPasswordMode(false)}
                  >
                    Use passkey instead
                  </button>
                )}

                {((canUsePassword && passwordMode) || !canUsePasskey) && (
                  <>
                    <div>
                      <label htmlFor="password" className="text-text-muted mb-1 block text-sm">
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
                      aria-describedby={
                        passwordLoginDisabledReason ? "login-submit-help" : undefined
                      }
                    >
                      {loading ? "Signing in..." : "Sign in"}
                    </Button>
                    {passwordLoginDisabledReason && (
                      <p
                        id="login-submit-help"
                        role="status"
                        className="text-text-muted text-center text-xs leading-5"
                      >
                        {passwordLoginDisabledReason}
                      </p>
                    )}
                  </>
                )}
              </>
            )}
          </form>
        )}

        {canUsePasskey && passkeyLoginDisabledReason && (
          <p
            id="passkey-login-help"
            role="status"
            className="text-text-muted mt-2 text-center text-xs leading-5"
          >
            {passkeyLoginDisabledReason}
          </p>
        )}
        {emailConfirmed && !passkeyLoginAvailable && emailOptions.passkey_available && (
          <PasskeyUnavailableNotice />
        )}

        <p className="text-text-muted mt-3 text-center text-sm">
          <Link
            to="/reset-password"
            className="text-primary font-medium underline underline-offset-2 hover:opacity-90"
          >
            Forgot password?
          </Link>
        </p>

        {signupAvailable && (
          <p className="text-text-muted mt-4 text-center text-sm">
            Don't have an account?{" "}
            <Link
              to="/register"
              className="text-primary font-medium underline underline-offset-2 hover:opacity-90"
            >
              Sign up
            </Link>
          </p>
        )}

        {nativeApp && (
          <div className="border-border mt-6 grid grid-cols-1 gap-2 border-t pt-4 sm:grid-cols-2">
            <Button
              type="button"
              variant="secondary"
              size="sm"
              className="w-full"
              onClick={() => switchLoginConnection("local")}
            >
              <HardDrive size={15} aria-hidden="true" />
              Use local vault
            </Button>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              className="w-full"
              onClick={() => switchLoginConnection("server")}
            >
              <Server size={15} aria-hidden="true" />
              Change server
            </Button>
          </div>
        )}
      </div>
    </main>
  );
}
