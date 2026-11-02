import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import {
  canSubmitPasskeyLogin,
  getLoginServerUrl,
  getLoginSuccessPath,
  getLoginTwoFactorCodeForSubmit,
  isLikelyLoginEmail,
  getPasskeyLoginDisabledReason,
  getPasswordLoginDisabledReason,
  isLoginTwoFactorCodeFormat,
  isTwoFactorRequiredError,
  LOGIN_PASSKEY_UNAVAILABLE_NOTICE_CLASS,
  LOGIN_PASSKEY_UNAVAILABLE_NOTICE_COPY,
  LOGIN_TWO_FACTOR_CODE_HELP_CLASS,
  LOGIN_TWO_FACTOR_CODE_HELP_COPY,
  LOGIN_TWO_FACTOR_CODE_HELP_ID,
  LOGIN_TWO_FACTOR_CODE_INPUT_ID,
  LOGIN_TWO_FACTOR_CODE_MAX_LENGTH,
  LOGIN_TWO_FACTOR_CODE_PLACEHOLDER,
  LOGIN_TWO_FACTOR_REQUIRED_NOTICE_CLASS,
  LOGIN_TWO_FACTOR_REQUIRED_NOTICE_COPY,
  LoginPage,
  PasskeyUnavailableNotice,
  shouldResetTwoFactorPrompt,
  switchLoginConnection,
  TwoFactorLoginCodeInput,
  TwoFactorRequiredNotice,
} from "./LoginPage";
import { api, ApiError } from "@/lib/api";

const passkeySupportMock = vi.hoisted(() => ({
  supported: false,
}));
const nativeAppMock = vi.hoisted(() => ({ native: false }));
const authConfigMock = vi.hoisted(() => ({
  config: {
    google_enabled: false,
    github_enabled: false,
    oidc_enabled: false,
    setup_completed: true,
    allow_public_signups: false,
  },
}));

vi.mock("@/lib/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api")>()),
  isTauri: () => nativeAppMock.native,
}));

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    login: vi.fn(),
    loginWithPasskey: vi.fn(),
    config: authConfigMock.config,
  }),
}));

vi.mock("@/lib/passkeys", () => ({
  isPasskeySupported: () => passkeySupportMock.supported,
}));

function renderLoginPage(): string {
  return renderToStaticMarkup(
    <MemoryRouter>
      <LoginPage />
    </MemoryRouter>,
  );
}

function createMemoryStorage(entries: Record<string, string> = {}): Storage {
  const values = new Map(Object.entries(entries));
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => values.delete(key),
    setItem: (key, value) => values.set(key, value),
  };
}

