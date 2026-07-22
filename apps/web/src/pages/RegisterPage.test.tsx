import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AuthConfig } from "@/hooks/useAuth";
import { PUBLIC_SIGNUPS_DISABLED_COPY, RegisterPage } from "./RegisterPage";

const registerMock = vi.fn();
const authState = vi.hoisted(() => ({
  config: null as AuthConfig | null,
}));

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    user: null,
    config: authState.config,
    loading: false,
    login: vi.fn(),
    loginWithPasskey: vi.fn(),
    register: registerMock,
    updateCurrentUser: vi.fn(),
    updateProfile: vi.fn(),
    logout: vi.fn(),
  }),
}));

const baseConfig: AuthConfig = {
  google_enabled: false,
  github_enabled: false,
  oidc_enabled: false,
  oidc_display_name: "SSO",
  require_2fa: false,
  allow_public_signups: false,
  setup_completed: true,
  site_name: "Starlane Notes",
  max_upload_size_mb: 100,
};

function renderRegisterPage() {
  return renderToStaticMarkup(
    <MemoryRouter>
      <RegisterPage />
    </MemoryRouter>,
  );
}

describe("RegisterPage", () => {
  afterEach(() => {
    authState.config = null;
    registerMock.mockReset();
  });

  it("shows invite-only copy instead of the form when public signups are disabled after setup", () => {
    authState.config = { ...baseConfig, setup_completed: true, allow_public_signups: false };

    const html = renderRegisterPage();

    expect(html).toContain("Account creation is invite only");
    expect(html).toContain(PUBLIC_SIGNUPS_DISABLED_COPY);
    expect(html).not.toContain("id=\"email\"");
  });

  it("keeps first-user setup registration available before setup is complete", () => {
    authState.config = { ...baseConfig, setup_completed: false, allow_public_signups: false };

    const html = renderRegisterPage();

    expect(html).toContain("Create your account");
    expect(html).toContain("id=\"email\"");
  });

  it("allows the form when public signups are explicitly enabled", () => {
    authState.config = { ...baseConfig, setup_completed: true, allow_public_signups: true };

    const html = renderRegisterPage();

    expect(html).toContain("Create your account");
    expect(html).toContain("id=\"email\"");
    expect(html).not.toContain(PUBLIC_SIGNUPS_DISABLED_COPY);
  });
});