describe("LoginPage passkey affordance", () => {
  it("recognizes complete email addresses for automatic credential discovery", () => {
    expect(isLikelyLoginEmail("eagle@example.test")).toBe(true);
    expect(isLikelyLoginEmail(" eagle@example.test ")).toBe(true);
    expect(isLikelyLoginEmail("eagle@example")).toBe(false);
    expect(isLikelyLoginEmail("eagle example.test")).toBe(false);
  });

  it("resolves the active web server and hides signup when public registration is disabled", () => {
    nativeAppMock.native = false;
    authConfigMock.config.allow_public_signups = false;

    const html = renderLoginPage();

    expect(getLoginServerUrl(false, null, "https://notes.example.test")).toBe(
      "https://notes.example.test",
    );
    expect(html).not.toContain("Don't have an account?");
    expect(html).not.toContain('href="/register"');
  });

  it("shows signup only when the connected server explicitly enables public registration", () => {
    authConfigMock.config.allow_public_signups = true;

    const html = renderLoginPage();

    expect(html).toContain('href="/register"');
    expect(html).toContain("Sign up");
    authConfigMock.config.allow_public_signups = false;
  });

  it("keeps initial owner registration reachable before server setup is complete", () => {
    authConfigMock.config.setup_completed = false;
    authConfigMock.config.allow_public_signups = false;

    const html = renderLoginPage();

    expect(html).toContain('href="/register"');
    authConfigMock.config.setup_completed = true;
  });

  it("shows native controls for switching to a local vault or another server", () => {
    nativeAppMock.native = true;

    const html = renderLoginPage();

    expect(getLoginServerUrl(true, "https://notes.example.test/")).toBe(
      "https://notes.example.test",
    );
    expect(html).toContain("Use local vault");
    expect(html).toContain("Change server");

    nativeAppMock.native = false;
  });

  it("keeps native connection controls out of the web sign-in surface", () => {
    nativeAppMock.native = false;

    const html = renderLoginPage();

    expect(html).not.toContain("Use local vault");
    expect(html).not.toContain("Change server");
  });

  it("clears prior credentials and routes native connection changes without deleting vault data", () => {
    const localStorage = createMemoryStorage({
      server_url: "https://old.example.test",
      starlane_storage_mode: "server",
      starlane_sync_provider: "google-drive",
      preserved_vault_record: "keep",
    });
    const replace = vi.fn();
    vi.stubGlobal("window", {
      localStorage,
      location: { origin: "https://old.example.test", replace },
    });
    vi.stubGlobal("localStorage", localStorage);
    const tokenSpy = vi.spyOn(api, "setToken").mockImplementation(() => undefined);
    const refreshSpy = vi.spyOn(api, "setRefreshToken").mockImplementation(() => undefined);

    switchLoginConnection("server");

    expect(tokenSpy).toHaveBeenCalledWith(null);
    expect(refreshSpy).toHaveBeenCalledWith(null);
    expect(localStorage.getItem("server_url")).toBeNull();
    expect(localStorage.getItem("starlane_storage_mode")).toBe("server");
    expect(localStorage.getItem("starlane_sync_provider")).toBe("none");
    expect(localStorage.getItem("preserved_vault_record")).toBe("keep");
    expect(replace).toHaveBeenCalledWith("/");

    switchLoginConnection("local");

    expect(localStorage.getItem("starlane_storage_mode")).toBeNull();
    expect(localStorage.getItem("starlane_sync_provider")).toBeNull();
    expect(localStorage.getItem("preserved_vault_record")).toBe("keep");

    tokenSpy.mockRestore();
    refreshSpy.mockRestore();
    vi.unstubAllGlobals();
  });

  it("returns browser handoffs to the dedicated app sign-in page after login", () => {
    expect(getLoginSuccessPath("")).toBe("/");
    expect(getLoginSuccessPath("?native_handoff=handoff-1")).toBe(
      "/app-signin/?handoff_id=handoff-1",
    );
  });

  it("starts with the email identity step when passkey support is unavailable", () => {
    passkeySupportMock.supported = false;
    nativeAppMock.native = false;

    const html = renderLoginPage();

    expect(html).toContain("Continue");
    expect(html).toContain('id="email"');
    expect(html).not.toContain('id="password"');
  });

  it("keeps the native browser-passkey path behind email discovery", () => {
    passkeySupportMock.supported = false;
    nativeAppMock.native = true;
    const html = renderLoginPage();
    expect(html).toContain("Continue");
    expect(html).toContain('id="email"');
    nativeAppMock.native = false;
  });

  it("uses an email-first passkey discovery step when passkeys are supported", () => {
    passkeySupportMock.supported = true;

    const html = renderLoginPage();

    expect(html).toContain("Continue");
    expect(html).toContain('id="email"');
    expect(html).not.toContain('id="password"');
    expect(html).toContain("rounded-button");
    expect(html).toContain("focus-ring");
    expect(html).toContain("font-medium");
    expect(html).toContain("border-border");
    expect(html).toContain("bg-surface");
    expect(html).toContain("text-text-muted");
    expect(html).not.toContain("Passkey sign-in is unavailable");
  });

  it("requires passkey support, an email, and no active loading state before submitting", () => {
    expect(canSubmitPasskeyLogin("", true, false)).toBe(false);
    expect(canSubmitPasskeyLogin("   ", true, false)).toBe(false);
    expect(canSubmitPasskeyLogin("eagle@example.test", false, false)).toBe(false);
    expect(canSubmitPasskeyLogin("eagle@example.test", true, true)).toBe(false);
    expect(canSubmitPasskeyLogin(" eagle@example.test ", true, false)).toBe(true);
  });

  it("explains why passkey sign-in is disabled", () => {
    expect(getPasskeyLoginDisabledReason("eagle@example.test", false, false)).toBe(
      "Passkey sign-in is unavailable in this browser or webview.",
    );
    expect(getPasskeyLoginDisabledReason("eagle@example.test", true, true)).toBe(
      "Passkey sign-in is paused while another sign-in is in progress.",
    );
    expect(getPasskeyLoginDisabledReason("", true, false)).toBe(
      "Enter your email first so Starlane can find your saved passkey.",
    );
    expect(getPasskeyLoginDisabledReason("   ", true, false)).toBe(
      "Enter your email first so Starlane can find your saved passkey.",
    );
    expect(getPasskeyLoginDisabledReason("eagle@example.test", true, false)).toBeNull();
  });

  it("explains why password sign-in is disabled after the 2FA prompt appears", () => {
    expect(getPasswordLoginDisabledReason(false, "", false)).toBeNull();
    expect(getPasswordLoginDisabledReason(true, "", false)).toBe(
      "Enter an authenticator or recovery code to continue.",
    );
    expect(getPasswordLoginDisabledReason(true, "   ", false)).toBe(
      "Enter an authenticator or recovery code to continue.",
    );
    expect(getPasswordLoginDisabledReason(true, "12345", false)).toBe(
      "Enter a 6-digit authenticator code or recovery code like 1234-5678.",
    );
    expect(getPasswordLoginDisabledReason(true, "12345a", false)).toBe(
      "Enter a 6-digit authenticator code or recovery code like 1234-5678.",
    );
    expect(getPasswordLoginDisabledReason(true, "123456", false)).toBeNull();
    expect(getPasswordLoginDisabledReason(true, " 123456 ", false)).toBeNull();
    expect(getPasswordLoginDisabledReason(true, "1234-5678", false)).toBeNull();
    expect(getPasswordLoginDisabledReason(true, "12345678", false)).toBeNull();
    expect(getPasswordLoginDisabledReason(false, "", true)).toBe(
      "Sign-in is already in progress.",
    );
  });

  it("accepts only login 2FA authenticator and recovery-code formats", () => {
    expect(isLoginTwoFactorCodeFormat("123456")).toBe(true);
    expect(isLoginTwoFactorCodeFormat("1234-5678")).toBe(true);
    expect(isLoginTwoFactorCodeFormat("12345678")).toBe(true);
    expect(isLoginTwoFactorCodeFormat("12345")).toBe(false);
    expect(isLoginTwoFactorCodeFormat("1234567")).toBe(false);
    expect(isLoginTwoFactorCodeFormat("1234 5678")).toBe(false);
    expect(isLoginTwoFactorCodeFormat("1234-567a")).toBe(false);
  });

  it("normalizes authenticator or recovery codes before password login submission", () => {
    expect(getLoginTwoFactorCodeForSubmit(false, " 123456 ")).toBeUndefined();
    expect(getLoginTwoFactorCodeForSubmit(true, " 123456 ")).toBe("123456");
    expect(getLoginTwoFactorCodeForSubmit(true, " 1234-5678 ")).toBe("1234-5678");
  });

  it("resets the 2FA prompt when primary login credentials change", () => {
    expect(shouldResetTwoFactorPrompt("eagle@example.test", "eagle@example.test")).toBe(false);
    expect(shouldResetTwoFactorPrompt("eagle@example.test", "tucker@example.test")).toBe(true);
    expect(shouldResetTwoFactorPrompt("old-password", "new-password")).toBe(true);
  });

  it("detects documented 2FA-required login errors without treating generic failures as setup prompts", () => {
    expect(isTwoFactorRequiredError(new ApiError("2FA code required", 403, "2fa_required"))).toBe(
      true,
    );
    expect(isTwoFactorRequiredError(new Error("2fa_required: Two-factor authentication required"))).toBe(
      true,
    );
    expect(isTwoFactorRequiredError(new Error("2FA required"))).toBe(true);
    expect(isTwoFactorRequiredError(new Error("2FA code required"))).toBe(true);
    expect(isTwoFactorRequiredError(new Error("Two-factor authentication required"))).toBe(true);
    expect(isTwoFactorRequiredError(new ApiError("Invalid credentials", 401, "unauthorized"))).toBe(
      false,
    );
    expect(isTwoFactorRequiredError(new Error("invalid credentials"))).toBe(false);
  });

  it("shows a clear two-factor-required notice with Starlane surface tokens", () => {
    const html = renderToStaticMarkup(<TwoFactorRequiredNotice />);

    expect(html).toContain('role="status"');
    expect(html).toContain(LOGIN_TWO_FACTOR_REQUIRED_NOTICE_COPY);
    expect(html).toContain("authenticator or recovery code");
    expect(html).toContain(LOGIN_TWO_FACTOR_REQUIRED_NOTICE_CLASS);
    expect(html).not.toMatch(/bg-blue-|border-gray-|rounded-xl|text-orange-/);
  });

  it("explains unavailable passkey sign-in with Starlane surface tokens", () => {
    const html = renderToStaticMarkup(<PasskeyUnavailableNotice />);

    expect(html).toContain('role="status"');
    expect(html).toContain(LOGIN_PASSKEY_UNAVAILABLE_NOTICE_COPY);
    expect(html).toContain("Use email and password instead.");
    expect(html).toContain(LOGIN_PASSKEY_UNAVAILABLE_NOTICE_CLASS);
    expect(html).not.toMatch(/bg-blue-|border-gray-|rounded-xl|text-orange-/);
  });

  it("lets the 2FA prompt accept either authenticator codes or recovery codes", () => {
    const html = renderToStaticMarkup(
      <TwoFactorLoginCodeInput value="" onChange={() => undefined} autoFocus />,
    );

    expect(html).toContain("Authenticator or recovery code");
    expect(html).toContain(`id="${LOGIN_TWO_FACTOR_CODE_INPUT_ID}"`);
    expect(html).toContain(LOGIN_TWO_FACTOR_CODE_PLACEHOLDER);
    expect(html).toContain(`maxLength="${LOGIN_TWO_FACTOR_CODE_MAX_LENGTH}"`);
    expect(html).toContain("inputMode=\"text\"");
    expect(html).toContain("autoComplete=\"one-time-code\"");
    expect(html).toContain("autoCapitalize=\"none\"");
    expect(html).toContain("autoCorrect=\"off\"");
    expect(html).toContain(LOGIN_TWO_FACTOR_CODE_HELP_COPY);
    expect(html).toContain(`aria-describedby="${LOGIN_TWO_FACTOR_CODE_HELP_ID}"`);
    expect(html).toContain(`id="${LOGIN_TWO_FACTOR_CODE_HELP_ID}"`);
    expect(html).toContain(LOGIN_TWO_FACTOR_CODE_HELP_CLASS);
    expect(html).not.toContain("pattern=");
  });
});
